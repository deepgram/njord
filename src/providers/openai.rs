use anyhow::Result;
use async_trait::async_trait;
use futures::Stream;
use reqwest::Client;
use serde_json::json;
use tokio::time::{sleep, Duration};

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
    
    async fn make_request_with_retry(&self, url: &str, payload: &serde_json::Value) -> Result<reqwest::Response> {
        const MAX_RETRIES: u32 = 5;
        const BASE_DELAY_MS: u64 = 500; // 0.5 seconds
        
        for attempt in 0..MAX_RETRIES {
            let response = self.client
                .post(url)
                .header("Authorization", format!("Bearer {}", self.api_key))
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
                            return Err(anyhow::anyhow!("OpenAI API error after {} retries: {}", MAX_RETRIES, error_text));
                        }
                    } else {
                        // Don't retry on client errors (4xx except 429)
                        let error_text = resp.text().await?;
                        return Err(anyhow::anyhow!("OpenAI API error: {}", error_text));
                    }
                }
                Err(e) => {
                    // Retry on network errors
                    if attempt < MAX_RETRIES - 1 {
                        let delay_ms = BASE_DELAY_MS * 2_u64.pow(attempt);
                        sleep(Duration::from_millis(delay_ms)).await;
                        continue;
                    } else {
                        return Err(anyhow::anyhow!("OpenAI API network error after {} retries: {}", MAX_RETRIES, e));
                    }
                }
            }
        }
        
        unreachable!()
    }
    
    pub fn is_reasoning_model(&self, model: &str) -> bool {
        model.starts_with("o1") || model.starts_with("o3") || model.starts_with("o4")
    }
    
    
    fn supports_streaming(&self, model: &str) -> bool {
        // Based on your analysis, these models don't support streaming
        !matches!(model, "o3-pro" | "o1-pro")
    }
    
    pub fn supports_temperature(&self, model: &str) -> bool {
        // Models that don't support custom temperature
        !matches!(model, "o4-mini" | "o3-pro" | "o1-pro") && !self.is_reasoning_model(model)
    }
    
    #[allow(dead_code)]
    pub fn supports_thinking(&self, _model: &str) -> bool {
        // OpenAI models don't support thinking
        false
    }
    
}

#[async_trait]
impl LLMProvider for OpenAIProvider {
    async fn chat(&self, request: ChatRequest) -> Result<Box<dyn Stream<Item = Result<String>> + Unpin + Send>> {
        // Use different APIs based on model requirements
        let use_responses_api = matches!(request.model.as_str(), "o3-pro" | "o1-pro");
        
        let (url, payload) = if use_responses_api {
            // Use Responses API for models that require it
            let url = "https://api.openai.com/v1/responses";
            let mut payload = json!({
                "model": request.model,
                "input": request.messages
            });
            
            // Only add temperature for non-reasoning models
            if !self.is_reasoning_model(&request.model) {
                payload["temperature"] = json!(request.temperature);
            }
            
            (url, payload)
        } else {
            // Use Chat Completions API for regular models
            let url = "https://api.openai.com/v1/chat/completions";
            let mut payload = json!({
                "model": request.model,
                "messages": request.messages,
                "stream": request.stream
            });
            
            // Only add temperature for models that support it
            if self.supports_temperature(&request.model) {
                payload["temperature"] = json!(request.temperature);
            }
            
            (url, payload)
        };
        
        // Check if model supports streaming
        let can_stream = self.supports_streaming(&request.model);
        let should_stream = request.stream && can_stream;
        
        let response = self.make_request_with_retry(url, &payload).await?;
        
        if should_stream && !use_responses_api {
            // Handle streaming response for Chat Completions API
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
                                                    pending_content.insert(0, content.to_string());
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
            
            let content = if use_responses_api {
                // Parse response using the Responses API format
                json_response
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
                    .to_string()
            } else {
                // Parse response using the Chat Completions API format
                json_response
                    .get("choices")
                    .and_then(|choices| choices.as_array())
                    .and_then(|arr| arr.first())
                    .and_then(|choice| choice.get("message"))
                    .and_then(|msg| msg.get("content"))
                    .and_then(|content| content.as_str())
                    .unwrap_or("No response content")
                    .to_string()
            };
            
            let stream = futures::stream::once(async move { Ok(content) });
            Ok(Box::new(Box::pin(stream)))
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
