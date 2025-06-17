use anyhow::Result;
use async_trait::async_trait;
use futures::Stream;
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
            // Handle streaming response with proper SSE parsing
            use futures::stream::unfold;
            use futures::StreamExt;
            
            let mut buffer = String::new();
            let mut byte_stream = response.bytes_stream();
            
            let stream = unfold(
                (buffer, byte_stream),
                |(mut buffer, mut byte_stream)| async move {
                    loop {
                        match byte_stream.next().await {
                            Some(Ok(bytes)) => {
                                let chunk = String::from_utf8_lossy(&bytes);
                                buffer.push_str(&chunk);
                                
                                // Process complete lines ending with \n
                                while let Some(newline_pos) = buffer.find('\n') {
                                    let line = buffer[..newline_pos].trim();
                                    buffer = buffer[newline_pos + 1..].to_string();
                                    
                                    // Parse SSE data lines
                                    if let Some(json_str) = line.strip_prefix("data: ") {
                                        if json_str.trim() == "[DONE]" {
                                            return None; // End of stream
                                        }
                                        
                                        // Parse the JSON chunk
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
                                                    return Some((Ok(content.to_string()), (buffer, byte_stream)));
                                                }
                                            }
                                        }
                                    }
                                }
                                // Continue to next chunk if no complete line found
                            }
                            Some(Err(e)) => {
                                return Some((Err(anyhow::anyhow!("Stream error: {}", e)), (buffer, byte_stream)));
                            }
                            None => {
                                // Stream ended - process any remaining complete lines in buffer
                                while let Some(newline_pos) = buffer.find('\n') {
                                    let line = buffer[..newline_pos].trim();
                                    buffer = buffer[newline_pos + 1..].to_string();
                                    
                                    if let Some(json_str) = line.strip_prefix("data: ") {
                                        if json_str.trim() != "[DONE]" {
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
                                                        return Some((Ok(content.to_string()), (String::new(), byte_stream)));
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                return None; // Stream truly ended
                            }
                        }
                    }
                }
            );
            
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
