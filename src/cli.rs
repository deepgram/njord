use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "njord")]
#[command(about = "Interactive LLM REPL - Navigate the vast ocean of AI conversations")]
#[command(version)]
pub struct Args {
    /// OpenAI API key
    #[arg(long, env)]
    pub openai_key: Option<String>,
    
    /// Anthropic API key
    #[arg(long, env)]
    pub anthropic_key: Option<String>,
    
    /// Google Gemini API key
    #[arg(long, env)]
    pub gemini_key: Option<String>,
    
    /// Default model to use
    #[arg(short, long, default_value = "gpt-3.5-turbo")]
    pub model: String,
    
    /// Temperature for responses (0.0 to 2.0)
    #[arg(short, long, default_value = "0.7")]
    pub temperature: f32,
    
    /// Load a specific chat session
    #[arg(long)]
    pub load_session: Option<String>,
    
    /// Start with a new chat session
    #[arg(long)]
    pub new_session: bool,
}
