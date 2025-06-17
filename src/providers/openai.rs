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
            // Handle streaming response with proper SSE parsing
            use futures::stream::unfold;
            
            let buffer = String::new();
            let byte_stream = response.bytes_stream();
            
            let stream = unfold(
                (buffer, byte_stream),
                |(mut buffer, mut byte_stream)| async move {
                    loop {
                        match byte_stream.next().await {
                            Some(Ok(bytes)) => {
                                let chunk = String::from_utf8_lossy(&bytes);
                                buffer.push_str(&chunk);
                                
                                // Process complete lines
                                while let Some(line_end) = buffer.find('\n') {
                                    let line = buffer[..line_end].trim().to_string();
                                    buffer = buffer[line_end + 1..].to_string();
                                    
                                    if let Some(json_str) = line.strip_prefix("data: ") {
                                        if json_str.trim() == "[DONE]" {
                                            return None; // End of stream
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
                                                    return Some((Ok(content.to_string()), (buffer, byte_stream)));
                                                }
                                            }
                                        }
                                    }
                                }
                                
                                // If we didn't find content in this chunk, continue to next chunk
                                continue;
                            }
                            Some(Err(e)) => {
                                return Some((Err(anyhow::anyhow!("Stream error: {}", e)), (buffer, byte_stream)));
                            }
                            None => {
                                // Stream ended - process any remaining content in buffer
                                let remaining_buffer = buffer.trim();
                                if !remaining_buffer.is_empty() {
                                    // Split by lines and also check for incomplete lines
                                    let mut lines: Vec<&str> = remaining_buffer.lines().collect();
                                    
                                    // If buffer doesn't end with newline, the last part might be an incomplete line
                                    if !buffer.ends_with('\n') && !remaining_buffer.is_empty() {
                                        // Check if the remaining content looks like a data line
                                        if remaining_buffer.starts_with("data: ") {
                                            lines.push(remaining_buffer);
                                        }
                                    }
                                    
                                    for line in lines {
                                        if let Some(json_str) = line.trim().strip_prefix("data: ") {
                                            if json_str.trim() != "[DONE]" && !json_str.trim().is_empty() {
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
                                }
                                return None; // Stream ended
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
