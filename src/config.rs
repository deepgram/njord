use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::cli::Args;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub api_keys: HashMap<String, String>,
    pub default_model: String,
    pub temperature: f32,
    pub max_tokens: u32,
    pub thinking_budget: u32,
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
            max_tokens: args.max_tokens,
            thinking_budget: args.thinking_budget,
            load_session: args.load_session.clone(),
            new_session: args.new_session,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::Args;

    #[test]
    fn test_config_from_args_with_cli_keys() {
        let args = Args {
            openai_key: Some("test-openai-key".to_string()),
            anthropic_key: Some("test-anthropic-key".to_string()),
            gemini_key: Some("test-gemini-key".to_string()),
            model: "gpt-4".to_string(),
            temperature: 0.8,
            max_tokens: 2000,
            thinking_budget: 10000,
            load_session: Some("test-session".to_string()),
            new_session: true,
        };
        
        let config = Config::from_args(&args).unwrap();
        
        assert_eq!(config.api_keys.get("openai"), Some(&"test-openai-key".to_string()));
        assert_eq!(config.api_keys.get("anthropic"), Some(&"test-anthropic-key".to_string()));
        assert_eq!(config.api_keys.get("gemini"), Some(&"test-gemini-key".to_string()));
        assert_eq!(config.temperature, 0.8);
        assert_eq!(config.max_tokens, 2000);
        assert_eq!(config.thinking_budget, 10000);
        assert_eq!(config.load_session, Some("test-session".to_string()));
        assert!(config.new_session);
    }

    #[test]
    fn test_config_default_model_selection() {
        // Test with Anthropic key - should default to Claude
        let args_anthropic = Args {
            openai_key: None,
            anthropic_key: Some("test-key".to_string()),
            gemini_key: None,
            model: "claude-sonnet-4-20250514".to_string(),
            temperature: 0.7,
            max_tokens: 4096,
            thinking_budget: 20000,
            load_session: None,
            new_session: false,
        };
        
        let config = Config::from_args(&args_anthropic).unwrap();
        assert_eq!(config.default_model, "claude-sonnet-4-20250514");
        
        // Test with OpenAI key - should default to o3-pro
        let args_openai = Args {
            openai_key: Some("test-key".to_string()),
            anthropic_key: None,
            gemini_key: None,
            model: "claude-sonnet-4-20250514".to_string(),
            temperature: 0.7,
            max_tokens: 4096,
            thinking_budget: 20000,
            load_session: None,
            new_session: false,
        };
        
        let config = Config::from_args(&args_openai).unwrap();
        assert_eq!(config.default_model, "o3-pro");
        
        // Test with Gemini key - should default to gemini-2.5-pro
        let args_gemini = Args {
            openai_key: None,
            anthropic_key: None,
            gemini_key: Some("test-key".to_string()),
            model: "claude-sonnet-4-20250514".to_string(),
            temperature: 0.7,
            max_tokens: 4096,
            thinking_budget: 20000,
            load_session: None,
            new_session: false,
        };
        
        let config = Config::from_args(&args_gemini).unwrap();
        assert_eq!(config.default_model, "gemini-2.5-pro");
    }

    #[test]
    fn test_config_no_api_keys() {
        // Store original values to restore later
        let original_openai = std::env::var("OPENAI_API_KEY").ok();
        let original_anthropic = std::env::var("ANTHROPIC_API_KEY").ok();
        let original_gemini = std::env::var("GEMINI_API_KEY").ok();
        
        // Clear any environment variables that might interfere
        std::env::remove_var("OPENAI_API_KEY");
        std::env::remove_var("ANTHROPIC_API_KEY");
        std::env::remove_var("GEMINI_API_KEY");
        
        let args = Args {
            openai_key: None,
            anthropic_key: None,
            gemini_key: None,
            model: "gpt-4".to_string(),
            temperature: 0.7,
            max_tokens: 4096,
            thinking_budget: 20000,
            load_session: None,
            new_session: false,
        };
        
        let config = Config::from_args(&args).unwrap();
        assert!(config.api_keys.is_empty());
        assert_eq!(config.default_model, "gpt-4"); // Falls back to CLI arg
        
        // Restore original environment variables
        if let Some(key) = original_openai {
            std::env::set_var("OPENAI_API_KEY", key);
        }
        if let Some(key) = original_anthropic {
            std::env::set_var("ANTHROPIC_API_KEY", key);
        }
        if let Some(key) = original_gemini {
            std::env::set_var("GEMINI_API_KEY", key);
        }
    }

    #[test]
    fn test_config_precedence() {
        // Store original values to restore later
        let original_openai = std::env::var("OPENAI_API_KEY").ok();
        let original_anthropic = std::env::var("ANTHROPIC_API_KEY").ok();
        
        // Set environment variables
        std::env::set_var("OPENAI_API_KEY", "env-openai-key");
        std::env::set_var("ANTHROPIC_API_KEY", "env-anthropic-key");
        
        // CLI args should take precedence
        let args = Args {
            openai_key: Some("cli-openai-key".to_string()),
            anthropic_key: None, // This should use env var
            gemini_key: None,
            model: "gpt-4".to_string(),
            temperature: 0.7,
            max_tokens: 4096,
            thinking_budget: 20000,
            load_session: None,
            new_session: false,
        };
        
        let config = Config::from_args(&args).unwrap();
        assert_eq!(config.api_keys.get("openai"), Some(&"cli-openai-key".to_string()));
        assert_eq!(config.api_keys.get("anthropic"), Some(&"env-anthropic-key".to_string()));
        
        // Restore original environment variables
        if let Some(key) = original_openai {
            std::env::set_var("OPENAI_API_KEY", key);
        } else {
            std::env::remove_var("OPENAI_API_KEY");
        }
        if let Some(key) = original_anthropic {
            std::env::set_var("ANTHROPIC_API_KEY", key);
        } else {
            std::env::remove_var("ANTHROPIC_API_KEY");
        }
    }
}
