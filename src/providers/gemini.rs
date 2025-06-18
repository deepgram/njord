use anyhow::Result;
use async_trait::async_trait;
use futures::Stream;
use reqwest::Client;
use serde_json::json;

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
                "https://generativelanguage.googleapis.com/v1beta/models/{}:streamGenerateContent?key={}",
                request.model, self.api_key
            )
        } else {
            format!(
                "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
                request.model, self.api_key
            )
        };
        
        let contents = self.convert_messages(&request.messages);
        
        let payload = json!({
            "contents": contents,
            "generationConfig": {
                "temperature": request.temperature,
                "maxOutputTokens": 4096,
            }
        });
        
        let response = self.client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await?;
        
        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(anyhow::anyhow!("Gemini API error: {}", error_text));
        }
        
        if request.stream {
            // Handle streaming response
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
                                eprintln!("Debug - Gemini streaming chunk: {:?}", chunk);
                                buffer.push_str(&chunk);
                                
                                // Process complete JSON objects (Gemini sends one JSON per line)
                                while let Some(newline_pos) = buffer.find('\n') {
                                    let line = buffer[..newline_pos].trim().to_string();
                                    buffer = buffer[newline_pos + 1..].to_string();
                                    
                                    if line.is_empty() {
                                        continue;
                                    }
                                    
                                    eprintln!("Debug - Gemini streaming line: {}", line);
                                    
                                    // Parse the JSON response
                                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&line) {
                                        eprintln!("Debug - Gemini streaming JSON: {}", serde_json::to_string_pretty(&json).unwrap_or_default());
                                        if let Some(candidates) = json.get("candidates") {
                                            if let Some(candidates_array) = candidates.as_array() {
                                                if let Some(candidate) = candidates_array.first() {
                                                    if let Some(content_obj) = candidate.get("content") {
                                                        if let Some(parts) = content_obj.get("parts") {
                                                            if let Some(parts_array) = parts.as_array() {
                                                                if let Some(part) = parts_array.first() {
                                                                    if let Some(text) = part.get("text") {
                                                                        if let Some(text_str) = text.as_str() {
                                                                            if !text_str.is_empty() {
                                                                                eprintln!("Debug - Gemini streaming text found: {:?}", text_str);
                                                                                pending_content.insert(0, text_str.to_string());
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
                                    } else {
                                        eprintln!("Debug - Gemini streaming JSON parse failed for line: {}", line);
                                    }
                                }
                                
                                // If we have pending content, yield the first piece
                                if let Some(content) = pending_content.pop() {
                                    return Some((Ok(content), (buffer, byte_stream, pending_content)));
                                }
                                // Continue to next chunk if no content to yield
                            }
                            Some(Err(e)) => {
                                eprintln!("Debug - Gemini streaming error: {}", e);
                                return Some((Err(anyhow::anyhow!("Stream error: {}", e)), (buffer, byte_stream, pending_content)));
                            }
                            None => {
                                eprintln!("Debug - Gemini streaming ended");
                                // Stream ended - process any remaining complete lines in buffer
                                while let Some(newline_pos) = buffer.find('\n') {
                                    let line = buffer[..newline_pos].trim().to_string();
                                    buffer = buffer[newline_pos + 1..].to_string();
                                    
                                    if !line.is_empty() {
                                        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&line) {
                                            if let Some(candidates) = json.get("candidates") {
                                                if let Some(candidates_array) = candidates.as_array() {
                                                    if let Some(candidate) = candidates_array.first() {
                                                        if let Some(content_obj) = candidate.get("content") {
                                                            if let Some(parts) = content_obj.get("parts") {
                                                                if let Some(parts_array) = parts.as_array() {
                                                                    if let Some(part) = parts_array.first() {
                                                                        if let Some(text) = part.get("text") {
                                                                            if let Some(text_str) = text.as_str() {
                                                                                if !text_str.is_empty() {
                                                                                    pending_content.insert(0, text_str.to_string());
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
            
            eprintln!("Debug - Gemini non-streaming response: {}", serde_json::to_string_pretty(&json_response).unwrap_or_default());
            
            let content = json_response
                .get("candidates")
                .and_then(|candidates| candidates.as_array())
                .and_then(|arr| arr.first())
                .and_then(|candidate| candidate.get("content"))
                .and_then(|content| content.get("parts"))
                .and_then(|parts| parts.as_array())
                .and_then(|arr| arr.first())
                .and_then(|part| part.get("text"))
                .and_then(|text| text.as_str())
                .unwrap_or_else(|| {
                    eprintln!("Debug - Gemini non-streaming content parsing failed");
                    "No response content"
                })
                .to_string();
            
            eprintln!("Debug - Gemini non-streaming final content: {:?}", content);
            
            let stream = futures::stream::once(async move { Ok(content) });
            Ok(Box::new(Box::pin(stream)))
        }
    }
    
    fn get_models(&self) -> Vec<String> {
        vec![
            "gemini-2.5-pro".to_string(),
            "gemini-2.5-flash".to_string(),
            "gemini-2.5-flash-lite".to_string(),
        ]
    }
    
    fn get_name(&self) -> &str {
        "gemini"
    }
}
