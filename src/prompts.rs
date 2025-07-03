use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use chrono::{DateTime, Utc};

pub const PROMPTS_FILE: &str = ".njord-prompts";

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SystemPrompt {
    pub name: String,
    pub content: String,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub usage_count: u32,
}

impl SystemPrompt {
    pub fn new(name: String, content: String) -> Self {
        let now = Utc::now();
        Self {
            name,
            content,
            description: None,
            tags: Vec::new(),
            created_at: now,
            updated_at: now,
            usage_count: 0,
        }
    }
    
    pub fn with_description(mut self, description: String) -> Self {
        self.description = Some(description);
        self
    }
    
    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }
    
    pub fn increment_usage(&mut self) {
        self.usage_count += 1;
        self.updated_at = Utc::now();
    }
    
    pub fn update_content(&mut self, content: String) {
        self.content = content;
        self.updated_at = Utc::now();
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PromptLibrary {
    pub prompts: HashMap<String, SystemPrompt>,
    #[serde(skip)]
    pub prompts_file_path: String,
}

impl PromptLibrary {
    pub fn new(prompts_file_path: String) -> Self {
        Self {
            prompts: HashMap::new(),
            prompts_file_path,
        }
    }
    
    pub fn load(prompts_file_path: String) -> Result<Self> {
        let path = PathBuf::from(&prompts_file_path);
        
        if !path.exists() {
            return Ok(Self::new(prompts_file_path));
        }
        
        let content = fs::read_to_string(&path)?;
        let mut library: PromptLibrary = serde_json::from_str(&content)?;
        library.prompts_file_path = prompts_file_path;
        Ok(library)
    }
    
    pub fn save(&self) -> Result<()> {
        let content = serde_json::to_string_pretty(self)?;
        fs::write(&self.prompts_file_path, content)?;
        Ok(())
    }
    
    pub fn save_prompt(&mut self, name: String, content: String) -> Result<()> {
        let prompt = SystemPrompt::new(name.clone(), content);
        self.prompts.insert(name, prompt);
        self.save()?;
        Ok(())
    }
    
    pub fn get_prompt(&self, name: &str) -> Option<&SystemPrompt> {
        self.prompts.get(name)
    }
    
    pub fn get_prompt_mut(&mut self, name: &str) -> Option<&mut SystemPrompt> {
        self.prompts.get_mut(name)
    }
    
    pub fn apply_prompt(&mut self, name: &str) -> Option<String> {
        if let Some(prompt) = self.prompts.get_mut(name) {
            prompt.increment_usage();
            let content = prompt.content.clone();
            let _ = self.save(); // Best effort save
            Some(content)
        } else {
            None
        }
    }
    
    pub fn list_prompts(&self) -> Vec<&String> {
        let mut names: Vec<_> = self.prompts.keys().collect();
        names.sort_by(|a, b| {
            // Sort by usage count (descending), then by name
            let prompt_a = &self.prompts[*a];
            let prompt_b = &self.prompts[*b];
            prompt_b.usage_count.cmp(&prompt_a.usage_count)
                .then(a.cmp(b))
        });
        names
    }
    
    pub fn delete_prompt(&mut self, name: &str) -> Result<bool> {
        let existed = self.prompts.remove(name).is_some();
        if existed {
            self.save()?;
        }
        Ok(existed)
    }
    
    pub fn rename_prompt(&mut self, old_name: &str, new_name: &str) -> Result<bool> {
        if !self.prompts.contains_key(old_name) {
            return Ok(false);
        }
        
        if self.prompts.contains_key(new_name) {
            return Err(anyhow::anyhow!("Prompt '{}' already exists", new_name));
        }
        
        if let Some(mut prompt) = self.prompts.remove(old_name) {
            prompt.name = new_name.to_string();
            prompt.updated_at = Utc::now();
            self.prompts.insert(new_name.to_string(), prompt);
            self.save()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }
    
    pub fn search_prompts(&self, term: &str) -> Vec<PromptSearchResult> {
        let mut results = Vec::new();
        let term_lower = term.to_lowercase();
        
        for (name, prompt) in &self.prompts {
            let mut relevance_score = 0;
            let mut matched_fields = Vec::new();
            
            // Check name match
            if name.to_lowercase().contains(&term_lower) {
                relevance_score += 10;
                matched_fields.push("name".to_string());
            }
            
            // Check content match
            if prompt.content.to_lowercase().contains(&term_lower) {
                relevance_score += 5;
                matched_fields.push("content".to_string());
            }
            
            // Check description match
            if let Some(ref description) = prompt.description {
                if description.to_lowercase().contains(&term_lower) {
                    relevance_score += 7;
                    matched_fields.push("description".to_string());
                }
            }
            
            // Check tags match
            for tag in &prompt.tags {
                if tag.to_lowercase().contains(&term_lower) {
                    relevance_score += 8;
                    matched_fields.push("tags".to_string());
                    break; // Only count tags once
                }
            }
            
            if relevance_score > 0 {
                results.push(PromptSearchResult {
                    name: name.clone(),
                    prompt: prompt.clone(),
                    relevance_score,
                    matched_fields,
                });
            }
        }
        
        // Sort by relevance score (descending), then by usage count, then by name
        results.sort_by(|a, b| {
            b.relevance_score.cmp(&a.relevance_score)
                .then(b.prompt.usage_count.cmp(&a.prompt.usage_count))
                .then(a.name.cmp(&b.name))
        });
        
        results
    }
    
    pub fn update_prompt_content(&mut self, name: &str, content: String) -> Result<bool> {
        if let Some(prompt) = self.prompts.get_mut(name) {
            prompt.update_content(content);
            self.save()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }
    
    pub fn export_prompts(&self, file_path: Option<&str>) -> Result<String> {
        let export_data = serde_json::to_string_pretty(&self.prompts)?;
        
        if let Some(path) = file_path {
            fs::write(path, &export_data)?;
            Ok(format!("Exported {} prompts to {}", self.prompts.len(), path))
        } else {
            Ok(export_data)
        }
    }
    
    pub fn import_prompts(&mut self, file_path: &str, overwrite: bool) -> Result<ImportResult> {
        let content = fs::read_to_string(file_path)?;
        let imported_prompts: HashMap<String, SystemPrompt> = serde_json::from_str(&content)?;
        
        let mut imported_count = 0;
        let mut skipped_count = 0;
        let mut overwritten_count = 0;
        
        for (name, prompt) in imported_prompts {
            if self.prompts.contains_key(&name) {
                if overwrite {
                    self.prompts.insert(name, prompt);
                    overwritten_count += 1;
                } else {
                    skipped_count += 1;
                }
            } else {
                self.prompts.insert(name, prompt);
                imported_count += 1;
            }
        }
        
        if imported_count > 0 || overwritten_count > 0 {
            self.save()?;
        }
        
        Ok(ImportResult {
            imported_count,
            skipped_count,
            overwritten_count,
        })
    }
    
    pub fn ensure_unique_prompt_name(&self, base_name: &str) -> String {
        if !self.prompts.contains_key(base_name) {
            return base_name.to_string();
        }
        
        for i in 2..=999 {
            let candidate = format!("{} ({})", base_name, i);
            if !self.prompts.contains_key(&candidate) {
                return candidate;
            }
        }
        
        // Fallback with timestamp if we somehow hit 999 duplicates
        format!("{} ({})", base_name, Utc::now().format("%H:%M:%S"))
    }
}

#[derive(Debug, Clone)]
pub struct PromptSearchResult {
    pub name: String,
    pub prompt: SystemPrompt,
    pub relevance_score: u32,
    pub matched_fields: Vec<String>,
}

#[derive(Debug)]
pub struct ImportResult {
    pub imported_count: usize,
    pub skipped_count: usize,
    pub overwritten_count: usize,
}
