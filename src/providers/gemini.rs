use anyhow::Result;
use async_trait::async_trait;
use futures::{stream, Stream};
use reqwest::Client;

use super::{LLMProvider, ChatRequest};

pub struct GeminiProvider {
    client: Client,
    api_key: String,
}

impl GeminiProvider {
    pub fn new(api_key: &str) -> Result<Self> {
        Ok(Self {
            client: Client::new(),
            api_key: api_key.to_string(),
        })
    }
}

#[async_trait]
impl LLMProvider for GeminiProvider {
    async fn chat(&self, _request: ChatRequest) -> Result<Box<dyn Stream<Item = Result<String>> + Unpin + Send>> {
        // TODO: Implement Gemini API integration
        let stream = stream::once(async { Ok("TODO: Gemini implementation".to_string()) });
        Ok(Box::pin(stream))
    }
    
    fn get_models(&self) -> Vec<String> {
        vec![
            "gemini-pro".to_string(),
            "gemini-pro-vision".to_string(),
        ]
    }
    
    fn get_name(&self) -> &str {
        "gemini"
    }
}
