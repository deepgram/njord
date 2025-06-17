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
        
        if let Some(key) = &args.openai_key {
            api_keys.insert("openai".to_string(), key.clone());
        }
        
        if let Some(key) = &args.anthropic_key {
            api_keys.insert("anthropic".to_string(), key.clone());
        }
        
        if let Some(key) = &args.gemini_key {
            api_keys.insert("gemini".to_string(), key.clone());
        }
        
        Ok(Config {
            api_keys,
            default_model: args.model.clone(),
            temperature: args.temperature,
            load_session: args.load_session.clone(),
            new_session: args.new_session,
        })
    }
}
