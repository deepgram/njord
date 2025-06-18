use anyhow::Result;
use async_trait::async_trait;
use futures::Stream;
use reqwest::Client;
use serde_json::json;

use super::{LLMProvider, ChatRequest, Message};

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
    
    fn supports_thinking(&self, model: &str) -> bool {
        // Models that support thinking
        matches!(model, 
            "claude-opus-4-20250514" | 
            "claude-sonnet-4-20250514" | 
            "claude-3-7-sonnet-20250219"
        )
    }
    
    fn convert_messages(&self, messages: &[Message]) -> (Option<String>, Vec<serde_json::Value>) {
        let mut system_message = None;
        let mut anthropic_messages = Vec::new();
        
        for msg in messages {
            if msg.role == "system" {
                system_message = Some(msg.content.clone());
            } else {
                anthropic_messages.push(json!({
                    "role": msg.role,
                    "content": msg.content
                }));
            }
        }
        
        (system_message, anthropic_messages)
    }
}

#[async_trait]
impl LLMProvider for AnthropicProvider {
    async fn chat(&self, request: ChatRequest) -> Result<Box<dyn Stream<Item = Result<String>> + Unpin + Send>> {
        let url = "https://api.anthropic.com/v1/messages";
        
        let (system_message, anthropic_messages) = self.convert_messages(&request.messages);
        
        // Set max_tokens based on whether thinking is enabled
        let max_tokens = if request.thinking && self.supports_thinking(&request.model) {
            25000 // Must be greater than thinking budget_tokens (20000)
        } else {
            4096
        };
        
        let mut payload = json!({
            "model": request.model,
            "max_tokens": max_tokens,
            "messages": anthropic_messages,
            "stream": request.stream
        });
        
        if let Some(system) = system_message {
            payload["system"] = json!(system);
        }
        
        // Enable thinking for supported models
        if request.thinking && self.supports_thinking(&request.model) {
            payload["thinking"] = json!({
                "type": "enabled",
                "budget_tokens": 20000
            });
            // Temperature must be 1.0 when thinking is enabled
            payload["temperature"] = json!(1.0);
        } else {
            payload["temperature"] = json!(request.temperature);
        }
        
        let response = self.client
            .post(url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await?;
        
        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(anyhow::anyhow!("Anthropic API error: {}", error_text));
        }
        
        if request.stream {
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
                                            // Handle different event types
                                            if let Some(event_type) = json_val.get("type").and_then(|t| t.as_str()) {
                                                match event_type {
                                                    "content_block_delta" => {
                                                        // Check if this is a thinking content block
                                                        let is_thinking = json_val
                                                            .get("content_block")
                                                            .and_then(|cb| cb.get("type"))
                                                            .and_then(|t| t.as_str()) == Some("thinking");
                                                        
                                                        if let Some(content) = json_val
                                                            .get("delta")
                                                            .and_then(|delta| delta.get("text"))
                                                            .and_then(|text| text.as_str())
                                                        {
                                                            if !content.is_empty() {
                                                                if is_thinking {
                                                                    pending_content.insert(0, format!("thinking:{}", content));
                                                                } else {
                                                                    pending_content.insert(0, format!("content:{}", content));
                                                                }
                                                            }
                                                        }
                                                    }
                                                    "message_stop" => {
                                                        // End of message
                                                        if let Some(content) = pending_content.pop() {
                                                            return Some((Ok(content), (buffer, byte_stream, pending_content)));
                                                        }
                                                        return None;
                                                    }
                                                    _ => {
                                                        // Ignore other event types (message_start, content_block_start, etc.)
                                                    }
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
                                                if let Some(event_type) = json_val.get("type").and_then(|t| t.as_str()) {
                                                    if event_type == "content_block_delta" {
                                                        if let Some(content) = json_val
                                                            .get("delta")
                                                            .and_then(|delta| delta.get("text"))
                                                            .and_then(|text| text.as_str())
                                                        {
                                                            if !content.is_empty() {
                                                                pending_content.insert(0, content.to_string());
                                                            }
                                                        }
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
            
            let content = json_response
                .get("content")
                .and_then(|content| content.as_array())
                .and_then(|arr| arr.first())
                .and_then(|block| block.get("text"))
                .and_then(|text| text.as_str())
                .unwrap_or("No response content")
                .to_string();
            
            let stream = futures::stream::once(async move { Ok(content) });
            Ok(Box::new(Box::pin(stream)))
        }
    }
    
    fn get_models(&self) -> Vec<String> {
        vec![
            "claude-sonnet-4-20250514".to_string(),
            "claude-opus-4-20250514".to_string(),
            "claude-3-7-sonnet-20250219".to_string(),
            "claude-3-5-sonnet-20241022".to_string(),
            "claude-3-5-haiku-20241022".to_string(),
            "claude-3-5-sonnet-20240620".to_string(),
        ]
    }
    
    fn get_name(&self) -> &str {
        "anthropic"
    }
    
}
