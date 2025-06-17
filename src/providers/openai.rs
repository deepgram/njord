use anyhow::Result;
use async_trait::async_trait;
use futures::{stream, Stream};
use reqwest::Client;

use super::{LLMProvider, ChatRequest};

#[allow(dead_code)]
pub struct OpenAIProvider {
    client: Client,
    api_key: String,
}

impl OpenAIProvider {
    pub fn new(api_key: &str) -> Result<Self> {
        Ok(Self {
            client: Client::new(),
            api_key: api_key.to_string(),
        })
    }
}

#[async_trait]
impl LLMProvider for OpenAIProvider {
    async fn chat(&self, _request: ChatRequest) -> Result<Box<dyn Stream<Item = Result<String>> + Unpin + Send>> {
        // TODO: Implement OpenAI API integration
        let stream = stream::once(async { Ok("TODO: OpenAI implementation".to_string()) });
        Ok(Box::new(Box::pin(stream)))
    }
    
    fn get_models(&self) -> Vec<String> {
        vec![
            "gpt-3.5-turbo".to_string(),
            "gpt-4".to_string(),
            "gpt-4-turbo".to_string(),
        ]
    }
    
    fn get_name(&self) -> &str {
        "openai"
    }
}
