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
    pub state_directory: String,
}

impl Config {
    pub fn sessions_file(&self) -> String {
        format!("{}/.njord.sessions", self.state_directory)
    }
    
    pub fn prompts_file(&self) -> String {
        format!("{}/.njord.prompts", self.state_directory)
    }
    
    pub fn inputs_file(&self) -> String {
        format!("{}/.njord.inputs", self.state_directory)
    }
}

impl Config {
    pub fn from_args(args: &Args) -> Result<Self> {
        // Read environment variables once
        let env_openai = std::env::var("OPENAI_API_KEY").ok();
        let env_anthropic = std::env::var("ANTHROPIC_API_KEY").ok();
        let env_gemini = std::env::var("GEMINI_API_KEY").ok();
        
        Self::from_args_and_env(args, env_openai, env_anthropic, env_gemini)
    }
    
    pub fn from_args_and_env(
        args: &Args,
        env_openai: Option<String>,
        env_anthropic: Option<String>,
        env_gemini: Option<String>,
    ) -> Result<Self> {
        let mut api_keys = HashMap::new();
        
        // Check CLI args first, then environment variables
        if let Some(key) = &args.openai_key {
            api_keys.insert("openai".to_string(), key.clone());
        } else if let Some(key) = env_openai {
            api_keys.insert("openai".to_string(), key);
        }
        
        if let Some(key) = &args.anthropic_key {
            api_keys.insert("anthropic".to_string(), key.clone());
        } else if let Some(key) = env_anthropic {
            api_keys.insert("anthropic".to_string(), key);
        }
        
        if let Some(key) = &args.gemini_key {
            api_keys.insert("gemini".to_string(), key.clone());
        } else if let Some(key) = env_gemini {
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
            state_directory: args.state_directory.clone(),
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
            state_directory: ".".to_string(),
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
            history_file: crate::history::HISTORY_FILE.to_string(),
        };
        
        let config = Config::from_args_and_env(&args_anthropic, None, None, None).unwrap();
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
            history_file: crate::history::HISTORY_FILE.to_string(),
        };
        
        let config = Config::from_args_and_env(&args_openai, None, None, None).unwrap();
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
            history_file: crate::history::HISTORY_FILE.to_string(),
        };
        
        let config = Config::from_args_and_env(&args_gemini, None, None, None).unwrap();
        assert_eq!(config.default_model, "gemini-2.5-pro");
    }

    #[test]
    fn test_config_no_api_keys() {
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
            history_file: crate::history::HISTORY_FILE.to_string(),
        };
        
        let config = Config::from_args_and_env(&args, None, None, None).unwrap();
        assert!(config.api_keys.is_empty());
        assert_eq!(config.default_model, "gpt-4"); // Falls back to CLI arg
    }

    #[test]
    fn test_config_precedence() {
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
            history_file: crate::history::HISTORY_FILE.to_string(),
        };
        
        let config = Config::from_args_and_env(
            &args,
            Some("env-openai-key".to_string()),
            Some("env-anthropic-key".to_string()),
            None,
        ).unwrap();
        assert_eq!(config.api_keys.get("openai"), Some(&"cli-openai-key".to_string()));
        assert_eq!(config.api_keys.get("anthropic"), Some(&"env-anthropic-key".to_string()));
    }
}
