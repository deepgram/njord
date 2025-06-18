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
    
    pub fn search_all_sessions(&self, term: &str, current_session: &ChatSession) -> Vec<SearchResult> {
        let mut results = Vec::new();
        let term_lower = term.to_lowercase();
        
        // Search current session first
        if !current_session.messages.is_empty() {
            let session_results = self.search_session_messages("current", &current_session.messages, &term_lower);
            results.extend(session_results);
        }
        
        // Search saved sessions
        for (session_name, session) in &self.saved_sessions {
            let session_results = self.search_session_messages(session_name, &session.messages, &term_lower);
            results.extend(session_results);
        }
        
        // Sort by session name (current first) then by message number
        results.sort_by(|a, b| {
            match (a.session_name.as_str(), b.session_name.as_str()) {
                ("current", "current") => a.message_number.cmp(&b.message_number),
                ("current", _) => std::cmp::Ordering::Less,
                (_, "current") => std::cmp::Ordering::Greater,
                (a_name, b_name) => a_name.cmp(b_name).then(a.message_number.cmp(&b.message_number)),
            }
        });
        
        results
    }
    
    fn search_session_messages(&self, session_name: &str, messages: &[crate::session::NumberedMessage], term_lower: &str) -> Vec<SearchResult> {
        let mut results = Vec::new();
        
        for numbered_message in messages {
            let content_lower = numbered_message.message.content.to_lowercase();
            if content_lower.contains(term_lower) {
                let excerpt = self.create_excerpt(&numbered_message.message.content, term_lower);
                results.push(SearchResult {
                    session_name: session_name.to_string(),
                    message_number: numbered_message.number,
                    role: numbered_message.message.role.clone(),
                    excerpt,
                });
            }
        }
        
        results
    }
    
    fn create_excerpt(&self, content: &str, term_lower: &str) -> String {
        const EXCERPT_LENGTH: usize = 120;
        const CONTEXT_LENGTH: usize = 40;
        
        let content_lower = content.to_lowercase();
        
        // Find the first occurrence of the term
        if let Some(match_start) = content_lower.find(term_lower) {
            let match_end = match_start + term_lower.len();
            
            // Calculate excerpt bounds
            let excerpt_start = if match_start > CONTEXT_LENGTH {
                // Find a good break point (space) before the match
                let ideal_start = match_start.saturating_sub(CONTEXT_LENGTH);
                content[ideal_start..match_start]
                    .rfind(' ')
                    .map(|pos| ideal_start + pos + 1)
                    .unwrap_or(ideal_start)
            } else {
                0
            };
            
            let excerpt_end = if content.len() > match_end + CONTEXT_LENGTH {
                // Find a good break point (space) after the match
                let ideal_end = std::cmp::min(content.len(), match_end + CONTEXT_LENGTH);
                content[match_end..ideal_end]
                    .find(' ')
                    .map(|pos| match_end + pos)
                    .unwrap_or(ideal_end)
            } else {
                content.len()
            };
            
            let mut excerpt = String::new();
            
            // Add leading ellipsis if we're not at the start
            if excerpt_start > 0 {
                excerpt.push_str("...");
            }
            
            // Add the excerpt with highlighted term
            let before_match = &content[excerpt_start..match_start];
            let matched_term = &content[match_start..match_end];
            let after_match = &content[match_end..excerpt_end];
            
            excerpt.push_str(before_match);
            excerpt.push_str("**");
            excerpt.push_str(matched_term);
            excerpt.push_str("**");
            excerpt.push_str(after_match);
            
            // Add trailing ellipsis if we're not at the end
            if excerpt_end < content.len() {
                excerpt.push_str("...");
            }
            
            // Ensure the excerpt isn't too long
            if excerpt.len() > EXCERPT_LENGTH + 20 { // +20 for ellipsis and highlighting
                excerpt.truncate(EXCERPT_LENGTH);
                excerpt.push_str("...");
            }
            
            excerpt
        } else {
            // Fallback: just show the beginning of the content
            if content.len() > EXCERPT_LENGTH {
                format!("{}...", &content[..EXCERPT_LENGTH])
            } else {
                content.to_string()
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub session_name: String,
    pub message_number: usize,
    pub role: String,
    pub excerpt: String,
}
