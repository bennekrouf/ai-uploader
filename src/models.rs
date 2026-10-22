use serde::{Deserialize, Serialize};

// ── Chat completions (DeepSeek, Mistral — both OpenAI-compatible) ────────────

#[derive(Serialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f64>,
}

#[derive(Serialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Deserialize, Debug)]
pub struct ChatResponse {
    pub choices: Vec<ChatChoice>,
    #[serde(default)]
    pub usage: Option<ChatUsage>,
}

/// The tokens a completion spent, as the provider counts them.
#[derive(Deserialize, Debug, Clone, Copy, Default)]
pub struct ChatUsage {
    #[serde(default)]
    pub prompt_tokens: i64,
    #[serde(default)]
    pub completion_tokens: i64,
}

#[derive(Deserialize, Debug)]
pub struct ChatChoice {
    pub message: ChatRespMessage,
}

#[derive(Deserialize, Debug)]
pub struct ChatRespMessage {
    pub content: String,
}
