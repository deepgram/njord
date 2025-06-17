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
    pub temperature: f32,
    pub system_prompt: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NumberedMessage {
    pub number: usize,
    pub message: Message,
    pub timestamp: DateTime<Utc>,
    pub code_blocks: Vec<CodeBlock>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeBlock {
    pub number: usize,
    pub language: Option<String>,
    pub content: String,
}

impl ChatSession {
    pub fn new(model: String, temperature: f32) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            name: None,
            created_at: now,
            updated_at: now,
            messages: Vec::new(),
            current_model: model,
            temperature,
            system_prompt: None,
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
        });
        
        self.updated_at = Utc::now();
        number
    }
    
    fn extract_code_blocks(&self, _content: &str) -> Vec<CodeBlock> {
        // TODO: Implement code block extraction from markdown
        Vec::new()
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
}
