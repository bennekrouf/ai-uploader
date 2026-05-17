use graflog::app_log;
use reqwest::Client;
use std::env;
use std::error::Error;
use std::fs;

use crate::{
    extract_yaml::extract_yaml,
    load_prompt::load_prompt,
    models::{ChatMessage, CohereRequest, CohereResponse},
    yaml_validator,
};

pub async fn format_yaml_with_cohere(
    input_file_path: &str,
    template_file_path: &str,
    system_prompt_path: &str,
    user_prompt_path: &str,
    model: &str,
) -> Result<String, Box<dyn Error>> {
    // Load environment variables
    dotenv::dotenv().ok();
    let api_key = env::var("COHERE_API_KEY")
        .map_err(|_| "COHERE_API_KEY not found in environment variables")?;

    // Read files (same as before)
    let input_content = fs::read_to_string(input_file_path)?;
    let template_content = fs::read_to_string(template_file_path)?;
    let system_prompt = load_prompt(system_prompt_path)?;
    let user_prompt_template = load_prompt(user_prompt_path)?;

    let user_prompt = user_prompt_template
        .replace("{INPUT_CONTENT}", &input_content)
        .replace("{TEMPLATE_CONTENT}", &template_content);

    // Prepare Cohere request
    let client = Client::new();
    let request = CohereRequest {
        model: model.to_string(),
        // model:     "command".to_string(), // or "command-nightly", "command-light"
        message: user_prompt,
        max_tokens: Some(4000),
        temperature: Some(0.1),
        chat_history: vec![ChatMessage {
            role: "SYSTEM".to_string(),
            message: system_prompt,
        }],
    };

    app_log!(info, "Calling Cohere API");
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

    // Extract and validate YAML (same as before)
    let yaml_content = extract_yaml(&cohere_response.text);
    let fixed_yaml = yaml_validator::validate_and_fix_yaml(&yaml_content)?;

    Ok(fixed_yaml)
}
