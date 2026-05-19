use graflog::app_log;
use reqwest::Client;
use std::env;
use std::error::Error;
use std::fs;

use crate::{
    extract_yaml::extract_yaml,
    load_prompt::load_prompt,
    models::{
        ChatMessage, CohereRequest, CohereResponse,
        DeepSeekMessage, DeepSeekRequest, DeepSeekResponse,
        ClaudeMessage, ClaudeRequest, ClaudeResponse,
    },
    yaml_validator,
};

pub async fn format_yaml_with_cohere(
    input_file_path: &str,
    template_file_path: &str,
    system_prompt_path: &str,
    user_prompt_path: &str,
    model: &str,
) -> Result<String, Box<dyn Error>> {
    dotenv::dotenv().ok();
    let api_key = env::var("COHERE_API_KEY")
        .map_err(|_| "COHERE_API_KEY not found in environment variables")?;

    let (system_prompt, user_prompt) =
        build_prompts(input_file_path, template_file_path, system_prompt_path, user_prompt_path)?;

    let client = Client::new();
    let request = CohereRequest {
        model: model.to_string(),
        message: user_prompt,
        max_tokens: Some(4000),
        temperature: Some(0.1),
        chat_history: vec![ChatMessage {
            role: "SYSTEM".to_string(),
            message: system_prompt,
        }],
    };

    app_log!(info, "Calling Cohere API with model {}", model);
    let resp = client
        .post("https://api.cohere.ai/v1/chat")
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&request)
        .send()
        .await?;

    if !resp.status().is_success() {
        let error_text = resp.text().await?;
        app_log!(error, "Failed to call Cohere: {}", error_text);
        return Err(format!("Cohere API error: {}", error_text).into());
    }

    let cohere_response: CohereResponse = resp.json().await?;
    app_log!(info, "Received response from Cohere");

    let yaml_content = extract_yaml(&cohere_response.text);
    let fixed_yaml = yaml_validator::validate_and_fix_yaml(&yaml_content)?;
    Ok(fixed_yaml)
}

pub async fn format_yaml_with_deepseek(
    input_file_path: &str,
    template_file_path: &str,
    system_prompt_path: &str,
    user_prompt_path: &str,
    model: &str,
) -> Result<String, Box<dyn Error>> {
    dotenv::dotenv().ok();
    let api_key = env::var("DEEPSEEK_API_KEY")
        .map_err(|_| "DEEPSEEK_API_KEY not found in environment variables")?;

    let (system_prompt, user_prompt) =
        build_prompts(input_file_path, template_file_path, system_prompt_path, user_prompt_path)?;

    let client = Client::new();
    let request = DeepSeekRequest {
        model: model.to_string(),
        messages: vec![
            DeepSeekMessage { role: "system".to_string(), content: system_prompt },
            DeepSeekMessage { role: "user".to_string(), content: user_prompt },
        ],
        max_tokens: Some(4000),
        temperature: Some(0.1),
    };

    app_log!(info, "Calling DeepSeek API with model {}", model);
    let resp = client
        .post("https://api.deepseek.com/v1/chat/completions")
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&request)
        .send()
        .await?;

    if !resp.status().is_success() {
        let error_text = resp.text().await?;
        app_log!(error, "Failed to call DeepSeek: {}", error_text);
        return Err(format!("DeepSeek API error: {}", error_text).into());
    }

    let ds_response: DeepSeekResponse = resp.json().await?;
    let text = ds_response
        .choices
        .first()
        .map(|c| c.message.content.clone())
        .unwrap_or_default();
    app_log!(info, "Received response from DeepSeek");

    let yaml_content = extract_yaml(&text);
    let fixed_yaml = yaml_validator::validate_and_fix_yaml(&yaml_content)?;
    Ok(fixed_yaml)
}

pub async fn format_yaml_with_claude(
    input_file_path: &str,
    template_file_path: &str,
    system_prompt_path: &str,
    user_prompt_path: &str,
    model: &str,
) -> Result<String, Box<dyn Error>> {
    dotenv::dotenv().ok();
    let api_key = env::var("CLAUDE_API_KEY")
        .map_err(|_| "CLAUDE_API_KEY not found in environment variables")?;

    let (system_prompt, user_prompt) =
        build_prompts(input_file_path, template_file_path, system_prompt_path, user_prompt_path)?;

    let client = Client::new();
    let request = ClaudeRequest {
        model: model.to_string(),
        max_tokens: 4000,
        system: system_prompt,
        messages: vec![ClaudeMessage {
            role: "user".to_string(),
            content: user_prompt,
        }],
    };

    app_log!(info, "Calling Claude API with model {}", model);
    let resp = client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", &api_key)
        .header("anthropic-version", "2023-06-01")
        .header("Content-Type", "application/json")
        .json(&request)
        .send()
        .await?;

    if !resp.status().is_success() {
        let error_text = resp.text().await?;
        app_log!(error, "Failed to call Claude: {}", error_text);
        return Err(format!("Claude API error: {}", error_text).into());
    }

    let claude_response: ClaudeResponse = resp.json().await?;
    let text = claude_response
        .content
        .first()
        .map(|b| b.text.clone())
        .unwrap_or_default();
    app_log!(info, "Received response from Claude");

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
