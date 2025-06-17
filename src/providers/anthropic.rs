use anyhow::Result;
use async_trait::async_trait;
use futures::{stream, Stream};
use reqwest::Client;

use super::{LLMProvider, ChatRequest};

#[allow(dead_code)]
pub struct AnthropicProvider {
    client: Client,
    api_key: String,
}

impl AnthropicProvider {
    pub fn new(api_key: &str) -> Result<Self> {
        Ok(Self {
            client: Client::new(),
            api_key: api_key.to_string(),
        })
    }
}

#[async_trait]
impl LLMProvider for AnthropicProvider {
    async fn chat(&self, _request: ChatRequest) -> Result<Box<dyn Stream<Item = Result<String>> + Unpin + Send>> {
        // TODO: Implement Anthropic API integration
        let stream = stream::once(async { Ok("TODO: Anthropic implementation".to_string()) });
        Ok(Box::new(Box::pin(stream)))
    }
    
    fn get_models(&self) -> Vec<String> {
        vec![
            "claude-3-haiku-20240307".to_string(),
            "claude-3-sonnet-20240229".to_string(),
            "claude-3-opus-20240229".to_string(),
        ]
    }
    
    fn get_name(&self) -> &str {
        "anthropic"
    }
}
