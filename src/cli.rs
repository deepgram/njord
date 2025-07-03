use clap::Parser;

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
    
    /// Path to the history file (default: .njord)
    #[arg(long, default_value = crate::history::HISTORY_FILE)]
    pub history_file: String,
}
