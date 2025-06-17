use anyhow::Result;
use async_trait::async_trait;
use futures::{Stream, StreamExt};
use reqwest::Client;
use serde_json::json;

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
    async fn chat(&self, request: ChatRequest) -> Result<Box<dyn Stream<Item = Result<String>> + Unpin + Send>> {
        let url = "https://api.openai.com/v1/chat/completions";
        
        let payload = json!({
            "model": request.model,
            "messages": request.messages,
            "temperature": request.temperature,
            "stream": request.stream
        });
        
        let response = self.client
            .post(url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await?;
        
        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(anyhow::anyhow!("OpenAI API error: {}", error_text));
        }
        
        if request.stream {
            // Handle streaming response - simplified approach
            let stream = response
                .bytes_stream()
                .map(|chunk| {
                    match chunk {
                        Ok(bytes) => {
                            let text = String::from_utf8_lossy(&bytes);
                            // Simple parsing - look for content in each chunk
                            for line in text.lines() {
                                if let Some(json_str) = line.strip_prefix("data: ") {
                                    if json_str.trim() == "[DONE]" {
                                        continue;
                                    }
                                    if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(json_str) {
                                        if let Some(content) = json_val
                                            .get("choices")
                                            .and_then(|c| c.as_array())
                                            .and_then(|arr| arr.first())
                                            .and_then(|choice| choice.get("delta"))
                                            .and_then(|delta| delta.get("content"))
                                            .and_then(|content| content.as_str())
                                        {
                                            if !content.is_empty() {
                                                return Ok(content.to_string());
                                            }
                                        }
                                    }
                                }
                            }
                            Ok(String::new()) // Return empty string if no content found
                        }
                        Err(e) => Err(anyhow::anyhow!("Stream error: {}", e)),
                    }
                });
            
            Ok(Box::new(Box::pin(stream)))
        } else {
            // Handle non-streaming response
            let json_response: serde_json::Value = response.json().await?;
            
            let content = json_response
                .get("choices")
                .and_then(|choices| choices.as_array())
                .and_then(|arr| arr.first())
                .and_then(|choice| choice.get("message"))
                .and_then(|msg| msg.get("content"))
                .and_then(|content| content.as_str())
                .unwrap_or("No response content")
                .to_string();
            
            let stream = futures::stream::once(async move { Ok(content) });
            Ok(Box::new(Box::pin(stream)))
        }
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
