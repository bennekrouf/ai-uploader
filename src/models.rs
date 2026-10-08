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
}

#[derive(Deserialize, Debug)]
pub struct ChatChoice {
    pub message: ChatRespMessage,
}

#[derive(Deserialize, Debug)]
pub struct ChatRespMessage {
    pub content: String,
}
