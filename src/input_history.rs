use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::fs;
use std::path::PathBuf;
use chrono::{DateTime, Utc};

const MAX_HISTORY_ENTRIES: usize = 1000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputHistoryEntry {
    pub input: String,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct InputHistory {
    entries: VecDeque<InputHistoryEntry>,
    #[serde(skip)]
    file_path: String,
}

impl InputHistory {
    pub fn new(file_path: String) -> Self {
        Self {
            entries: VecDeque::new(),
            file_path,
        }
    }
    
    pub fn load(file_path: String) -> Result<Self> {
        let path = PathBuf::from(&file_path);
        
        if !path.exists() {
            return Ok(Self::new(file_path));
        }
        
        let content = fs::read_to_string(&path)?;
        let mut history: InputHistory = serde_json::from_str(&content)?;
        history.file_path = file_path;
        
        Ok(history)
    }
    
    pub fn save(&self) -> Result<()> {
        let content = serde_json::to_string_pretty(self)?;
        fs::write(&self.file_path, content)?;
        Ok(())
    }
    
    pub fn add_entry(&mut self, input: String) {
        // Skip empty or whitespace-only inputs
        if input.trim().is_empty() {
            return;
        }
        
        // Skip special control signals
        if input == "__CTRL_C__" {
            return;
        }
        
        // Don't add duplicate consecutive entries
        if let Some(last_entry) = self.entries.back() {
            if last_entry.input == input {
                return;
            }
        }
        
        let entry = InputHistoryEntry {
            input,
            timestamp: Utc::now(),
        };
        
        self.entries.push_back(entry);
        
        // Keep only the last MAX_HISTORY_ENTRIES
        while self.entries.len() > MAX_HISTORY_ENTRIES {
            self.entries.pop_front();
        }
    }
    
    pub fn get_entries(&self) -> Vec<String> {
        self.entries.iter().map(|entry| entry.input.clone()).collect()
    }
    
    pub fn clear(&mut self) {
        self.entries.clear();
    }
    
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_input_history_basic() {
        let mut history = InputHistory::new("test.json".to_string());
        
        assert_eq!(history.len(), 0);
        assert!(history.is_empty());
        
        history.add_entry("test command".to_string());
        assert_eq!(history.len(), 1);
        assert!(!history.is_empty());
        
        let entries = history.get_entries();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0], "test command");
    }
    
    #[test]
    fn test_input_history_duplicates() {
        let mut history = InputHistory::new("test.json".to_string());
        
        history.add_entry("test command".to_string());
        history.add_entry("test command".to_string()); // Duplicate
        history.add_entry("different command".to_string());
        history.add_entry("different command".to_string()); // Duplicate
        
        assert_eq!(history.len(), 2);
        let entries = history.get_entries();
        assert_eq!(entries, vec!["test command", "different command"]);
    }
    
    #[test]
    fn test_input_history_empty_inputs() {
        let mut history = InputHistory::new("test.json".to_string());
        
        history.add_entry("".to_string());
        history.add_entry("   ".to_string());
        history.add_entry("valid command".to_string());
        history.add_entry("\n\t  \n".to_string());
        
        assert_eq!(history.len(), 1);
        let entries = history.get_entries();
        assert_eq!(entries, vec!["valid command"]);
    }
    
    #[test]
    fn test_input_history_save_load() -> Result<()> {
        let temp_file = NamedTempFile::new()?;
        let file_path = temp_file.path().to_string_lossy().to_string();
        
        // Create and populate history
        {
            let mut history = InputHistory::new(file_path.clone());
            history.add_entry("command 1".to_string());
            history.add_entry("command 2".to_string());
            history.save()?;
        }
        
        // Load history from file
        let loaded_history = InputHistory::load(file_path)?;
        assert_eq!(loaded_history.len(), 2);
        
        let entries = loaded_history.get_entries();
        assert_eq!(entries, vec!["command 1", "command 2"]);
        
        Ok(())
    }
    
    #[test]
    fn test_input_history_max_entries() {
        let mut history = InputHistory::new("test.json".to_string());
        
        // Add more than MAX_HISTORY_ENTRIES
        for i in 0..1200 {
            history.add_entry(format!("command {}", i));
        }
        
        assert_eq!(history.len(), MAX_HISTORY_ENTRIES);
        
        let entries = history.get_entries();
        // Should have the last 1000 entries
        assert_eq!(entries[0], "command 200");
        assert_eq!(entries[999], "command 1199");
    }
}
