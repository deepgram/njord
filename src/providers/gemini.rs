use anyhow::Result;
use async_trait::async_trait;
use futures::Stream;
use reqwest::Client;
use serde_json::json;
use tokio::time::{sleep, Duration};

use super::{LLMProvider, ChatRequest, Message};

#[allow(dead_code)]
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
    
    async fn make_request_with_retry(&self, url: &str, payload: &serde_json::Value) -> Result<reqwest::Response> {
        const MAX_RETRIES: u32 = 5;
        const BASE_DELAY_MS: u64 = 500; // 0.5 seconds
        
        for attempt in 0..MAX_RETRIES {
            let response = self.client
                .post(url)
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
                            return Err(anyhow::anyhow!("Gemini API error after {} retries: {}", MAX_RETRIES, error_text));
                        }
                    } else {
                        // Don't retry on client errors (4xx except 429)
                        let error_text = resp.text().await?;
                        return Err(anyhow::anyhow!("Gemini API error: {}", error_text));
                    }
                }
                Err(e) => {
                    // Retry on network errors
                    if attempt < MAX_RETRIES - 1 {
                        let delay_ms = BASE_DELAY_MS * 2_u64.pow(attempt);
                        sleep(Duration::from_millis(delay_ms)).await;
                        continue;
                    } else {
                        return Err(anyhow::anyhow!("Gemini API network error after {} retries: {}", MAX_RETRIES, e));
                    }
                }
            }
        }
        
        unreachable!()
    }
    
    #[allow(dead_code)]
    pub fn supports_thinking(&self, _model: &str) -> bool {
        // All Gemini models support thinking
        true
    }
    
    #[allow(dead_code)]
    pub fn supports_temperature(&self, _model: &str) -> bool {
        // All Gemini models support temperature
        true
    }
    
    #[allow(dead_code)]
    pub fn supports_streaming(&self, _model: &str) -> bool {
        // All Gemini models support streaming
        true
    }
    
    fn convert_messages(&self, messages: &[Message]) -> Vec<serde_json::Value> {
        let mut contents = Vec::new();
        
        for msg in messages {
            let role = match msg.role.as_str() {
                "user" => "user",
                "assistant" => "model",
                "system" => "user", // Gemini doesn't have system role, treat as user
                _ => "user",
            };
            
            contents.push(json!({
                "role": role,
                "parts": [{
                    "text": msg.content
                }]
            }));
        }
        
        contents
    }
}

#[async_trait]
impl LLMProvider for GeminiProvider {
    async fn chat(&self, request: ChatRequest) -> Result<Box<dyn Stream<Item = Result<String>> + Unpin + Send>> {
        let url = if request.stream {
            format!(
                "https://generativelanguage.googleapis.com/v1beta/models/{}:streamGenerateContent?alt=sse&key={}",
                request.model, self.api_key
            )
        } else {
            format!(
                "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
                request.model, self.api_key
            )
        };
        
        let contents = self.convert_messages(&request.messages);
        
        // Build generation config with thinking support
        let mut generation_config = json!({
            "temperature": request.temperature,
            "maxOutputTokens": request.max_tokens,
        });
        
        // Add thinking config if thinking is enabled
        if request.thinking {
            generation_config["thinkingConfig"] = json!({
                "includeThoughts": true,
                "thinkingLevel": "HIGH"
            });
        } else {
            // Explicitly set LOW when thinking is off to minimize latency
            generation_config["thinkingConfig"] = json!({
                "includeThoughts": false,
                "thinkingLevel": "LOW"
            });
        }
        
        let payload = json!({
            "contents": contents,
            "generationConfig": generation_config
        });
        
        let response = self.make_request_with_retry(&url, &payload).await?;
        
        if request.stream {
            // Handle streaming response with proper SSE parsing
            use futures::stream::unfold;
            use futures::StreamExt;
            
            let buffer = String::new();
            let byte_stream = response.bytes_stream();
            
            let stream = unfold(
                (buffer, byte_stream, Vec::<String>::new()),
                move |(mut buffer, mut byte_stream, mut pending_content)| async move {
                    // First, check if we have pending content to yield
                    if let Some(content) = pending_content.pop() {
                        return Some((Ok(content), (buffer, byte_stream, pending_content)));
                    }
                    
                    loop {
                        match byte_stream.next().await {
                            Some(Ok(bytes)) => {
                                let chunk = String::from_utf8_lossy(&bytes);
                                buffer.push_str(&chunk);
                                
                                // Process complete lines ending with \n
                                while let Some(newline_pos) = buffer.find('\n') {
                                    let line = buffer[..newline_pos].trim().to_string();
                                    buffer = buffer[newline_pos + 1..].to_string();
                                    
                                    if line.is_empty() {
                                        continue;
                                    }
                                    
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
                                            if let Some(candidates) = json_val.get("candidates") {
                                                if let Some(candidates_array) = candidates.as_array() {
                                                    if let Some(candidate) = candidates_array.first() {
                                                        if let Some(content_obj) = candidate.get("content") {
                                                            if let Some(parts) = content_obj.get("parts") {
                                                                if let Some(parts_array) = parts.as_array() {
                                                                    for part in parts_array {
                                                                        // Check if this part is a thought
                                                                        let is_thought = part.get("thought")
                                                                            .and_then(|v| v.as_bool())
                                                                            .unwrap_or(false);
                                                                        
                                                                        // Get the text content
                                                                        if let Some(text) = part.get("text") {
                                                                            if let Some(text_str) = text.as_str() {
                                                                                if !text_str.is_empty() {
                                                                                    if is_thought {
                                                                                        // Prefix thinking content
                                                                                        pending_content.insert(0, format!("thinking:{}", text_str));
                                                                                    } else {
                                                                                        // Prefix regular content
                                                                                        pending_content.insert(0, format!("content:{}", text_str));
                                                                                    }
                                                                                }
                                                                            }
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                        }
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
                                                if let Some(candidates) = json_val.get("candidates") {
                                                    if let Some(candidates_array) = candidates.as_array() {
                                                        if let Some(candidate) = candidates_array.first() {
                                                            if let Some(content_obj) = candidate.get("content") {
                                                                if let Some(parts) = content_obj.get("parts") {
                                                                    if let Some(parts_array) = parts.as_array() {
                                                                        for part in parts_array {
                                                                            // Check if this part is a thought
                                                                            let is_thought = part.get("thought")
                                                                                .and_then(|v| v.as_bool())
                                                                                .unwrap_or(false);
                                                                            
                                                                            // Get the text content
                                                                            if let Some(text) = part.get("text") {
                                                                                if let Some(text_str) = text.as_str() {
                                                                                    if !text_str.is_empty() {
                                                                                        if is_thought {
                                                                                            // Prefix thinking content
                                                                                            pending_content.insert(0, format!("thinking:{}", text_str));
                                                                                        } else {
                                                                                            // Prefix regular content
                                                                                            pending_content.insert(0, format!("content:{}", text_str));
                                                                                        }
                                                                                    }
                                                                                }
                                                                            }
                                                                        }
                                                                    }
                                                                }
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
            
            let mut full_content = String::new();
            let mut has_thinking = false;
            let mut has_content = false;
            
            // Extract both thought and text content
            if let Some(candidates) = json_response.get("candidates") {
                if let Some(candidates_array) = candidates.as_array() {
                    if let Some(candidate) = candidates_array.first() {
                        if let Some(content_obj) = candidate.get("content") {
                            if let Some(parts) = content_obj.get("parts") {
                                if let Some(parts_array) = parts.as_array() {
                                    // First pass: collect all thinking content
                                    for part in parts_array {
                                        let is_thought = part.get("thought")
                                            .and_then(|v| v.as_bool())
                                            .unwrap_or(false);
                                        
                                        if is_thought {
                                            if let Some(text) = part.get("text") {
                                                if let Some(text_str) = text.as_str() {
                                                    if !text_str.is_empty() {
                                                        if !has_thinking {
                                                            has_thinking = true;
                                                        }
                                                        full_content.push_str(&format!("thinking:{}", text_str));
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    
                                    // Second pass: collect all regular content
                                    for part in parts_array {
                                        let is_thought = part.get("thought")
                                            .and_then(|v| v.as_bool())
                                            .unwrap_or(false);
                                        
                                        if !is_thought {
                                            if let Some(text) = part.get("text") {
                                                if let Some(text_str) = text.as_str() {
                                                    if !text_str.is_empty() {
                                                        if !has_content {
                                                            has_content = true;
                                                        }
                                                        full_content.push_str(&format!("content:{}", text_str));
                                                    }
                                                }
                                            }
                                        }
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
            
            let stream = futures::stream::once(async move { Ok(full_content) });
            Ok(Box::new(Box::pin(stream)))
        }
    }
    
    fn get_models(&self) -> Vec<String> {
        vec![
            "gemini-3-pro-preview".to_string(),
            "gemini-2.5-pro".to_string(),
            "gemini-2.5-flash".to_string(),
            "gemini-2.5-flash-lite".to_string(),
        ]
    }
    
    fn get_name(&self) -> &str {
        "gemini"
    }
    
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
