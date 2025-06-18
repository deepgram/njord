use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::cli::Args;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub api_keys: HashMap<String, String>,
    pub default_model: String,
    pub temperature: f32,
    pub load_session: Option<String>,
    pub new_session: bool,
}

impl Config {
    pub fn from_args(args: &Args) -> Result<Self> {
        let mut api_keys = HashMap::new();
        
        // Check CLI args first, then environment variables
        if let Some(key) = &args.openai_key {
            api_keys.insert("openai".to_string(), key.clone());
        } else if let Ok(key) = std::env::var("OPENAI_API_KEY") {
            api_keys.insert("openai".to_string(), key);
        }
        
        if let Some(key) = &args.anthropic_key {
            api_keys.insert("anthropic".to_string(), key.clone());
        } else if let Ok(key) = std::env::var("ANTHROPIC_API_KEY") {
            api_keys.insert("anthropic".to_string(), key);
        }
        
        if let Some(key) = &args.gemini_key {
            api_keys.insert("gemini".to_string(), key.clone());
        } else if let Ok(key) = std::env::var("GEMINI_API_KEY") {
            api_keys.insert("gemini".to_string(), key);
        }
        
        // Update default model to use the first model from the first available provider
        let default_model = if api_keys.contains_key("anthropic") {
            "claude-sonnet-4-20250514".to_string()
        } else if api_keys.contains_key("openai") {
            "o3-pro".to_string()
        } else if api_keys.contains_key("gemini") {
            "gemini-2.5-pro".to_string()
        } else {
            args.model.clone() // Fallback to CLI arg if no providers available
        };
        
        Ok(Config {
            api_keys,
            default_model,
            temperature: args.temperature,
            load_session: args.load_session.clone(),
            new_session: args.new_session,
        })
    }
}
