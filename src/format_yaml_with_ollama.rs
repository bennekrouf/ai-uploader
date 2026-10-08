use graflog::app_log;
use reqwest::Client;
use std::env;
use std::error::Error;
use std::fs;

use crate::{
    extract_yaml::extract_yaml,
    load_prompt::load_prompt,
    models::{ChatMessage, ChatRequest, ChatResponse},
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

pub async fn format_yaml(
    provider: &Provider,
    input_file_path: &str,
    template_file_path: &str,
    system_prompt_path: &str,
    user_prompt_path: &str,
    model: &str,
    api_key: Option<&str>,
) -> Result<String, Box<dyn Error>> {
    // The key set in the dashboard, sent by the store; this process's
    // environment only when none was set there.
    dotenv::dotenv().ok();
    let api_key = match api_key {
        Some(k) => k.to_string(),
        None => env::var(provider.key_var).map_err(|_| {
            format!(
                "no {} API key: set one in the dashboard (Admin → YAML import), or {} on this server",
                provider.label, provider.key_var
            )
        })?,
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
        .post(provider.endpoint)
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
    Ok(fixed_yaml)
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
