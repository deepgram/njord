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
    
    pub fn is_reasoning_model(&self, model: &str) -> bool {
        model.starts_with("o1") || model.starts_with("o3") || model.starts_with("o4")
    }
    
    fn supports_streaming(&self, model: &str) -> bool {
        // Based on your analysis, these models don't support streaming
        !matches!(model, "o3-pro" | "o1-pro")
    }
    
    fn requires_responses_api(&self, model: &str) -> bool {
        // Based on your analysis, these models require the responses API
        matches!(model, "o3-pro" | "o1-pro")
    }
}

#[async_trait]
impl LLMProvider for OpenAIProvider {
    async fn chat(&self, request: ChatRequest) -> Result<Box<dyn Stream<Item = Result<String>> + Unpin + Send>> {
        // Use Responses API for all models since all support it
        let url = "https://api.openai.com/v1/responses";
        
        // Check if model supports streaming
        let can_stream = self.supports_streaming(&request.model);
        let should_stream = request.stream && can_stream;
        
        let mut payload = json!({
            "model": request.model,
            "input": request.messages
        });
        
        // Add streaming if supported
        if should_stream {
            payload["stream"] = json!(true);
        }
        
        // Only add temperature for non-reasoning models
        if !self.is_reasoning_model(&request.model) {
            payload["temperature"] = json!(request.temperature);
        }
        
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
        
        if should_stream {
            // Handle streaming response with proper SSE parsing
            use futures::stream::unfold;
            use futures::StreamExt;
            
            let buffer = String::new();
            let byte_stream = response.bytes_stream();
            
            let stream = unfold(
                (buffer, byte_stream, Vec::<String>::new()),
                |(mut buffer, mut byte_stream, mut pending_content)| async move {
                    // First, check if we have pending content to yield
                    if let Some(content) = pending_content.pop() {
                        return Some((Ok(content), (buffer, byte_stream, pending_content)));
                    }
                    
                    loop {
                        match byte_stream.next().await {
                            Some(Ok(bytes)) => {
                                let chunk = String::from_utf8_lossy(&bytes);
                                buffer.push_str(&chunk);
                                
                                // Process ALL complete lines ending with \n
                                while let Some(newline_pos) = buffer.find('\n') {
                                    let line = buffer[..newline_pos].trim().to_string();
                                    buffer = buffer[newline_pos + 1..].to_string();
                                    
                                    
                                    // Parse SSE data lines
                                    if let Some(json_str) = line.strip_prefix("data: ") {
                                        if json_str.trim() == "[DONE]" {
                                            // If we have pending content, yield it first
                                            if let Some(content) = pending_content.pop() {
                                                return Some((Ok(content), (buffer, byte_stream, pending_content)));
                                            }
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
                                                    pending_content.insert(0, content.to_string()); // Insert at beginning to maintain order
                                                }
                                            }
                                        }
                                    }
                                }
                                
                                // If we have pending content, yield the first piece
                                if let Some(content) = pending_content.pop() {
                                    return Some((Ok(content), (buffer, byte_stream, pending_content)));
                                }
                                // Continue to next chunk if no content to yield
                            }
                            Some(Err(e)) => {
                                return Some((Err(anyhow::anyhow!("Stream error: {}", e)), (buffer, byte_stream, pending_content)));
                            }
                            None => {
                                // Stream ended - process any remaining complete lines in buffer
                                while let Some(newline_pos) = buffer.find('\n') {
                                    let line = buffer[..newline_pos].trim().to_string();
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
                                                        pending_content.insert(0, content.to_string());
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                
                                // Yield any remaining pending content
                                if let Some(content) = pending_content.pop() {
                                    return Some((Ok(content), (String::new(), byte_stream, pending_content)));
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
            
            // Parse response using the Responses API format
            let content = json_response
                .get("output")
                .and_then(|output| output.as_array())
                .and_then(|arr| {
                    // Find the message object in the output array
                    arr.iter().find(|item| {
                        item.get("type").and_then(|t| t.as_str()) == Some("message")
                    })
                })
                .and_then(|message| message.get("content"))
                .and_then(|content| content.as_array())
                .and_then(|arr| arr.first())
                .and_then(|content_item| content_item.get("text"))
                .and_then(|text| text.as_str())
                .unwrap_or("No response content")
                .to_string();
            
            // For non-streaming models, simulate streaming by yielding content in chunks
            if !can_stream && !content.is_empty() {
                use futures::stream;
                use std::time::Duration;
                
                // Split content into words and move them into the closure
                let words: Vec<String> = content.split_whitespace().map(|s| s.to_string()).collect();
                let chunk_size = 3; // Words per chunk
                
                let stream = stream::unfold(
                    (words, 0),
                    move |(words, mut index)| async move {
                        if index >= words.len() {
                            return None;
                        }
                        
                        // Take next chunk of words
                        let end_index = std::cmp::min(index + chunk_size, words.len());
                        let chunk_words = &words[index..end_index];
                        let chunk = if index == 0 {
                            chunk_words.join(" ")
                        } else {
                            format!(" {}", chunk_words.join(" "))
                        };
                        
                        index = end_index;
                        
                        // Add small delay to simulate streaming
                        tokio::time::sleep(Duration::from_millis(50)).await;
                        
                        Some((Ok(chunk), (words, index)))
                    }
                );
                
                Ok(Box::new(Box::pin(stream)))
            } else {
                let stream = futures::stream::once(async move { Ok(content) });
                Ok(Box::new(Box::pin(stream)))
            }
        }
    }
    
    fn get_models(&self) -> Vec<String> {
        vec![
            "o3-pro".to_string(),
            "o3".to_string(),
            "o4-mini".to_string(),
            "o3-mini".to_string(),
            "o1-pro".to_string(),
            "o1".to_string(),
            "gpt-4.1".to_string(),
            "gpt-4o".to_string(),
            "gpt-4.1-mini".to_string(),
            "gpt-4o-mini".to_string(),
            "gpt-4.1-nano".to_string(),
        ]
    }
    
    fn get_name(&self) -> &str {
        "openai"
    }
    
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
