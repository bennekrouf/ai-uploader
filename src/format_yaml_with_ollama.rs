use graflog::app_log;
use reqwest::Client;
use std::env;
use std::error::Error;
use std::fs;

use crate::{
    extract_yaml::extract_yaml,
    load_prompt::load_prompt,
    models::{ChatMessage, ChatRequest, ChatResponse, ChatUsage},
    yaml_validator,
};

/// An AI provider the uploader can format with. Both speak the OpenAI-style
/// chat-completions API, so one call serves them. DeepSeek, first, is the
/// default; Mistral the alternative.
pub struct Provider {
    pub id: &'static str,
    pub label: &'static str,
    pub endpoint: &'static str,
    pub key_var: &'static str,
}

pub const PROVIDERS: &[Provider] = &[
    Provider {
        id: "deepseek",
        label: "DeepSeek",
        endpoint: "https://api.deepseek.com/v1/chat/completions",
        key_var: "DEEPSEEK_API_KEY",
    },
    Provider {
        id: "mistral",
        label: "Mistral",
        endpoint: "https://api.mistral.ai/v1/chat/completions",
        key_var: "MISTRAL_API_KEY",
    },
];

/// The provider for `id`; the default for anything else, such as a Cohere or
/// Claude setting saved before those were dropped.
pub fn provider(id: &str) -> &'static Provider {
    PROVIDERS.iter().find(|p| p.id == id).unwrap_or(&PROVIDERS[0])
}

/// Where one request goes. The store sends the provider's chat URL with the
/// file, so the import runs on any OpenAI-style provider the super admin picks
/// in the dashboard (Admin → AI models), not only the two known here. Without
/// a URL, an unknown id falls back to the default provider, as before.
pub struct Upstream {
    pub id: String,
    pub label: String,
    pub endpoint: String,
    /// The environment variable holding a key, for a provider known here.
    pub key_var: Option<&'static str>,
}

pub fn upstream(id: &str, chat_url: Option<&str>) -> Upstream {
    let known = PROVIDERS.iter().find(|p| p.id == id);
    match (known, chat_url.filter(|u| u.starts_with("https://"))) {
        (Some(p), url) => Upstream {
            id: p.id.into(),
            label: p.label.into(),
            endpoint: url.unwrap_or(p.endpoint).into(),
            key_var: Some(p.key_var),
        },
        (None, Some(url)) => Upstream { id: id.into(), label: id.into(), endpoint: url.into(), key_var: None },
        (None, None) => {
            let p = provider(id);
            Upstream { id: p.id.into(), label: p.label.into(), endpoint: p.endpoint.into(), key_var: Some(p.key_var) }
        }
    }
}

/// The formatted text and what the call spent; `usage` is None when the
/// provider left it out.
pub struct Formatted {
    pub text: String,
    pub usage: Option<ChatUsage>,
}

impl Formatted {
    /// The call's model and tokens as response headers, so the store can put
    /// the call on the platform's LLM ledger without the body changing shape.
    pub fn usage_headers(&self, provider: &Upstream, model: &str) -> Vec<(&'static str, String)> {
        let mut headers = vec![("X-LLM-Provider", provider.id.to_string()), ("X-LLM-Model", model.to_string())];
        if let Some(u) = self.usage {
            headers.push(("X-LLM-Input-Tokens", u.prompt_tokens.to_string()));
            headers.push(("X-LLM-Output-Tokens", u.completion_tokens.to_string()));
        }
        headers
    }
}

pub async fn format_yaml(
    provider: &Upstream,
    input_file_path: &str,
    template_file_path: &str,
    system_prompt_path: &str,
    user_prompt_path: &str,
    model: &str,
    api_key: Option<&str>,
) -> Result<Formatted, Box<dyn Error>> {
    // The key set in the dashboard, sent by the store; this process's
    // environment only when none was set there.
    dotenv::dotenv().ok();
    let api_key = match (api_key, provider.key_var) {
        (Some(k), _) => k.to_string(),
        (None, Some(var)) => env::var(var).map_err(|_| {
            format!(
                "no {} API key: set one in the dashboard (Admin → AI models), or {} on this server",
                provider.label, var
            )
        })?,
        (None, None) => {
            return Err(format!("no {} API key: set one in the dashboard (Admin → AI models)", provider.label).into())
        }
    };

    let (system_prompt, user_prompt) =
        build_prompts(input_file_path, template_file_path, system_prompt_path, user_prompt_path)?;

    let client = Client::new();
    let request = ChatRequest {
        model: model.to_string(),
        messages: vec![
            ChatMessage { role: "system".to_string(), content: system_prompt },
            ChatMessage { role: "user".to_string(), content: user_prompt },
        ],
        max_tokens: Some(4000),
        temperature: Some(0.1),
    };

    app_log!(info, "Calling {} API with model {}", provider.label, model);
    let resp = client
        .post(&provider.endpoint)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&request)
        .send()
        .await?;

    if !resp.status().is_success() {
        let error_text = resp.text().await?;
        app_log!(error, "Failed to call {}: {}", provider.label, error_text);
        return Err(format!("{} API error: {}", provider.label, error_text).into());
    }

    let chat_response: ChatResponse = resp.json().await?;
    let text = chat_response
        .choices
        .first()
        .map(|c| c.message.content.clone())
        .unwrap_or_default();
    app_log!(info, "Received response from {}", provider.label);

    let yaml_content = extract_yaml(&text);
    let fixed_yaml = yaml_validator::validate_and_fix_yaml(&yaml_content)?;
    Ok(Formatted { text: fixed_yaml, usage: chat_response.usage })
}

fn build_prompts(
    input_file_path: &str,
    template_file_path: &str,
    system_prompt_path: &str,
    user_prompt_path: &str,
) -> Result<(String, String), Box<dyn Error>> {
    let input_content = fs::read_to_string(input_file_path)?;
    let template_content = fs::read_to_string(template_file_path)?;
    let system_prompt = load_prompt(system_prompt_path)?;
    let user_prompt_template = load_prompt(user_prompt_path)?;

    let user_prompt = user_prompt_template
        .replace("{INPUT_CONTENT}", &input_content)
        .replace("{TEMPLATE_CONTENT}", &template_content);

    Ok((system_prompt, user_prompt))
}
