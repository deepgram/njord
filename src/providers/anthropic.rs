use anyhow::Result;
use async_trait::async_trait;
use futures::Stream;
use reqwest::Client;
use serde_json::json;
use tokio::time::{sleep, Duration};

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
    
    async fn make_request_with_retry(&self, url: &str, payload: &serde_json::Value) -> Result<reqwest::Response> {
        const MAX_RETRIES: u32 = 5;
        const BASE_DELAY_MS: u64 = 500; // 0.5 seconds
        
        for attempt in 0..MAX_RETRIES {
            let response = self.client
                .post(url)
                .header("x-api-key", &self.api_key)
                .header("anthropic-version", "2023-06-01")
                .header("Content-Type", "application/json")
                .json(payload)
                .send()
                .await;
            
            match response {
                Ok(resp) => {
                    if resp.status().is_success() {
                        return Ok(resp);
                    } else if resp.status().is_server_error() || resp.status() == 429 {
                        // Retry on server errors (5xx) and rate limiting (429)
                        if attempt < MAX_RETRIES - 1 {
                            let delay_ms = BASE_DELAY_MS * 2_u64.pow(attempt);
                            sleep(Duration::from_millis(delay_ms)).await;
                            continue;
                        } else {
                            let error_text = resp.text().await?;
                            return Err(anyhow::anyhow!("Anthropic API error after {} retries: {}", MAX_RETRIES, error_text));
                        }
                    } else {
                        // Don't retry on client errors (4xx except 429)
                        let error_text = resp.text().await?;
                        return Err(anyhow::anyhow!("Anthropic API error: {}", error_text));
                    }
                }
                Err(e) => {
                    // Retry on network errors
                    if attempt < MAX_RETRIES - 1 {
                        let delay_ms = BASE_DELAY_MS * 2_u64.pow(attempt);
                        sleep(Duration::from_millis(delay_ms)).await;
                        continue;
                    } else {
                        return Err(anyhow::anyhow!("Anthropic API network error after {} retries: {}", MAX_RETRIES, e));
                    }
                }
            }
        }
        
        unreachable!()
    }
    
    pub fn supports_thinking(&self, model: &str) -> bool {
        // Models that support thinking
        matches!(model, 
            "claude-opus-4-1-20250805" |
            "claude-opus-4-20250514" | 
            "claude-sonnet-4-20250514" | 
            "claude-3-7-sonnet-20250219" |
            "claude-sonnet-4-5-20250929" |
            "claude-haiku-4-5-20251001"
        )
    }
    
    #[allow(dead_code)]
    pub fn supports_temperature(&self, _model: &str) -> bool {
        // All Anthropic models support temperature
        true
    }
    
    #[allow(dead_code)]
    pub fn supports_streaming(&self, _model: &str) -> bool {
        // All Anthropic models support streaming
        true
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
            // Must be greater than thinking budget_tokens
            std::cmp::max(request.max_tokens, request.thinking_budget + 1000)
        } else {
            request.max_tokens
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
                "budget_tokens": request.thinking_budget
            });
            // Temperature must be 1.0 when thinking is enabled
            payload["temperature"] = json!(1.0);
        } else {
            payload["temperature"] = json!(request.temperature);
        }
        
        let response = self.make_request_with_retry(url, &payload).await?;
        
        if request.stream {
            // Handle streaming response with proper SSE parsing
            use futures::stream::unfold;
            use futures::StreamExt;
            
            let buffer = String::new();
            let byte_stream = response.bytes_stream();
            
            let stream = unfold(
                (buffer, byte_stream, Vec::<String>::new(), std::collections::HashMap::<usize, String>::new()),
                |(mut buffer, mut byte_stream, mut pending_content, mut content_block_types)| async move {
                    // First, check if we have pending content to yield
                    if let Some(content) = pending_content.pop() {
                        return Some((Ok(content), (buffer, byte_stream, pending_content, content_block_types)));
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
                                                return Some((Ok(content), (buffer, byte_stream, pending_content, content_block_types)));
                                            }
                                            return None; // End of stream
                                        }
                                        
                                        // Parse the JSON chunk
                                        if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(json_str) {
                                            // Handle different event types
                                            if let Some(event_type) = json_val.get("type").and_then(|t| t.as_str()) {
                                                match event_type {
                                                    "content_block_start" => {
                                                        // Track content block types by index
                                                        if let Some(index) = json_val.get("index").and_then(|i| i.as_u64()) {
                                                            if let Some(content_block) = json_val.get("content_block") {
                                                                if let Some(block_type) = content_block.get("type").and_then(|t| t.as_str()) {
                                                                    content_block_types.insert(index as usize, block_type.to_string());
                                                                }
                                                            }
                                                        }
                                                    }
                                                    "content_block_delta" => {
                                                        // Get the index to determine content block type
                                                        if let Some(index) = json_val.get("index").and_then(|i| i.as_u64()) {
                                                            let index = index as usize;
                                                            let is_thinking = content_block_types.get(&index) == Some(&"thinking".to_string());
                                                            
                                                            // Handle thinking deltas
                                                            if is_thinking {
                                                                if let Some(thinking_content) = json_val
                                                                    .get("delta")
                                                                    .and_then(|delta| delta.get("thinking"))
                                                                    .and_then(|thinking| thinking.as_str())
                                                                {
                                                                    if !thinking_content.is_empty() {
                                                                        pending_content.insert(0, format!("thinking:{}", thinking_content));
                                                                    }
                                                                }
                                                            } else {
                                                                // Handle regular text deltas
                                                                if let Some(text_content) = json_val
                                                                    .get("delta")
                                                                    .and_then(|delta| delta.get("text"))
                                                                    .and_then(|text| text.as_str())
                                                                {
                                                                    if !text_content.is_empty() {
                                                                        pending_content.insert(0, format!("content:{}", text_content));
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                    "message_stop" => {
                                                        // End of message
                                                        if let Some(content) = pending_content.pop() {
                                                            return Some((Ok(content), (buffer, byte_stream, pending_content, content_block_types)));
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
                                    return Some((Ok(content), (buffer, byte_stream, pending_content, content_block_types)));
                                }
                                // Continue to next chunk if no content to yield
                            }
                            Some(Err(e)) => {
                                return Some((Err(anyhow::anyhow!("Stream error: {}", e)), (buffer, byte_stream, pending_content, content_block_types)));
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
                                    return Some((Ok(content), (String::new(), byte_stream, pending_content, content_block_types)));
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
            "claude-opus-4-1-20250805".to_string(),
            "claude-sonnet-4-5-20250929".to_string(),
            "claude-haiku-4-5-20251001".to_string(),
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
    
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
