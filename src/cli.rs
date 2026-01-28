use clap::Parser;

/// Returns the default state directory following XDG Base Directory specification.
/// Uses $XDG_DATA_HOME/njord if set, otherwise ~/.local/share/njord
pub fn default_state_directory() -> String {
    if let Ok(xdg_data_home) = std::env::var("XDG_DATA_HOME") {
        format!("{}/njord", xdg_data_home)
    } else if let Some(home) = dirs::home_dir() {
        format!("{}/.local/share/njord", home.display())
    } else {
        ".".to_string() // Fallback to current directory
    }
}

#[derive(Parser, Debug)]
#[command(name = "njord")]
#[command(about = "Interactive LLM REPL - Navigate the vast ocean of AI conversations")]
#[command(version)]
pub struct Args {
    /// OpenAI API key
    #[arg(long)]
    pub openai_key: Option<String>,
    
    /// Anthropic API key
    #[arg(long)]
    pub anthropic_key: Option<String>,
    
    /// Google Gemini API key
    #[arg(long)]
    pub gemini_key: Option<String>,
    
    /// Default model to use
    #[arg(short, long, default_value = "claude-sonnet-4-20250514")]
    pub model: String,
    
    /// Temperature for responses (0.0 to 2.0)
    #[arg(short, long, default_value = "0.7")]
    pub temperature: f32,
    
    /// Maximum tokens for responses
    #[arg(long, default_value = "4096")]
    pub max_tokens: u32,
    
    /// Thinking token budget for supported models
    #[arg(long, default_value = "20000")]
    pub thinking_budget: u32,
    
    /// Load a specific chat session
    #[arg(long)]
    pub load_session: Option<String>,
    
    /// Start with a new chat session
    #[arg(long)]
    pub new_session: bool,
    
    /// State directory for njord data files (sessions, prompts, inputs)
    #[arg(long, default_value_t = default_state_directory())]
    pub state_directory: String,
    
    /// Run in ephemeral mode (don't save any changes to disk)
    #[arg(long)]
    pub ephemeral: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_state_directory_returns_valid_path() {
        let result = default_state_directory();
        // Should either end with njord (if XDG or home is set) or be "." (fallback)
        assert!(
            result.ends_with("/njord") || result == ".",
            "Expected path ending with /njord or '.', got: {}",
            result
        );
    }
}
