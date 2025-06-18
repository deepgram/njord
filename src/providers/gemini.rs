use anyhow::Result;
use async_trait::async_trait;
use futures::{stream, Stream, StreamExt};
use reqwest::Client;
use serde_json::{json, Value};
use tokio_stream::wrappers::LinesStream;
use tokio_util::io::StreamReader;

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
        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:streamGenerateContent?key={}",
            request.model, self.api_key
        );
        
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
        
        let stream = response.bytes_stream();
        let reader = StreamReader::new(stream.map(|result| {
            result.map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
        }));
        
        let lines_stream = LinesStream::new(reader.lines());
        let content_stream = lines_stream.map(|line_result| {
            match line_result {
                Ok(line) => {
                    if line.trim().is_empty() {
                        return Ok("".to_string());
                    }
                    
                    // Parse the JSON response
                    match serde_json::from_str::<Value>(&line) {
                        Ok(json) => {
                            if let Some(candidates) = json.get("candidates") {
                                if let Some(candidate) = candidates.get(0) {
                                    if let Some(content) = candidate.get("content") {
                                        if let Some(parts) = content.get("parts") {
                                            if let Some(part) = parts.get(0) {
                                                if let Some(text) = part.get("text") {
                                                    if let Some(text_str) = text.as_str() {
                                                        return Ok(text_str.to_string());
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            Ok("".to_string())
                        }
                        Err(_) => Ok("".to_string()),
                    }
                }
                Err(e) => Err(anyhow::anyhow!("Stream error: {}", e)),
            }
        });
        
        Ok(Box::new(Box::pin(content_stream)))
    }
    
    fn get_models(&self) -> Vec<String> {
        vec![
            "gemini-1.5-flash".to_string(),
            "gemini-1.5-pro".to_string(),
            "gemini-1.0-pro".to_string(),
        ]
    }
    
    fn get_name(&self) -> &str {
        "gemini"
    }
}
