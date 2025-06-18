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
        const MIN_EXCERPT_LENGTH: usize = 120;  // Minimum excerpt length
        const MAX_EXCERPT_LENGTH: usize = 300;  // Maximum excerpt length
        const CONTEXT_LENGTH: usize = 60;       // Context around match
        
        let content_lower = content.to_lowercase();
        
        // Find the first occurrence of the term
        if let Some(match_start) = content_lower.find(term_lower) {
            let match_end = match_start + term_lower.len();
            
            // Start with desired context around the match
            let mut excerpt_start = match_start.saturating_sub(CONTEXT_LENGTH);
            let mut excerpt_end = std::cmp::min(content.len(), match_end + CONTEXT_LENGTH);
            
            // Ensure minimum excerpt length
            let current_length = excerpt_end - excerpt_start;
            if current_length < MIN_EXCERPT_LENGTH {
                let needed_extra = MIN_EXCERPT_LENGTH - current_length;
                let extra_before = needed_extra / 2;
                let extra_after = needed_extra - extra_before;
                
                // Extend backwards if possible
                if excerpt_start >= extra_before {
                    excerpt_start -= extra_before;
                } else {
                    // Can't extend backwards enough, extend forwards more
                    let remaining = extra_before - excerpt_start;
                    excerpt_start = 0;
                    excerpt_end = std::cmp::min(content.len(), excerpt_end + extra_after + remaining);
                }
                
                // Extend forwards if possible
                if excerpt_end + extra_after <= content.len() {
                    excerpt_end += extra_after;
                } else {
                    // Can't extend forwards enough, extend backwards more if possible
                    let remaining = extra_after - (content.len() - excerpt_end);
                    excerpt_end = content.len();
                    excerpt_start = excerpt_start.saturating_sub(remaining);
                }
            }
            
            // Find good break points (word boundaries)
            if excerpt_start > 0 {
                if let Some(space_pos) = content[excerpt_start..match_start].rfind(' ') {
                    excerpt_start += space_pos + 1;
                }
            }
            
            if excerpt_end < content.len() {
                if let Some(space_pos) = content[match_end..excerpt_end].find(' ') {
                    excerpt_end = match_end + space_pos;
                }
            }
            
            // Ensure we don't exceed maximum length
            if excerpt_end - excerpt_start > MAX_EXCERPT_LENGTH {
                let excess = (excerpt_end - excerpt_start) - MAX_EXCERPT_LENGTH;
                excerpt_end -= excess;
                
                // Re-find word boundary
                if excerpt_end < content.len() {
                    if let Some(space_pos) = content[match_end..excerpt_end].rfind(' ') {
                        excerpt_end = match_end + space_pos;
                    }
                }
            }
            
            let mut excerpt = String::new();
            
            // Add leading ellipsis if we're not at the start
            if excerpt_start > 0 {
                excerpt.push_str("...");
            }
            
            // Add the excerpt with highlighted term using terminal colors
            let before_match = &content[excerpt_start..match_start];
            let matched_term = &content[match_start..match_end];
            let after_match = &content[match_end..excerpt_end];
            
            excerpt.push_str(before_match);
            excerpt.push_str("\x1b[1;33m");  // Bold yellow for highlighting
            excerpt.push_str(matched_term);
            excerpt.push_str("\x1b[0m");     // Reset formatting
            excerpt.push_str(after_match);
            
            // Add trailing ellipsis if we're not at the end
            if excerpt_end < content.len() {
                excerpt.push_str("...");
            }
            
            excerpt
        } else {
            // Fallback: just show the beginning of the content
            if content.len() > MIN_EXCERPT_LENGTH {
                format!("{}...", &content[..MIN_EXCERPT_LENGTH])
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
