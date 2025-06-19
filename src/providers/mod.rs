pub mod openai;
pub mod anthropic;
pub mod gemini;

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use futures::Stream;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ChatRequest {
    pub messages: Vec<Message>,
    pub model: String,
    pub temperature: f32,
    pub max_tokens: u32,
    pub thinking_budget: u32,
    pub stream: bool,
    pub thinking: bool,
}

#[async_trait]
pub trait LLMProvider: Send + Sync {
    #[allow(dead_code)]
    async fn chat(&self, request: ChatRequest) -> Result<Box<dyn Stream<Item = Result<String>> + Unpin + Send>>;
    fn get_models(&self) -> Vec<String>;
    #[allow(dead_code)]
    fn get_name(&self) -> &str;
    fn as_any(&self) -> &dyn std::any::Any;
}

pub fn get_provider_for_model(model: &str) -> Option<&'static str> {
    if model.starts_with("claude-") {
        Some("anthropic")
    } else if model.starts_with("gpt-") || model.starts_with("o1") || model.starts_with("o3") || model.starts_with("o4") {
        Some("openai")
    } else if model.starts_with("gemini-") {
        Some("gemini")
    } else {
        None
    }
}

pub fn create_provider(name: &str, api_key: &str) -> Result<Box<dyn LLMProvider>> {
    match name {
        "openai" => Ok(Box::new(openai::OpenAIProvider::new(api_key)?)),
        "anthropic" => Ok(Box::new(anthropic::AnthropicProvider::new(api_key)?)),
        "gemini" => Ok(Box::new(gemini::GeminiProvider::new(api_key)?)),
        _ => Err(anyhow::anyhow!("Unknown provider: {}", name)),
    }
}
