use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::providers::Message;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatSession {
    pub id: Uuid,
    pub name: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub messages: Vec<NumberedMessage>,
    pub current_model: String,
    pub current_provider: Option<String>,
    pub temperature: f32,
    pub max_tokens: u32,
    pub thinking_budget: u32,
    pub system_prompt: Option<String>,
    pub thinking_enabled: bool,
    #[serde(default)]
    pub has_llm_interaction: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NumberedMessage {
    pub number: usize,
    pub message: Message,
    pub timestamp: DateTime<Utc>,
    pub code_blocks: Vec<CodeBlock>,
    pub provider: Option<String>,
    pub model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeBlock {
    pub number: usize,
    pub language: Option<String>,
    pub content: String,
}

impl ChatSession {
    pub fn new(model: String, temperature: f32, max_tokens: u32, thinking_budget: u32) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            name: None,
            created_at: now,
            updated_at: now,
            messages: Vec::new(),
            current_model: model,
            current_provider: None,
            temperature,
            max_tokens,
            thinking_budget,
            system_prompt: None,
            thinking_enabled: false,
            has_llm_interaction: false,
        }
    }
    
    pub fn add_message(&mut self, message: Message) -> usize {
        let number = self.messages.len() + 1;
        let code_blocks = self.extract_code_blocks(&message.content);
        
        self.messages.push(NumberedMessage {
            number,
            message,
            timestamp: Utc::now(),
            code_blocks,
            provider: None,
            model: None,
        });
        
        self.updated_at = Utc::now();
        number
    }
    
    pub fn add_message_with_metadata(&mut self, message: Message, provider: Option<String>, model: Option<String>) -> usize {
        let number = self.messages.len() + 1;
        let code_blocks = self.extract_code_blocks(&message.content);
        
        self.messages.push(NumberedMessage {
            number,
            message,
            timestamp: Utc::now(),
            code_blocks,
            provider,
            model,
        });
        
        self.updated_at = Utc::now();
        number
    }
    
    fn extract_code_blocks(&self, content: &str) -> Vec<CodeBlock> {
        let mut code_blocks = Vec::new();
        let mut block_number = 1;
        
        // Find all code blocks using regex with DOTALL flag for multiline matching
        let code_block_regex = regex::Regex::new(r"(?s)```(\w+)?\n(.*?)\n```").unwrap();
        
        for captures in code_block_regex.captures_iter(content) {
            let language = captures.get(1).map(|m| m.as_str().to_string());
            let code_content = captures.get(2).map(|m| m.as_str().to_string()).unwrap_or_default();
            
            if !code_content.trim().is_empty() {
                code_blocks.push(CodeBlock {
                    number: block_number,
                    language,
                    content: code_content,
                });
                block_number += 1;
            }
        }
        
        code_blocks
    }
    
    pub fn undo(&mut self, count: usize) -> Result<()> {
        if count > self.messages.len() {
            return Err(anyhow::anyhow!("Cannot undo {} messages, only {} available", count, self.messages.len()));
        }
        
        for _ in 0..count {
            self.messages.pop();
        }
        
        self.updated_at = Utc::now();
        Ok(())
    }
    
    pub fn goto(&mut self, message_number: usize) -> Result<()> {
        if message_number == 0 || message_number > self.messages.len() {
            return Err(anyhow::anyhow!("Invalid message number: {}", message_number));
        }
        
        self.messages.truncate(message_number);
        self.updated_at = Utc::now();
        Ok(())
    }
    
    pub fn mark_llm_interaction(&mut self) {
        self.has_llm_interaction = true;
    }
    
    pub fn should_auto_save(&self) -> bool {
        self.has_llm_interaction && !self.messages.is_empty()
    }
    
    pub fn merge_session(&mut self, other: &ChatSession) -> Result<()> {
        let mut next_number = self.messages.len() + 1;
        
        for other_msg in &other.messages {
            let mut new_msg = other_msg.clone();
            new_msg.number = next_number;
            self.messages.push(new_msg);
            next_number += 1;
        }
        
        self.updated_at = Utc::now();
        if !other.messages.is_empty() {
            self.has_llm_interaction = true;
        }
        
        Ok(())
    }
    
    pub fn generate_auto_name(&self) -> String {
        self.created_at.format("%Y-%m-%d_%H:%M:%S").to_string()
    }
    
    pub fn create_copy(&self) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            name: None,
            created_at: now,
            updated_at: now,
            messages: self.messages.clone(),
            current_model: self.current_model.clone(),
            current_provider: self.current_provider.clone(),
            temperature: self.temperature,
            max_tokens: self.max_tokens,
            thinking_budget: self.thinking_budget,
            system_prompt: self.system_prompt.clone(),
            thinking_enabled: self.thinking_enabled,
            has_llm_interaction: false, // Reset for new copy
        }
    }
}
