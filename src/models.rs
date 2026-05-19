use serde::{Deserialize, Serialize};

// ── Cohere ───────────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct CohereRequest {
    pub model: String,
    pub message: String,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f64>,
    pub chat_history: Vec<ChatMessage>,
}

#[derive(Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub message: String,
}

#[derive(Deserialize, Debug)]
pub struct CohereResponse {
    pub text: String,
}

// ── DeepSeek (OpenAI-compatible) ─────────────────────────────────────────────

#[derive(Serialize)]
pub struct DeepSeekRequest {
    pub model: String,
    pub messages: Vec<DeepSeekMessage>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f64>,
}

#[derive(Serialize)]
pub struct DeepSeekMessage {
    pub role: String,
    pub content: String,
}

#[derive(Deserialize, Debug)]
pub struct DeepSeekResponse {
    pub choices: Vec<DeepSeekChoice>,
}

#[derive(Deserialize, Debug)]
pub struct DeepSeekChoice {
    pub message: DeepSeekRespMessage,
}

#[derive(Deserialize, Debug)]
pub struct DeepSeekRespMessage {
    pub content: String,
}

// ── Claude ───────────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct ClaudeRequest {
    pub model: String,
    pub max_tokens: u32,
    pub system: String,
    pub messages: Vec<ClaudeMessage>,
}

#[derive(Serialize)]
pub struct ClaudeMessage {
    pub role: String,
    pub content: String,
}

#[derive(Deserialize, Debug)]
pub struct ClaudeResponse {
    pub content: Vec<ClaudeContentBlock>,
}

#[derive(Deserialize, Debug)]
pub struct ClaudeContentBlock {
    pub text: String,
}
