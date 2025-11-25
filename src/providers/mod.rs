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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_provider_for_model() {
        // Test OpenAI models
        assert_eq!(get_provider_for_model("gpt-4"), Some("openai"));
        assert_eq!(get_provider_for_model("gpt-4o"), Some("openai"));
        assert_eq!(get_provider_for_model("o1-pro"), Some("openai"));
        assert_eq!(get_provider_for_model("o3-pro"), Some("openai"));
        assert_eq!(get_provider_for_model("o4-mini"), Some("openai"));
        
        // Test Anthropic models
        assert_eq!(get_provider_for_model("claude-sonnet-4-20250514"), Some("anthropic"));
        assert_eq!(get_provider_for_model("claude-opus-4-20250514"), Some("anthropic"));
        assert_eq!(get_provider_for_model("claude-3-5-sonnet-20241022"), Some("anthropic"));
        
        // Test Gemini models
        assert_eq!(get_provider_for_model("gemini-3-pro-preview"), Some("gemini"));
        assert_eq!(get_provider_for_model("gemini-2.5-pro"), Some("gemini"));
        assert_eq!(get_provider_for_model("gemini-2.5-flash"), Some("gemini"));
        assert_eq!(get_provider_for_model("gemini-2.5-flash-lite"), Some("gemini"));
        
        // Test unknown models
        assert_eq!(get_provider_for_model("unknown-model"), None);
        assert_eq!(get_provider_for_model(""), None);
        assert_eq!(get_provider_for_model("random-text"), None);
    }

    #[test]
    fn test_message_creation() {
        let message = Message {
            role: "user".to_string(),
            content: "Hello, world!".to_string(),
        };
        
        assert_eq!(message.role, "user");
        assert_eq!(message.content, "Hello, world!");
    }

    #[test]
    fn test_chat_request_creation() {
        let messages = vec![
            Message {
                role: "user".to_string(),
                content: "Hello".to_string(),
            },
            Message {
                role: "assistant".to_string(),
                content: "Hi there!".to_string(),
            },
        ];
        
        let request = ChatRequest {
            messages: messages.clone(),
            model: "gpt-4".to_string(),
            temperature: 0.7,
            max_tokens: 1000,
            thinking_budget: 5000,
            stream: true,
            thinking: false,
        };
        
        assert_eq!(request.messages.len(), 2);
        assert_eq!(request.model, "gpt-4");
        assert_eq!(request.temperature, 0.7);
        assert_eq!(request.max_tokens, 1000);
        assert_eq!(request.thinking_budget, 5000);
        assert!(request.stream);
        assert!(!request.thinking);
    }
}
