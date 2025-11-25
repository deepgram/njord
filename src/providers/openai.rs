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
        // gpt-5 and gpt-5.1 variants support reasoning, but gpt-4 variants do not
        // All o1, o3, o4 models support reasoning
        (model.starts_with("gpt-5") && !model.starts_with("gpt-4")) || 
        model.starts_with("o1") || 
        model.starts_with("o3") || 
        model.starts_with("o4")
    }
    
    pub fn supports_chat_completions(&self, model: &str) -> bool {
        // Based on the table, o3-pro and o1-pro don't support Chat Completions
        !matches!(model, "o3-pro" | "o1-pro")
    }
    
    pub fn supports_responses_api(&self, _model: &str) -> bool {
        // According to the table, all models support Responses API
        true
    }
    
    fn supports_streaming(&self, model: &str) -> bool {
        // Based on your analysis, these models don't support streaming
        !matches!(model, "o3-pro" | "o1-pro")
    }
    
    pub fn supports_temperature(&self, model: &str) -> bool {
        // Models that don't support custom temperature (reasoning models typically don't)
        !matches!(model, "o4-mini" | "o3-pro" | "o1-pro") && !self.is_reasoning_model(model)
    }
    
    pub fn supports_thinking(&self, model: &str) -> bool {
        // OpenAI reasoning models support thinking
        self.is_reasoning_model(model)
    }
    
}

#[async_trait]
impl LLMProvider for OpenAIProvider {
    async fn chat(&self, request: ChatRequest) -> Result<Box<dyn Stream<Item = Result<String>> + Unpin + Send>> {
        // Always prefer Responses API for reasoning support when thinking is enabled
        // Fall back to Chat Completions only if model doesn't support Responses API or thinking is disabled
        let use_responses_api = if request.thinking && self.supports_thinking(&request.model) {
            // Always use Responses API for thinking-enabled requests on reasoning models
            true
        } else if !self.supports_chat_completions(&request.model) {
            // Must use Responses API for models that don't support Chat Completions
            true
        } else {
            // Use Chat Completions for non-reasoning requests on models that support it
            false
        };
        
        // Capture thinking flag for debug output
        let is_thinking = request.thinking && self.supports_thinking(&request.model);
        
        let (url, payload) = if use_responses_api {
            // Use Responses API for reasoning support
            let url = "https://api.openai.com/v1/responses";
            let mut payload = json!({
                "model": request.model,
                "input": request.messages
            });
            
            // Add reasoning support for thinking-enabled models
            if is_thinking {
                payload["reasoning"] = json!({
                    "summary": "detailed",  // Use detailed for full reasoning output
                    "effort": "high"
                });
                payload["max_output_tokens"] = json!(request.max_tokens + request.thinking_budget);
            } else {
                payload["max_output_tokens"] = json!(request.max_tokens);
            }
            
            // Only add temperature for non-reasoning models
            if !self.is_reasoning_model(&request.model) && self.supports_temperature(&request.model) {
                payload["temperature"] = json!(request.temperature);
            }
            
            // DEBUG: Log the request payload when thinking is enabled
            if is_thinking {
                eprintln!("DEBUG: OpenAI Responses API request with reasoning: {}", serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "Failed to serialize".to_string()));
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
            
            // Add reasoning support for thinking-enabled models (though Chat Completions doesn't return reasoning)
            if is_thinking {
                payload["reasoning_effort"] = json!("high");
                // Use max_completion_tokens for reasoning models (includes output + reasoning)
                payload["max_completion_tokens"] = json!(request.max_tokens + request.thinking_budget);
            } else {
                // Use regular max_tokens for non-reasoning models
                payload["max_tokens"] = json!(request.max_tokens);
            }
            
            // Only add temperature for models that support it
            if self.supports_temperature(&request.model) {
                payload["temperature"] = json!(request.temperature);
            }
            
            (url, payload)
        };
        
        // Check if model supports streaming
        let can_stream = self.supports_streaming(&request.model);
        let should_stream = request.stream && can_stream && !use_responses_api; // Responses API doesn't support streaming
        
        let response = self.make_request_with_retry(url, &payload).await?;
        
        if should_stream {
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
                                                    // Chat Completions API doesn't return reasoning, so all content is regular content
                                                    pending_content.insert(0, format!("content:{}", content));
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
                                                        pending_content.insert(0, format!("content:{}", content));
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
            
            // DEBUG: Log the full response structure when thinking is enabled
            if is_thinking {
                eprintln!("DEBUG: OpenAI {} response: {}", 
                    if use_responses_api { "Responses API" } else { "Chat Completions API" },
                    serde_json::to_string_pretty(&json_response).unwrap_or_else(|_| "Failed to serialize".to_string()));
            }
            
            let mut full_content = String::new();
            
            if use_responses_api {
                // Parse response using the Responses API format
                if let Some(output) = json_response.get("output") {
                    if let Some(output_array) = output.as_array() {
                        // DEBUG: Log output array structure
                        if is_thinking {
                            eprintln!("DEBUG: Output array has {} items", output_array.len());
                            for (i, item) in output_array.iter().enumerate() {
                                if let Some(item_type) = item.get("type").and_then(|t| t.as_str()) {
                                    eprintln!("DEBUG: Output[{}] type: {}", i, item_type);
                                }
                            }
                        }
                        
                        // First, look for reasoning content
                        for item in output_array {
                            if let Some(item_type) = item.get("type").and_then(|t| t.as_str()) {
                                if item_type == "reasoning" {
                                    if let Some(reasoning_content) = item.get("content") {
                                        if let Some(reasoning_str) = reasoning_content.as_str() {
                                            // DEBUG: Log reasoning content
                                            if is_thinking {
                                                eprintln!("DEBUG: Found reasoning content: {}", reasoning_str);
                                            }
                                            full_content.push_str(&format!("thinking:{}", reasoning_str));
                                        } else if let Some(reasoning_array) = reasoning_content.as_array() {
                                            // Reasoning might be an array of content items
                                            for reasoning_item in reasoning_array {
                                                if let Some(text) = reasoning_item.get("text").and_then(|t| t.as_str()) {
                                                    // DEBUG: Log reasoning text
                                                    if is_thinking {
                                                        eprintln!("DEBUG: Found reasoning text: {}", text);
                                                    }
                                                    full_content.push_str(&format!("thinking:{}", text));
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        
                        // Then, look for message content
                        for item in output_array {
                            if let Some(item_type) = item.get("type").and_then(|t| t.as_str()) {
                                if item_type == "message" {
                                    if let Some(content) = item.get("content") {
                                        if let Some(content_array) = content.as_array() {
                                            for content_item in content_array {
                                                if let Some(text) = content_item.get("text").and_then(|t| t.as_str()) {
                                                    // DEBUG: Log message text
                                                    if is_thinking {
                                                        eprintln!("DEBUG: Found message text: {}", text);
                                                    }
                                                    full_content.push_str(&format!("content:{}", text));
                                                }
                                            }
                                        } else if let Some(content_str) = content.as_str() {
                                            // DEBUG: Log message content
                                            if is_thinking {
                                                eprintln!("DEBUG: Found message content: {}", content_str);
                                            }
                                            full_content.push_str(&format!("content:{}", content_str));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                
                if full_content.is_empty() {
                    full_content = "content:No response content".to_string();
                }
            } else {
                // Parse response using the Chat Completions API format
                let content = json_response
                    .get("choices")
                    .and_then(|choices| choices.as_array())
                    .and_then(|arr| arr.first())
                    .and_then(|choice| choice.get("message"))
                    .and_then(|msg| msg.get("content"))
                    .and_then(|content| content.as_str())
                    .unwrap_or("No response content")
                    .to_string();
                
                // Chat Completions API doesn't return reasoning, so all content is regular content
                full_content = format!("content:{}", content);
            }
            
            let stream = futures::stream::once(async move { Ok(full_content) });
            Ok(Box::new(Box::pin(stream)))
        }
    }
    
    fn get_models(&self) -> Vec<String> {
        vec![
            "gpt-5.1".to_string(),
            "gpt-5.1-2025-11-13".to_string(),
            "gpt-5".to_string(),
            "gpt-5-mini".to_string(),
            "gpt-5-nano".to_string(),
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
