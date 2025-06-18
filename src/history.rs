use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use crate::session::ChatSession;

const HISTORY_FILE: &str = ".llm-history";

#[derive(Debug, Serialize, Deserialize)]
pub struct History {
    pub current_session: Option<ChatSession>,
    pub saved_sessions: HashMap<String, ChatSession>,
}

impl History {
    pub fn new() -> Self {
        Self {
            current_session: None,
            saved_sessions: HashMap::new(),
        }
    }
    
    pub fn load() -> Result<Self> {
        let path = PathBuf::from(HISTORY_FILE);
        
        if !path.exists() {
            return Ok(Self::new());
        }
        
        let content = fs::read_to_string(&path)?;
        let history: History = serde_json::from_str(&content)?;
        Ok(history)
    }
    
    pub fn save(&self) -> Result<()> {
        let content = serde_json::to_string_pretty(self)?;
        fs::write(HISTORY_FILE, content)?;
        Ok(())
    }
    
    pub fn set_current_session(&mut self, session: ChatSession) {
        self.current_session = Some(session);
    }
    
    pub fn save_session(&mut self, name: String, mut session: ChatSession) -> Result<()> {
        // Set the session name when saving
        session.name = Some(name.clone());
        self.saved_sessions.insert(name, session);
        self.save()?;
        Ok(())
    }
    
    pub fn load_session(&self, name: &str) -> Option<&ChatSession> {
        self.saved_sessions.get(name)
    }
    
    pub fn list_sessions(&self) -> Vec<&String> {
        self.saved_sessions.keys().collect()
    }
    
    pub fn delete_session(&mut self, name: &str) -> Result<bool> {
        let existed = self.saved_sessions.remove(name).is_some();
        if existed {
            self.save()?;
        }
        Ok(existed)
    }
    
    pub fn auto_save_session(&mut self, session: &ChatSession) -> Result<Option<String>> {
        if !session.should_auto_save() {
            return Ok(None);
        }
        
        let auto_name = session.generate_auto_name();
        let mut session_to_save = session.clone();
        session_to_save.name = Some(auto_name.clone());
        
        self.saved_sessions.insert(auto_name.clone(), session_to_save);
        self.save()?;
        Ok(Some(auto_name))
    }
    
    pub fn get_recent_sessions(&self, limit: usize) -> Vec<(&String, &ChatSession)> {
        let mut sessions: Vec<_> = self.saved_sessions.iter().collect();
        sessions.sort_by(|a, b| b.1.updated_at.cmp(&a.1.updated_at));
        sessions.into_iter().take(limit).collect()
    }
    
    pub fn get_most_recent_session(&self) -> Option<&ChatSession> {
        self.saved_sessions
            .values()
            .max_by_key(|session| session.updated_at)
    }
}
