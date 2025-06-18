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
        
        if request.stream {
            // For now, disable streaming for Gemini and use non-streaming response
            // The Gemini streaming format is complex and needs more investigation
            let json_response: serde_json::Value = response.json().await?;
            
            // Debug: print the actual response structure
            eprintln!("Debug - Gemini response structure: {}", serde_json::to_string_pretty(&json_response).unwrap_or_default());
            
            // The response is an array of chunks, we need to concatenate all text parts
            let mut content = String::new();
            
            if let Some(chunks) = json_response.as_array() {
                for chunk in chunks {
                    if let Some(candidates) = chunk.get("candidates") {
                        if let Some(candidates_array) = candidates.as_array() {
                            if let Some(candidate) = candidates_array.first() {
                                if let Some(content_obj) = candidate.get("content") {
                                    if let Some(parts) = content_obj.get("parts") {
                                        if let Some(parts_array) = parts.as_array() {
                                            if let Some(part) = parts_array.first() {
                                                if let Some(text) = part.get("text") {
                                                    if let Some(text_str) = text.as_str() {
                                                        content.push_str(text_str);
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
            
            if content.is_empty() {
                eprintln!("Debug - Failed to parse Gemini response content");
                content = "No response content found".to_string();
            }
            
            let stream = futures::stream::once(async move { Ok(content) });
            Ok(Box::new(Box::pin(stream)))
        } else {
            // Handle non-streaming response (this path shouldn't be reached since we're using streaming URL)
            let json_response: serde_json::Value = response.json().await?;
            
            // The response is an array of chunks, we need to concatenate all text parts
            let mut content = String::new();
            
            if let Some(chunks) = json_response.as_array() {
                for chunk in chunks {
                    if let Some(candidates) = chunk.get("candidates") {
                        if let Some(candidates_array) = candidates.as_array() {
                            if let Some(candidate) = candidates_array.first() {
                                if let Some(content_obj) = candidate.get("content") {
                                    if let Some(parts) = content_obj.get("parts") {
                                        if let Some(parts_array) = parts.as_array() {
                                            if let Some(part) = parts_array.first() {
                                                if let Some(text) = part.get("text") {
                                                    if let Some(text_str) = text.as_str() {
                                                        content.push_str(text_str);
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
            
            if content.is_empty() {
                content = "No response content found".to_string();
            }
            
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
