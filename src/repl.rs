use anyhow::Result;
use std::collections::HashMap;
use futures::StreamExt;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::{
    commands::{Command, CommandParser},
    config::Config,
    history::History,
    providers::{create_provider, get_provider_for_model, LLMProvider, Message, ChatRequest},
    session::{ChatSession, CodeBlock},
    ui::{UI, CompletionContext},
};

#[derive(Debug, Clone)]
struct CodeBlockReference {
    global_number: usize,
    message_number: usize,
    code_block: CodeBlock,
}

pub struct Repl {
    config: Config,
    providers: HashMap<String, Box<dyn LLMProvider>>,
    session: ChatSession,
    history: History,
    command_parser: CommandParser,
    ui: UI,
    queued_message: Option<String>,
    active_request_token: Option<CancellationToken>,
    interrupted_message: Option<String>,
    ctrl_c_rx: mpsc::UnboundedReceiver<()>,
}

impl Repl {
    pub async fn new(config: Config, ctrl_c_rx: mpsc::UnboundedReceiver<()>) -> Result<Self> {
        let mut providers = HashMap::new();
        
        // Initialize providers based on available API keys
        for (provider_name, api_key) in &config.api_keys {
            match create_provider(provider_name, api_key) {
                Ok(provider) => {
                    providers.insert(provider_name.clone(), provider);
                }
                Err(e) => {
                    eprintln!("Failed to initialize {} provider: {}", provider_name, e);
                }
            }
        }
        
        if providers.is_empty() {
            return Err(anyhow::anyhow!("No valid API keys provided. Please set at least one API key."));
        }
        
        let history = History::load()?;
        
        // Always start with a fresh session unless explicitly loading one
        let mut session = if let Some(session_name) = &config.load_session {
            history.load_session(session_name)
                .cloned()
                .unwrap_or_else(|| ChatSession::new(config.default_model.clone(), config.temperature, config.max_tokens, config.thinking_budget))
        } else {
            // Always start fresh - no more automatic restoration of current session
            ChatSession::new(config.default_model.clone(), config.temperature, config.max_tokens, config.thinking_budget)
        };
        
        // Ensure we have a valid model and that its provider is available
        if let Some(required_provider) = get_provider_for_model(&session.current_model) {
            if !providers.contains_key(required_provider) {
                // Current model's provider is not available, find a default model
                session.current_model = Self::find_default_model(&providers);
            }
        } else {
            // Unknown model, find a default
            session.current_model = Self::find_default_model(&providers);
        }
        
        // Update session provider based on current model
        session.current_provider = get_provider_for_model(&session.current_model).map(|s| s.to_string());
        
        // For fresh sessions, use config values
        // For loaded sessions, keep their stored values
        if config.load_session.is_none() {
            session.temperature = config.temperature;
            session.max_tokens = config.max_tokens;
            session.thinking_budget = config.thinking_budget;
        }
        
        let command_parser = CommandParser::new()?;
        let mut ui = UI::new()?;
        
        // Set up initial completion context
        let completion_context = Self::build_completion_context(&providers, &history);
        ui.update_completion_context(completion_context)?;
        
        Ok(Self {
            config,
            providers,
            session,
            history,
            command_parser,
            ui,
            queued_message: None,
            active_request_token: None,
            interrupted_message: None,
            ctrl_c_rx,
        })
    }
    
    fn find_default_model(providers: &HashMap<String, Box<dyn LLMProvider>>) -> String {
        // Prefer Anthropic, then OpenAI, then Gemini
        if providers.contains_key("anthropic") {
            "claude-sonnet-4-20250514".to_string()
        } else if providers.contains_key("openai") {
            "o3-pro".to_string()
        } else if providers.contains_key("gemini") {
            "gemini-2.5-pro".to_string()
        } else {
            "claude-sonnet-4-20250514".to_string() // Fallback
        }
    }
    
    fn get_current_provider(&self) -> Option<&str> {
        get_provider_for_model(&self.session.current_model)
    }
    
    fn build_completion_context(providers: &HashMap<String, Box<dyn LLMProvider>>, history: &History) -> CompletionContext {
        let mut available_models = Vec::new();
        
        // Collect all models from all providers
        for (_provider_name, provider) in providers {
            available_models.extend(provider.get_models());
        }
        
        // Sort models for better completion experience
        available_models.sort();
        
        // Get session names
        let session_names = history.list_sessions().into_iter().cloned().collect();
        
        CompletionContext {
            available_models,
            session_names,
        }
    }
    
    fn update_completion_context(&mut self) -> Result<()> {
        let context = Self::build_completion_context(&self.providers, &self.history);
        self.ui.update_completion_context(context)
    }
    
    fn get_all_code_blocks(&self) -> Vec<CodeBlockReference> {
        let mut all_blocks = Vec::new();
        let mut global_block_number = 1;
        
        for numbered_message in &self.session.messages {
            for code_block in &numbered_message.code_blocks {
                all_blocks.push(CodeBlockReference {
                    global_number: global_block_number,
                    message_number: numbered_message.number,
                    code_block: code_block.clone(),
                });
                global_block_number += 1;
            }
        }
        
        all_blocks
    }
    
    fn copy_to_system_clipboard(&self, content: &str) -> Result<()> {
        use arboard::Clipboard;
        
        let mut clipboard = Clipboard::new()
            .map_err(|e| anyhow::anyhow!("Failed to access system clipboard: {}", e))?;
        
        clipboard.set_text(content)
            .map_err(|e| anyhow::anyhow!("Failed to set clipboard content: {}", e))?;
        
        Ok(())
    }
    
    fn copy_via_osc52(&self, content: &str) -> bool {
        use base64::Engine;
        use std::io::Write;
        
        // Encode content as base64
        let encoded = base64::engine::general_purpose::STANDARD.encode(content.as_bytes());
        
        // Emit OSC52 escape sequence: \033]52;c;<base64>\033\\
        // 'c' means clipboard (as opposed to 'p' for primary selection)
        print!("\x1b]52;c;{}\x1b\\", encoded);
        std::io::stdout().flush().unwrap_or(());
        
        true // OSC52 emission always "succeeds" (we can't know if terminal supports it)
    }
    
    fn execute_code_block(&mut self, block_ref: &CodeBlockReference) -> Result<()> {
        let language = block_ref.code_block.language.as_deref().unwrap_or("unknown");
        
        match language {
            "bash" | "sh" => {
                self.ui.print_info("Executing bash script...");
                let output = std::process::Command::new("bash")
                    .arg("-c")
                    .arg(&block_ref.code_block.content)
                    .output()?;
                
                if !output.stdout.is_empty() {
                    println!("Output:\n{}", String::from_utf8_lossy(&output.stdout));
                }
                if !output.stderr.is_empty() {
                    println!("Error:\n{}", String::from_utf8_lossy(&output.stderr));
                }
                self.ui.print_info(&format!("Process exited with code: {}", output.status.code().unwrap_or(-1)));
            }
            "python" | "py" => {
                self.ui.print_info("Executing Python script...");
                let output = std::process::Command::new("python3")
                    .arg("-c")
                    .arg(&block_ref.code_block.content)
                    .output()?;
                
                if !output.stdout.is_empty() {
                    println!("Output:\n{}", String::from_utf8_lossy(&output.stdout));
                }
                if !output.stderr.is_empty() {
                    println!("Error:\n{}", String::from_utf8_lossy(&output.stderr));
                }
                self.ui.print_info(&format!("Process exited with code: {}", output.status.code().unwrap_or(-1)));
            }
            "rust" => {
                self.ui.print_info("Rust code execution not yet supported (requires compilation)");
            }
            "javascript" | "js" => {
                self.ui.print_info("Executing JavaScript with Node.js...");
                let output = std::process::Command::new("node")
                    .arg("-e")
                    .arg(&block_ref.code_block.content)
                    .output()?;
                
                if !output.stdout.is_empty() {
                    println!("Output:\n{}", String::from_utf8_lossy(&output.stdout));
                }
                if !output.stderr.is_empty() {
                    println!("Error:\n{}", String::from_utf8_lossy(&output.stderr));
                }
                self.ui.print_info(&format!("Process exited with code: {}", output.status.code().unwrap_or(-1)));
            }
            _ => {
                self.ui.print_error(&format!("Execution not supported for language: {}", language));
                self.ui.print_info("Supported languages: bash, python, javascript");
            }
        }
        
        Ok(())
    }
    
    pub async fn run(&mut self) -> Result<()> {
        self.ui.draw_welcome()?;
        
        // Display current session status and recent sessions at startup
        self.display_startup_status();
        self.display_recent_sessions();
        
        loop {
            // Determine what message to show in prompt
            let prompt_message = if let Some(interrupted) = &self.interrupted_message {
                Some((interrupted.as_str(), "interrupted"))
            } else if let Some(queued) = &self.queued_message {
                Some((queued.as_str(), "retry"))
            } else {
                None
            };
            
            // Determine session name for prompt
            let session_name = if self.session.messages.is_empty() {
                None // Don't show session name for empty sessions
            } else {
                self.session.name.as_deref()
            };
            
            if let Some(input) = self.ui.read_input(prompt_message, session_name)? {
                // Handle Ctrl-C signal
                if input == "__CTRL_C__" {
                    // Cancel any active request
                    if let Some(token) = &self.active_request_token {
                        token.cancel();
                        self.ui.print_info("Request cancelled");
                    }
                    // Clear all queued/interrupted messages
                    self.queued_message = None;
                    self.interrupted_message = None;
                    continue;
                }
                
                
                // Clear queued/interrupted messages once we get input
                self.queued_message = None;
                self.interrupted_message = None;
                if input.starts_with('/') {
                    // This is a command attempt
                    if let Some(command) = self.command_parser.parse(&input) {
                        match self.handle_command(command).await {
                            Ok(should_continue) => {
                                if !should_continue {
                                    break;
                                }
                            }
                            Err(e) => {
                                self.ui.print_error(&e.to_string());
                            }
                        }
                    } else {
                        // Invalid command
                        self.ui.print_error(&format!("Invalid command: '{}'. Type /help for available commands.", input));
                    }
                } else {
                    // Handle regular chat message
                    if let Err(e) = self.handle_message(input).await {
                        self.ui.print_error(&e.to_string());
                    }
                }
                
                // Auto-save session if it has LLM interactions
                if let Err(e) = self.history.auto_save_session(&self.session) {
                    self.ui.print_error(&format!("Failed to auto-save session: {}", e));
                } else {
                    // Update completion context after auto-save
                    let _ = self.update_completion_context();
                }
            }
        }
        
        Ok(())
    }
    
    fn display_startup_status(&self) {
        if let Some(provider_name) = self.get_current_provider() {
            println!("\x1b[1;36mCurrent Configuration:\x1b[0m");
            println!("  Provider: {}", provider_name);
            println!("  Model: {}", self.session.current_model);
            
            // Show temperature with capability check
            let temp_display = self.get_temperature_display();
            println!("  Temperature: {}", temp_display);
            
            if let Some(system_prompt) = &self.session.system_prompt {
                println!("  System prompt: {}", system_prompt);
            } else {
                println!("  System prompt: (none)");
            }
            
            // Show thinking with capability check
            let thinking_display = self.get_thinking_display();
            println!("  Thinking: {}", thinking_display);
            
            println!("  Max tokens: {}", self.session.max_tokens);
            println!("  Thinking budget: {}", self.session.thinking_budget);
            
            // Show session info if we have messages
            if !self.session.messages.is_empty() {
                println!("  Session: {} messages", self.session.messages.len());
                if let Some(name) = &self.session.name {
                    println!("  Session name: {}", name);
                }
            } else {
                println!("  Session: new");
            }
        } else {
            println!("\x1b[1;31mNo provider available\x1b[0m");
        }
        println!();
    }
    
    fn display_recent_sessions(&self) {
        let recent_sessions = self.history.get_recent_sessions(3);
        if !recent_sessions.is_empty() {
            println!("\x1b[1;36mRecent sessions:\x1b[0m");
            for (name, session) in recent_sessions {
                let message_count = session.messages.len();
                let updated = session.updated_at.format("%m-%d %H:%M");
                println!("  /chat load {} - {} messages ({})", name, message_count, updated);
            }
            if let Some(_most_recent) = self.history.get_most_recent_session() {
                println!("  /chat continue - Continue most recent session");
            }
            println!();
        }
    }
    
    fn get_temperature_display(&self) -> String {
        if let Some(provider_name) = self.get_current_provider() {
            if let Some(provider) = self.providers.get(provider_name) {
                let supports_temp = match provider_name {
                    "openai" => {
                        if let Some(openai_provider) = provider.as_any().downcast_ref::<crate::providers::openai::OpenAIProvider>() {
                            openai_provider.supports_temperature(&self.session.current_model)
                        } else { true }
                    }
                    "anthropic" => {
                        // For Anthropic, temperature is N/A when thinking is enabled
                        if self.session.thinking_enabled {
                            if let Some(anthropic_provider) = provider.as_any().downcast_ref::<crate::providers::anthropic::AnthropicProvider>() {
                                !anthropic_provider.supports_thinking(&self.session.current_model)
                            } else { true }
                        } else {
                            true // All Anthropic models support temperature when thinking is disabled
                        }
                    }
                    "gemini" => true,    // All Gemini models support temperature
                    _ => true
                };
                
                if supports_temp {
                    self.session.temperature.to_string()
                } else {
                    "N/A".to_string()
                }
            } else {
                self.session.temperature.to_string()
            }
        } else {
            self.session.temperature.to_string()
        }
    }
    
    fn get_thinking_display(&self) -> String {
        if let Some(provider_name) = self.get_current_provider() {
            if let Some(provider) = self.providers.get(provider_name) {
                let supports_thinking = match provider_name {
                    "anthropic" => {
                        if let Some(anthropic_provider) = provider.as_any().downcast_ref::<crate::providers::anthropic::AnthropicProvider>() {
                            anthropic_provider.supports_thinking(&self.session.current_model)
                        } else { false }
                    }
                    "openai" => false,  // OpenAI models don't support thinking
                    "gemini" => false,  // Gemini models don't support thinking
                    _ => false
                };
                
                if supports_thinking {
                    if self.session.thinking_enabled { "enabled".to_string() } else { "disabled".to_string() }
                } else {
                    "N/A".to_string()
                }
            } else {
                if self.session.thinking_enabled { "enabled".to_string() } else { "disabled".to_string() }
            }
        } else {
            if self.session.thinking_enabled { "enabled".to_string() } else { "disabled".to_string() }
        }
    }
    
    fn get_session_display(&self) -> String {
        let message_count = self.session.messages.len();
        
        if message_count == 0 {
            "new (0 messages)".to_string()
        } else if let Some(name) = &self.session.name {
            format!("\"{}\" ({} messages)", name, message_count)
        } else {
            format!("untitled ({} messages)", message_count)
        }
    }
    
    fn get_next_agent_number(&self) -> usize {
        self.session.messages.iter()
            .filter(|msg| msg.message.role == "assistant")
            .count() + 1
    }
    
    fn get_message_number_for_agent(&self, agent_number: usize) -> Option<usize> {
        let mut agent_count = 0;
        for (i, msg) in self.session.messages.iter().enumerate() {
            if msg.message.role == "assistant" {
                agent_count += 1;
                if agent_count == agent_number {
                    return Some(i + 1);
                }
            }
        }
        None
    }
    
    async fn handle_command(&mut self, command: Command) -> Result<bool> {
        match command {
            Command::Quit => return Ok(false),
            Command::Help => {
                self.ui.print_info("Available commands:");
                println!("  /model MODEL - Switch to a different model (auto-detects provider)");
                println!("  /models - List available models across all providers");
                println!("  /status - Show current provider and model");
                println!("  /chat new - Start a new chat session");
                println!("  /chat save NAME - Save current session with given name");
                println!("  /chat load NAME - Load a previously saved session");
                println!("  /chat list - List all saved sessions");
                println!("  /chat delete [NAME] - Delete a saved session (defaults to current)");
                println!("  /chat continue - Continue the most recent session");
                println!("  /chat recent - Show recent sessions");
                println!("  /chat fork NAME - Save current session and start fresh");
                println!("  /chat merge NAME - Merge another session into current");
                println!("  /chat rename NEW_NAME [OLD_NAME] - Rename a session (defaults to current)");
                println!("  /chat auto-rename [NAME] - Auto-generate title for session (defaults to current)");
                println!("  /summarize [NAME] - Generate summary of session (defaults to current)");
                println!("  /undo [N] - Undo last N agent responses (default 1), restores user message for editing");
                println!("  /goto N - Jump back to Agent N (removes later messages)");
                println!("  /history - Show conversation history");
                println!("  /blocks - List all code blocks in session");
                println!("  /block N - Display code block N");
                println!("  /copy N - Copy code block N to clipboard");
                println!("  /save N FILE - Save code block N to file");
                println!("  /exec N - Execute code block N (with confirmation)");
                println!("  /system [PROMPT] - Set system prompt (empty to view, 'clear' to remove)");
                println!("  /temp TEMPERATURE - Set temperature (0.0-2.0)");
                println!("  /max-tokens TOKENS - Set maximum output tokens");
                println!("  /thinking-budget TOKENS - Set thinking token budget");
                println!("  /thinking on|off - Enable/disable thinking for supported models");
                println!("  /quit - Exit Njord");
                println!();
                println!("Input tips:");
                println!("  Start with ``` for multi-line input (end with ``` on its own line)");
                println!("  Use this for code, long prompts, or formatted text");
            }
            Command::Models => {
                self.ui.print_info("Available models:");
                
                // Group models by provider
                let mut all_models = Vec::new();
                for (provider_name, provider) in &self.providers {
                    for model in provider.get_models() {
                        all_models.push((provider_name.clone(), model));
                    }
                }
                
                // Sort by provider name for consistent display
                all_models.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
                
                let mut current_provider = String::new();
                for (provider_name, model) in all_models {
                    if provider_name != current_provider {
                        if !current_provider.is_empty() {
                            println!();
                        }
                        println!("  \x1b[1;36m{}:\x1b[0m", provider_name);
                        current_provider = provider_name;
                    }
                    println!("    {}", model);
                }
            }
            Command::ChatNew => {
                // Auto-save current session if it has interactions
                if let Err(e) = self.history.auto_save_session(&self.session) {
                    self.ui.print_error(&format!("Failed to auto-save current session: {}", e));
                } else {
                    // Update completion context after auto-save
                    let _ = self.update_completion_context();
                }
                
                self.session = ChatSession::new(self.config.default_model.clone(), self.config.temperature, self.config.max_tokens, self.config.thinking_budget);
                self.session.current_provider = get_provider_for_model(&self.session.current_model).map(|s| s.to_string());
                self.ui.print_info("Started new chat session");
            }
            Command::ChatContinue(session_name) => {
                let target_session = if let Some(ref name) = session_name {
                    // Continue specific named session
                    self.history.load_session(name).cloned()
                } else {
                    // Continue most recent session
                    self.history.get_most_recent_session().cloned()
                };
                
                if let Some(target_session) = target_session {
                    // Auto-save current session if it has interactions
                    if let Err(e) = self.history.auto_save_session(&self.session) {
                        self.ui.print_error(&format!("Failed to auto-save current session: {}", e));
                    } else {
                        // Update completion context after auto-save
                        let _ = self.update_completion_context();
                    }
                    
                    self.session = target_session;
                    // Update current provider based on session's model
                    self.session.current_provider = get_provider_for_model(&self.session.current_model).map(|s| s.to_string());
                    
                    // Enhanced feedback with full context
                    let auto_name = self.session.generate_auto_name();
                    let session_name = self.session.name.as_ref().unwrap_or(&auto_name);
                    let updated = self.session.updated_at.format("%Y-%m-%d %H:%M");
                    self.ui.print_info(&format!("Continuing session: \"{}\" ({} messages, last updated {})", 
                        session_name, self.session.messages.len(), updated));
                    
                    if let Some(provider_name) = &self.session.current_provider {
                        self.ui.print_info(&format!("Session model: {} ({})", self.session.current_model, provider_name));
                    } else {
                        self.ui.print_info(&format!("Session model: {}", self.session.current_model));
                    }
                } else {
                    if let Some(name) = session_name {
                        self.ui.print_error(&format!("Session '{}' not found", name));
                    } else {
                        self.ui.print_error("No recent sessions found");
                    }
                }
            }
            Command::ChatRecent => {
                let recent_sessions = self.history.get_recent_sessions(10);
                if recent_sessions.is_empty() {
                    self.ui.print_info("No recent sessions found");
                } else {
                    self.ui.print_info("Recent sessions:");
                    for (name, session) in recent_sessions {
                        let message_count = session.messages.len();
                        let updated = session.updated_at.format("%Y-%m-%d %H:%M");
                        println!("  {} - {} messages (updated {})", name, message_count, updated);
                    }
                }
            }
            Command::ChatFork(name) => {
                if name.trim().is_empty() {
                    self.ui.print_error("Session name cannot be empty");
                } else if self.session.messages.is_empty() {
                    self.ui.print_error("Cannot fork empty session");
                } else {
                    match self.history.save_session(name.clone(), self.session.clone()) {
                        Ok(()) => {
                            self.ui.print_info(&format!("Session forked as '{}' ({} messages)", name, self.session.messages.len()));
                            // Start fresh session
                            self.session = ChatSession::new(self.config.default_model.clone(), self.config.temperature, self.config.max_tokens, self.config.thinking_budget);
                            self.session.current_provider = get_provider_for_model(&self.session.current_model).map(|s| s.to_string());
                            self.ui.print_info("Started new session");
                            // Update completion context with new session
                            let _ = self.update_completion_context();
                        }
                        Err(e) => {
                            self.ui.print_error(&format!("Failed to fork session: {}", e));
                        }
                    }
                }
            }
            Command::ChatMerge(name) => {
                if let Some(other_session) = self.history.load_session(&name).cloned() {
                    let other_message_count = other_session.messages.len();
                    match self.session.merge_session(&other_session) {
                        Ok(()) => {
                            self.ui.print_info(&format!("Merged {} messages from '{}' into current session", 
                                other_message_count, name));
                            self.ui.print_info(&format!("Current session now has {} messages", self.session.messages.len()));
                        }
                        Err(e) => {
                            self.ui.print_error(&format!("Failed to merge session: {}", e));
                        }
                    }
                } else {
                    self.ui.print_error(&format!("Session '{}' not found", name));
                    let available_sessions = self.history.list_sessions();
                    if !available_sessions.is_empty() {
                        self.ui.print_info("Available sessions:");
                        for session_name in available_sessions.iter().take(5) {
                            println!("  {}", session_name);
                        }
                    }
                }
            }
            Command::ChatRename(new_name, old_name) => {
                if new_name.trim().is_empty() {
                    self.ui.print_error("New session name cannot be empty");
                } else {
                    let target_name = if let Some(ref old_name) = old_name {
                        // Rename specific session
                        old_name.clone()
                    } else {
                        // Rename current session - it must be saved first
                        if let Some(ref current_name) = self.session.name {
                            current_name.clone()
                        } else {
                            self.ui.print_error("Current session has no name. Save it first with /chat save NAME");
                            return Ok(true);
                        }
                    };
                
                    match self.history.rename_session(&target_name, &new_name) {
                        Ok(true) => {
                            self.ui.print_info(&format!("Session \"{}\" renamed to \"{}\"", target_name, new_name));
                        
                            // If we renamed the current session, update its name and show context
                            if old_name.is_none() || self.session.name.as_ref() == Some(&target_name) {
                                self.session.name = Some(new_name.clone());
                                self.ui.print_info(&format!("Current session: \"{}\" ({} messages)", 
                                    new_name, self.session.messages.len()));
                            }
                        
                            // Update completion context with new session name
                            let _ = self.update_completion_context();
                        }
                        Ok(false) => {
                            self.ui.print_error(&format!("Session '{}' not found", target_name));
                            let available_sessions = self.history.list_sessions();
                            if !available_sessions.is_empty() {
                                self.ui.print_info("Available sessions:");
                                for session_name in available_sessions.iter().take(5) {
                                    println!("  {}", session_name);
                                }
                            }
                        }
                        Err(e) => {
                            self.ui.print_error(&format!("Failed to rename session: {}", e));
                        }
                    }
                }
            }
            Command::ChatAutoRename(session_name) => {
                match self.handle_auto_rename(session_name).await {
                    Ok(()) => {
                        // Success message already printed in handle_auto_rename
                    }
                    Err(e) => {
                        self.ui.print_error(&format!("Failed to auto-rename session: {}", e));
                    }
                }
            }
            Command::Summarize(session_name) => {
                match self.handle_summarize(session_name).await {
                    Ok(()) => {
                        // Success - summary already printed
                    }
                    Err(e) => {
                        self.ui.print_error(&format!("Failed to summarize session: {}", e));
                    }
                }
            }
            Command::Undo(count) => {
                let count = count.unwrap_or(1);
                match self.session.undo(count) {
                    Ok(last_user_message) => {
                        self.ui.print_info(&format!("Undid last {} agent response(s)", count));
                        
                        // If we have a user message to restore, queue it for editing
                        if let Some(user_msg) = last_user_message {
                            self.queued_message = Some(user_msg);
                            self.ui.print_info("Last user message available for editing - press Enter to modify and resend");
                        }
                    }
                    Err(e) => {
                        self.ui.print_error(&e.to_string());
                    }
                }
            }
            Command::Model(model_name) => {
                // Determine which provider this model belongs to
                if let Some(required_provider) = get_provider_for_model(&model_name) {
                    // Check if we have this provider available
                    if let Some(provider) = self.providers.get(required_provider) {
                        let available_models = provider.get_models();
                        if available_models.contains(&model_name) {
                            let old_provider = self.get_current_provider().map(|s| s.to_string());
                            self.session.current_model = model_name.clone();
                            self.session.current_provider = Some(required_provider.to_string());
                            
                            if old_provider.as_deref() != Some(required_provider) {
                                self.ui.print_info(&format!("Switched to model: {} (provider: {})", model_name, required_provider));
                            } else {
                                self.ui.print_info(&format!("Switched to model: {}", model_name));
                            }
                        } else {
                            self.ui.print_error(&format!("Model '{}' not available. Available {} models: {}", 
                                model_name, required_provider, available_models.join(", ")));
                        }
                    } else {
                        self.ui.print_error(&format!("Provider '{}' not available (required for model '{}'). Check your API key.", 
                            required_provider, model_name));
                    }
                } else {
                    self.ui.print_error(&format!("Unknown model: '{}'. Use /models to see available models.", model_name));
                }
            }
            Command::Status => {
                if let Some(provider_name) = self.get_current_provider() {
                    self.ui.print_info(&format!("Current provider: {}", provider_name));
                    self.ui.print_info(&format!("Current model: {}", self.session.current_model));
                    
                    let temp_display = self.get_temperature_display();
                    self.ui.print_info(&format!("Temperature: {}", temp_display));
                    
                    if let Some(system_prompt) = &self.session.system_prompt {
                        self.ui.print_info(&format!("System prompt: {}", system_prompt));
                    } else {
                        self.ui.print_info("System prompt: (none)");
                    }
                    
                    let thinking_display = self.get_thinking_display();
                    self.ui.print_info(&format!("Thinking: {}", thinking_display));
                    
                    self.ui.print_info(&format!("Max tokens: {}", self.session.max_tokens));
                    self.ui.print_info(&format!("Thinking budget: {}", self.session.thinking_budget));
                    
                    // Show session information
                    let session_info = self.get_session_display();
                    self.ui.print_info(&format!("Session: {}", session_info));
                } else {
                    self.ui.print_error("No provider selected");
                }
            }
            Command::ChatSave(name) => {
                if name.trim().is_empty() {
                    self.ui.print_error("Session name cannot be empty");
                } else if self.session.messages.is_empty() {
                    self.ui.print_error("Cannot save empty session");
                } else {
                    match self.history.save_session(name.clone(), self.session.clone()) {
                        Ok(()) => {
                            self.ui.print_info(&format!("Session saved as '{}'", name));
                            // Update completion context with new session
                            let _ = self.update_completion_context();
                        }
                        Err(e) => {
                            self.ui.print_error(&format!("Failed to save session: {}", e));
                        }
                    }
                }
            }
            Command::ChatLoad(name) => {
                // First, clone the session if it exists
                let session_to_load = self.history.load_session(&name).cloned();
                
                if let Some(session) = session_to_load {
                    // Auto-save current session if it has interactions
                    if let Err(e) = self.history.auto_save_session(&self.session) {
                        self.ui.print_error(&format!("Failed to auto-save current session: {}", e));
                    } else {
                        // Update completion context after auto-save
                        let _ = self.update_completion_context();
                    }
                    
                    // Create a copy of the session (new ID, no name, fresh timestamps)
                    self.session = session.create_copy();
                    
                    // Update current provider based on session's model
                    self.session.current_provider = get_provider_for_model(&self.session.current_model).map(|s| s.to_string());
                    
                    // Enhanced feedback with full context
                    let created = self.session.created_at.format("%Y-%m-%d %H:%M");
                    self.ui.print_info(&format!("Loaded copy of session \"{}\" ({} messages, created {})", 
                        name, self.session.messages.len(), created));
                    
                    if let Some(session_provider) = &self.session.current_provider {
                        if self.providers.contains_key(session_provider) {
                            self.ui.print_info(&format!("Session model: {} ({})", self.session.current_model, session_provider));
                        } else {
                            self.ui.print_info(&format!("Session model: {} (provider '{}' not available)", self.session.current_model, session_provider));
                        }
                    } else {
                        self.ui.print_info(&format!("Session model: {}", self.session.current_model));
                    }
                    
                    self.ui.print_info(&format!("Original session \"{}\" unchanged", name));
                } else {
                    self.ui.print_error(&format!("Session '{}' not found", name));
                    let available_sessions = self.history.list_sessions();
                    if !available_sessions.is_empty() {
                        self.ui.print_info("Available sessions:");
                        for session_name in available_sessions {
                            println!("  {}", session_name);
                        }
                    }
                }
            }
            Command::ChatList => {
                let sessions = self.history.list_sessions();
                if sessions.is_empty() {
                    self.ui.print_info("No saved sessions");
                } else {
                    self.ui.print_info("Saved sessions:");
                    for session_name in sessions {
                        if let Some(session) = self.history.load_session(session_name) {
                            let message_count = session.messages.len();
                            let created = session.created_at.format("%Y-%m-%d %H:%M");
                            println!("  {} ({} messages, created {})", session_name, message_count, created);
                        }
                    }
                }
            }
            Command::ChatDelete(name_opt) => {
                let target_name = if let Some(name) = name_opt {
                    // Delete specific named session
                    name
                } else {
                    // Delete current session - it must be saved first
                    if let Some(ref current_name) = self.session.name {
                        current_name.clone()
                    } else {
                        self.ui.print_error("Current session has no name. Save it first with /chat save NAME, or specify a session name to delete");
                        return Ok(true);
                    }
                };
                
                match self.history.delete_session(&target_name) {
                    Ok(true) => {
                        self.ui.print_info(&format!("Session '{}' deleted", target_name));
                        
                        // Check if we deleted the current session
                        if self.session.name.as_ref() == Some(&target_name) {
                            // Reset to a new anonymous session
                            self.session = ChatSession::new(
                                self.config.default_model.clone(), 
                                self.config.temperature, 
                                self.config.max_tokens, 
                                self.config.thinking_budget
                            );
                            self.session.current_provider = get_provider_for_model(&self.session.current_model).map(|s| s.to_string());
                            self.ui.print_info("Current session was deleted - started new anonymous session");
                        }
                        
                        // Update completion context after deletion
                        let _ = self.update_completion_context();
                    }
                    Ok(false) => {
                        self.ui.print_error(&format!("Session '{}' not found", target_name));
                    }
                    Err(e) => {
                        self.ui.print_error(&format!("Failed to delete session: {}", e));
                    }
                }
            }
            Command::Search(term) => {
                if term.trim().is_empty() {
                    self.ui.print_error("Search term cannot be empty");
                } else {
                    let results = self.history.search_all_sessions(&term, &self.session);
                    
                    if results.is_empty() {
                        self.ui.print_info(&format!("No results found for '{}'", term));
                    } else {
                        self.ui.print_info(&format!("Search results for '{}' ({} matches):", term, results.len()));
                        println!();
                        
                        let mut current_session = String::new();
                        for result in results {
                            // Print session header if this is a new session
                            if result.session_name != current_session {
                                if !current_session.is_empty() {
                                    println!(); // Add spacing between sessions
                                }
                                println!("\x1b[1;36m[{}]\x1b[0m", result.session_name);
                                current_session = result.session_name.clone();
                            }
                            
                            // Print the search result
                            let role_color = if result.role == "user" {
                                "\x1b[1;34m" // Blue for user
                            } else {
                                "\x1b[1;35m" // Magenta for assistant
                            };
                            
                            println!("  {}Message {} ({})\x1b[0m: {}", 
                                role_color,
                                result.message_number,
                                result.role.chars().next().unwrap().to_uppercase().collect::<String>() + &result.role[1..],
                                result.excerpt
                            );
                        }
                        
                        println!();
                        self.ui.print_info("Use /goto N to jump to a message, or /chat load SESSION to switch sessions");
                    }
                }
            }
            Command::History => {
                if self.session.messages.is_empty() {
                    self.ui.print_info("No messages in current session");
                } else {
                    self.ui.print_info(&format!("Session history ({} messages):", self.session.messages.len()));
                    if let Some(name) = &self.session.name {
                        println!("Session name: {}", name);
                    }
                    println!("Created: {}", self.session.created_at.format("%Y-%m-%d %H:%M:%S UTC"));
                    println!("Model: {}", self.session.current_model);
                    println!("Temperature: {}", self.session.temperature);
                    if let Some(system_prompt) = &self.session.system_prompt {
                        println!("System prompt: {}", system_prompt);
                    }
                    println!();
                    
                    let mut conversation_index = 0;
                    let mut i = 0;
                    
                    while i < self.session.messages.len() {
                        let current_msg = &self.session.messages[i];
                        
                        if current_msg.message.role == "user" {
                            conversation_index += 1;
                            
                            // Print user message
                            let timestamp = current_msg.timestamp.format("%H:%M:%S");
                            let header = format!("\x1b[1;34m[{}] User {}", conversation_index, timestamp);
                            let styled_content = self.ui.style_code_blocks(&current_msg.message.content);
                            println!("{}\x1b[0m: {}", header, styled_content);
                            println!();
                            
                            // Look for the corresponding agent message
                            if i + 1 < self.session.messages.len() {
                                let next_msg = &self.session.messages[i + 1];
                                if next_msg.message.role == "assistant" {
                                    let agent_timestamp = next_msg.timestamp.format("%H:%M:%S");
                                    let mut agent_header = format!("\x1b[1;35m[{}] Agent {}", conversation_index, agent_timestamp);
                                    
                                    // Add provider/model info for assistant messages
                                    if let (Some(provider), Some(model)) = (&next_msg.provider, &next_msg.model) {
                                        agent_header.push_str(&format!(" ({}:{})", provider, model));
                                    } else if let Some(provider) = &next_msg.provider {
                                        agent_header.push_str(&format!(" ({})", provider));
                                    }
                                    
                                    let agent_styled_content = self.ui.style_code_blocks(&next_msg.message.content);
                                    println!("{}\x1b[0m: {}", agent_header, agent_styled_content);
                                    println!();
                                    
                                    i += 2; // Skip both user and agent message
                                } else {
                                    i += 1; // Only skip user message
                                }
                            } else {
                                i += 1; // Only skip user message
                            }
                        } else {
                            // Orphaned agent message (shouldn't happen in normal flow)
                            let timestamp = current_msg.timestamp.format("%H:%M:%S");
                            let mut header = format!("\x1b[1;35m[orphaned] Agent {}", timestamp);
                            
                            if let (Some(provider), Some(model)) = (&current_msg.provider, &current_msg.model) {
                                header.push_str(&format!(" ({}:{})", provider, model));
                            } else if let Some(provider) = &current_msg.provider {
                                header.push_str(&format!(" ({})", provider));
                            }
                            
                            let styled_content = self.ui.style_code_blocks(&current_msg.message.content);
                            println!("{}\x1b[0m: {}", header, styled_content);
                            println!();
                            
                            i += 1;
                        }
                    }
                }
            }
            Command::Goto(agent_number) => {
                // Convert agent number to message number
                if let Some(message_number) = self.get_message_number_for_agent(agent_number) {
                    let removed_count = self.session.messages.len().saturating_sub(message_number);
                    
                    // Before jumping, check if there's a user message after this agent message
                    let user_message_to_stage = if message_number < self.session.messages.len() {
                        // Look for the next user message after this agent message
                        self.session.messages.iter()
                            .skip(message_number)
                            .find(|msg| msg.message.role == "user")
                            .map(|msg| msg.message.content.clone())
                    } else {
                        None
                    };
                    
                    match self.session.goto(message_number) {
                        Ok(()) => {
                            self.ui.print_info(&format!("Jumped to Agent {}, removed {} later messages", 
                                agent_number, removed_count));
                            
                            // Stage the user message if we found one
                            if let Some(user_msg) = user_message_to_stage {
                                self.queued_message = Some(user_msg);
                                self.ui.print_info("User message staged for editing - press Enter to modify and resend");
                            }
                        }
                        Err(e) => {
                            self.ui.print_error(&e.to_string());
                        }
                    }
                } else {
                    self.ui.print_error(&format!("Agent {} not found", agent_number));
                }
            }
            Command::System(prompt) => {
                if prompt.trim().is_empty() {
                    // Show current system prompt
                    if let Some(current_prompt) = &self.session.system_prompt {
                        self.ui.print_info("Current system prompt:");
                        println!("{}", current_prompt);
                    } else {
                        self.ui.print_info("No system prompt is currently set");
                    }
                } else if prompt.trim() == "clear" {
                    // Clear system prompt
                    self.session.system_prompt = None;
                    self.ui.print_info("System prompt cleared");
                } else {
                    // Set new system prompt
                    self.session.system_prompt = Some(prompt);
                    self.ui.print_info("System prompt updated");
                }
            }
            Command::Temperature(temp) => {
                if temp < 0.0 || temp > 2.0 {
                    self.ui.print_error("Temperature must be between 0.0 and 2.0");
                } else {
                    self.session.temperature = temp;
                    self.ui.print_info(&format!("Temperature set to {}", temp));
                }
            }
            Command::MaxTokens(tokens) => {
                if tokens == 0 {
                    self.ui.print_error("Max tokens must be greater than 0");
                } else {
                    self.session.max_tokens = tokens;
                    self.ui.print_info(&format!("Max tokens set to {}", tokens));
                }
            }
            Command::ThinkingBudget(budget) => {
                if budget == 0 {
                    self.ui.print_error("Thinking budget must be greater than 0");
                } else {
                    self.session.thinking_budget = budget;
                    self.ui.print_info(&format!("Thinking budget set to {}", budget));
                }
            }
            Command::Thinking(enable) => {
                self.session.thinking_enabled = enable;
                self.ui.print_info(&format!("Thinking {}", if enable { "enabled" } else { "disabled" }));
            }
            Command::Blocks => {
                let all_blocks = self.get_all_code_blocks();
                if all_blocks.is_empty() {
                    self.ui.print_info("No code blocks found in current session");
                } else {
                    self.ui.print_info(&format!("Code blocks in current session ({} total):", all_blocks.len()));
                    for block_ref in &all_blocks {
                        let language_display = block_ref.code_block.language.as_deref().unwrap_or("text");
                        let preview = if block_ref.code_block.content.len() > 50 {
                            format!("{}...", &block_ref.code_block.content[..50].replace('\n', " "))
                        } else {
                            block_ref.code_block.content.replace('\n', " ")
                        };
                        println!("  [{}] Message {} ({}): {}", 
                            block_ref.global_number, 
                            block_ref.message_number, 
                            language_display, 
                            preview
                        );
                    }
                    println!();
                    self.ui.print_info("Use /block N to view, /copy N to copy, /save N FILE to save, /exec N to execute");
                }
            }
            Command::Block(block_number) => {
                let all_blocks = self.get_all_code_blocks();
                if let Some(block) = all_blocks.get(block_number.saturating_sub(1)) {
                    self.ui.print_info(&format!("Code block {} from message {}:", block_number, block.message_number));
                    println!();
                    self.ui.print_styled_code_block(&block.code_block.content, block.code_block.language.as_deref());
                } else {
                    self.ui.print_error(&format!("Code block {} not found. Use /blocks to list all code blocks.", block_number));
                }
            }
            Command::Copy(block_number) => {
                let all_blocks = self.get_all_code_blocks();
                if let Some(block) = all_blocks.get(block_number.saturating_sub(1)) {
                    let content = &block.code_block.content;
                    let mut success_methods = Vec::new();
                    
                    // Try system clipboard first
                    match self.copy_to_system_clipboard(content) {
                        Ok(()) => success_methods.push("system clipboard"),
                        Err(e) => {
                            // Don't show error yet, we'll try OSC52
                            eprintln!("System clipboard failed: {}", e);
                        }
                    }
                    
                    // Always emit OSC52 for terminal/SSH compatibility
                    if self.copy_via_osc52(content) {
                        success_methods.push("OSC52 (terminal)");
                    }
                    
                    if !success_methods.is_empty() {
                        self.ui.print_info(&format!("Code block {} copied via: {}", 
                            block_number, success_methods.join(", ")));
                    } else {
                        self.ui.print_error("Failed to copy to clipboard");
                        self.ui.print_info("Content displayed below:");
                        println!();
                        self.ui.print_styled_code_block(content, block.code_block.language.as_deref());
                    }
                } else {
                    self.ui.print_error(&format!("Code block {} not found. Use /blocks to list all code blocks.", block_number));
                }
            }
            Command::Save(block_number, filename) => {
                let all_blocks = self.get_all_code_blocks();
                if let Some(block) = all_blocks.get(block_number.saturating_sub(1)) {
                    match std::fs::write(&filename, &block.code_block.content) {
                        Ok(()) => {
                            self.ui.print_info(&format!("Code block {} saved to '{}'", block_number, filename));
                        }
                        Err(e) => {
                            self.ui.print_error(&format!("Failed to save code block: {}", e));
                        }
                    }
                } else {
                    self.ui.print_error(&format!("Code block {} not found. Use /blocks to list all code blocks.", block_number));
                }
            }
            Command::Exec(block_number) => {
                let all_blocks = self.get_all_code_blocks();
                if let Some(block) = all_blocks.get(block_number.saturating_sub(1)) {
                    self.ui.print_info(&format!("Code block {} from message {}:", block_number, block.message_number));
                    println!();
                    self.ui.print_styled_code_block(&block.code_block.content, block.code_block.language.as_deref());
                    println!();
                    self.ui.print_info("⚠️  Execute this code? This will run the code on your system!");
                    self.ui.print_info("Type 'yes' to execute, anything else to cancel:");
                    
                    if let Some(confirmation) = self.ui.read_input(None, None)? {
                        if confirmation.trim().to_lowercase() == "yes" {
                            self.execute_code_block(block)?;
                        } else {
                            self.ui.print_info("Code execution cancelled");
                        }
                    }
                } else {
                    self.ui.print_error(&format!("Code block {} not found. Use /blocks to list all code blocks.", block_number));
                }
            }
            _ => {
                self.ui.print_info(&format!("Command not yet implemented: {:?}", command));
            }
        }
        
        Ok(true)
    }
    
    async fn handle_message(&mut self, message: String) -> Result<()> {
        // Create cancellation token for this request
        let cancel_token = CancellationToken::new();
        self.active_request_token = Some(cancel_token.clone());
        
        // Split the receiver to avoid borrow checker issues
        let mut ctrl_c_rx = std::mem::replace(&mut self.ctrl_c_rx, tokio::sync::mpsc::unbounded_channel().1);
        
        // Try to send the message with retry logic
        let result = tokio::select! {
            result = self.send_message_with_retry(&message, 3, cancel_token.clone()) => {
                // Restore the receiver
                self.ctrl_c_rx = ctrl_c_rx;
                result
            },
            _ = cancel_token.cancelled() => {
                // Restore the receiver
                self.ctrl_c_rx = ctrl_c_rx;
                // Request was cancelled - queue as interrupted message
                self.interrupted_message = Some(message);
                self.ui.print_info("Request interrupted. Message available for editing.");
                return Ok(());
            }
            _ = ctrl_c_rx.recv() => {
                // Ctrl-C received - cancel local token and queue message
                cancel_token.cancel();
                // Restore the receiver
                self.ctrl_c_rx = ctrl_c_rx;
                self.interrupted_message = Some(message);
                self.ui.print_info("Request interrupted by Ctrl-C. Message available for editing.");
                return Ok(());
            }
        };
        
        // Clear active request token
        self.active_request_token = None;
        
        match result {
            Ok(()) => {
                // Success - message was sent and response received
            }
            Err(e) => {
                // Check if this was a cancellation
                if cancel_token.is_cancelled() {
                    self.interrupted_message = Some(message);
                    self.ui.print_info("Request interrupted. Message available for editing.");
                    return Ok(());
                }
                
                // All retries failed - queue the message for retry
                self.queued_message = Some(message);
                self.ui.print_error(&format!("Failed to send message after 3 attempts: {}", e));
                self.ui.print_info("Message queued for retry. Press Enter to retry, or modify and press Enter.");
            }
        }
        
        Ok(())
    }
    
    async fn send_message_with_retry(&mut self, message: &str, max_retries: u32, cancel_token: CancellationToken) -> Result<()> {
        let user_message = Message {
            role: "user".to_string(),
            content: message.to_string(),
        };
        
        // Don't add user message to history until we have a successful response
        let agent_number = self.get_next_agent_number();
        
        for attempt in 1..=max_retries {
            if attempt > 1 {
                let delay = std::time::Duration::from_millis(1000 * (1 << (attempt - 2))); // Exponential backoff: 1s, 2s, 4s
                self.ui.print_info(&format!("Retrying in {}s... (attempt {}/{})", delay.as_secs(), attempt, max_retries));
                
                // Check for cancellation during delay
                tokio::select! {
                    _ = tokio::time::sleep(delay) => {},
                    _ = cancel_token.cancelled() => {
                        return Err(anyhow::anyhow!("Request cancelled during retry delay"));
                    }
                }
            }
            
            // Send to LLM provider and handle streaming response
            if let Some(provider_name) = self.get_current_provider() {
                if let Some(provider) = self.providers.get(provider_name) {
                    // Create request with user message included but not yet in session history
                    let mut request_messages: Vec<Message> = Vec::new();
                    
                    // Add system prompt if present
                    if let Some(system_prompt) = &self.session.system_prompt {
                        request_messages.push(Message {
                            role: "system".to_string(),
                            content: system_prompt.clone(),
                        });
                    }
                    
                    // Add conversation history
                    request_messages.extend(self.session.messages.iter().map(|nm| nm.message.clone()));
                    
                    // Add current user message
                    request_messages.push(user_message.clone());
                    
                    let chat_request = ChatRequest {
                        messages: request_messages,
                        model: self.session.current_model.clone(),
                        temperature: self.session.temperature,
                        max_tokens: self.session.max_tokens,
                        thinking_budget: self.session.thinking_budget,
                        stream: true,
                        thinking: self.session.thinking_enabled,
                    };
                    
                    match provider.chat(chat_request).await {
                        Ok(mut stream) => {
                            let mut full_response = String::new();
                            let mut stream_error = false;
                            let mut has_thinking = false;
                            let mut has_content = false;
                            let mut thinking_started = false;
                            
                            loop {
                                tokio::select! {
                                    chunk_result = stream.next() => {
                                        match chunk_result {
                                            Some(chunk) => {
                                                match chunk {
                                                    Ok(content) => {
                                                        if !content.is_empty() {
                                                            if content.starts_with("thinking:") {
                                                                let thinking_text = &content[9..]; // Remove "thinking:" prefix
                                                                if !thinking_started {
                                                                    self.ui.print_thinking_prefix(agent_number);
                                                                    thinking_started = true;
                                                                    has_thinking = true;
                                                                }
                                                                self.ui.print_thinking_chunk(thinking_text);
                                                            } else if content.starts_with("content:") {
                                                                let content_text = &content[8..]; // Remove "content:" prefix
                                                                if has_thinking && !has_content {
                                                                    self.ui.print_thinking_end();
                                                                    self.ui.print_agent_prefix(agent_number);
                                                                    has_content = true;
                                                                } else if !has_content {
                                                                    self.ui.print_agent_prefix(agent_number);
                                                                    has_content = true;
                                                                }
                                                                self.ui.print_agent_chunk(content_text);
                                                                full_response.push_str(content_text);
                                                            } else {
                                                                // Fallback for providers that don't prefix
                                                                if !has_content {
                                                                    if has_thinking {
                                                                        self.ui.print_thinking_end();
                                                                    }
                                                                    self.ui.print_agent_prefix(agent_number);
                                                                    has_content = true;
                                                                }
                                                                self.ui.print_agent_chunk(&content);
                                                                full_response.push_str(&content);
                                                            }
                                                        }
                                                    }
                                                    Err(e) => {
                                                        self.ui.print_error(&format!("Stream error: {}", e));
                                                        stream_error = true;
                                                        break;
                                                    }
                                                }
                                            }
                                            None => break, // Stream ended
                                        }
                                    }
                                    _ = cancel_token.cancelled() => {
                                        self.ui.print_info("\nRequest cancelled");
                                        // User message was never added to history, so nothing to remove
                                        return Err(anyhow::anyhow!("Request cancelled"));
                                    }
                                }
                            }
                            
                            if stream_error {
                                if attempt < max_retries {
                                    continue; // Retry on stream error
                                } else {
                                    // User message was never added to history, so nothing to remove
                                    return Err(anyhow::anyhow!("Stream error on final attempt"));
                                }
                            }
                            
                            self.ui.print_agent_newline();
                            
                            // Add the complete response to the session with metadata
                            if !full_response.is_empty() {
                                // Mark that this session has had LLM interaction
                                self.session.mark_llm_interaction();
                                
                                // Now that we have a successful response, add both user and assistant messages
                                self.session.add_message(user_message.clone());
                                let assistant_message = Message {
                                    role: "assistant".to_string(),
                                    content: full_response,
                                };
                                self.session.add_message_with_metadata(
                                    assistant_message,
                                    self.session.current_provider.clone(),
                                    Some(self.session.current_model.clone())
                                );
                                return Ok(()); // Success!
                            } else {
                                if attempt < max_retries {
                                    self.ui.print_error("Empty response received, retrying...");
                                    continue;
                                } else {
                                    // User message was never added to history, so nothing to remove
                                    return Err(anyhow::anyhow!("Empty response on final attempt"));
                                }
                            }
                        }
                        Err(e) => {
                            if attempt < max_retries {
                                self.ui.print_error(&format!("API error (attempt {}/{}): {}", attempt, max_retries, e));
                                continue; // Retry on API error
                            } else {
                                // User message was never added to history, so nothing to remove
                                return Err(e);
                            }
                        }
                    }
                }
            }
        }
        
        Err(anyhow::anyhow!("No provider available"))
    }
    
    async fn handle_auto_rename(&mut self, session_name: Option<String>) -> Result<()> {
        // Determine which session to rename
        let (target_session, is_current_session) = if let Some(ref name) = session_name {
            // Auto-rename specific session
            if let Some(session) = self.history.load_session(name) {
                (session.clone(), false)
            } else {
                return Err(anyhow::anyhow!("Session '{}' not found", name));
            }
        } else {
            // Auto-rename current session - it must be saved first
            if self.session.name.is_some() {
                (self.session.clone(), true)
            } else {
                return Err(anyhow::anyhow!("Current session has no name. Save it first with /chat save NAME"));
            }
        };
        
        // Check if session has messages
        if target_session.messages.is_empty() {
            return Err(anyhow::anyhow!("Cannot auto-rename empty session"));
        }
        
        // Find the first user message
        let first_user_message = target_session.messages.iter()
            .find(|msg| msg.message.role == "user")
            .ok_or_else(|| anyhow::anyhow!("No user messages found in session"))?;
        
        self.ui.print_info("Generating title...");
        
        // Generate title using LLM
        let generated_title = self.generate_session_title(&first_user_message.message.content).await?;
        
        // Sanitize the title
        let sanitized_title = self.sanitize_session_title(&generated_title);
        
        if sanitized_title.is_empty() {
            return Err(anyhow::anyhow!("Generated title is empty after sanitization"));
        }
        
        // Get the current session name for renaming
        let current_name = if is_current_session {
            if let Some(ref current_name) = self.session.name {
                current_name.clone()
            } else {
                return Err(anyhow::anyhow!("Current session has no name"));
            }
        } else {
            session_name.unwrap()
        };
        
        // Rename the session
        match self.history.rename_session(&current_name, &sanitized_title) {
            Ok(true) => {
                self.ui.print_info(&format!("Session \"{}\" auto-renamed to \"{}\"", current_name, sanitized_title));
                
                // If we renamed the current session, update its name and show context
                if is_current_session {
                    self.session.name = Some(sanitized_title.clone());
                    self.ui.print_info(&format!("Current session: \"{}\" ({} messages)", 
                        sanitized_title, self.session.messages.len()));
                }
                
                // Update completion context with new session name
                let _ = self.update_completion_context();
                Ok(())
            }
            Ok(false) => {
                Err(anyhow::anyhow!("Session '{}' not found", current_name))
            }
            Err(e) => {
                Err(e)
            }
        }
    }
    
    async fn generate_session_title(&self, first_message: &str) -> Result<String> {
        // Determine which provider to use (prefer current session's provider)
        let provider_name = self.get_current_provider()
            .or_else(|| {
                // Fallback to any available provider, preferring Anthropic
                if self.providers.contains_key("anthropic") {
                    Some("anthropic")
                } else if self.providers.contains_key("openai") {
                    Some("openai")
                } else if self.providers.contains_key("gemini") {
                    Some("gemini")
                } else {
                    None
                }
            })
            .ok_or_else(|| anyhow::anyhow!("No provider available for title generation"))?;
        
        let provider = self.providers.get(provider_name)
            .ok_or_else(|| anyhow::anyhow!("Provider '{}' not available", provider_name))?;
        
        // Choose a reliable model for title generation
        let model = match provider_name {
            "anthropic" => "claude-3-5-sonnet-20241022",
            "openai" => "gpt-4o",
            "gemini" => "gemini-2.5-pro",
            _ => return Err(anyhow::anyhow!("Unknown provider: {}", provider_name)),
        };
        
        // Create system prompt for title generation
        let system_prompt = "Generate a concise, descriptive title (3-8 words) for a conversation that begins with the following message. Respond with ONLY the title, no quotes, no explanation, no additional text.";
        
        // Create messages for the request
        let messages = vec![
            Message {
                role: "system".to_string(),
                content: system_prompt.to_string(),
            },
            Message {
                role: "user".to_string(),
                content: first_message.to_string(),
            },
        ];
        
        // Create chat request (non-streaming for simplicity)
        let chat_request = ChatRequest {
            messages,
            model: model.to_string(),
            temperature: 0.7,
            max_tokens: 50, // Short response expected
            thinking_budget: 0, // No thinking needed for title generation
            stream: false,
            thinking: false,
        };
        
        // Send request and collect response
        let mut stream = provider.chat(chat_request).await?;
        let mut response = String::new();
        
        while let Some(chunk_result) = stream.next().await {
            match chunk_result {
                Ok(chunk) => {
                    // Handle both prefixed and non-prefixed content
                    if chunk.starts_with("content:") {
                        response.push_str(&chunk[8..]);
                    } else {
                        response.push_str(&chunk);
                    }
                }
                Err(e) => {
                    return Err(anyhow::anyhow!("Error generating title: {}", e));
                }
            }
        }
        
        if response.trim().is_empty() {
            return Err(anyhow::anyhow!("Empty response from LLM"));
        }
        
        Ok(response.trim().to_string())
    }
    
    fn sanitize_session_title(&self, title: &str) -> String {
        let mut sanitized = title.trim().to_string();
        
        // Remove surrounding quotes if present
        if (sanitized.starts_with('"') && sanitized.ends_with('"')) ||
           (sanitized.starts_with('\'') && sanitized.ends_with('\'')) {
            sanitized = sanitized[1..sanitized.len()-1].to_string();
        }
        
        // Remove common prefixes that LLMs might add
        let prefixes_to_remove = [
            "Title: ",
            "title: ",
            "Session: ",
            "session: ",
            "Chat: ",
            "chat: ",
        ];
        
        for prefix in &prefixes_to_remove {
            if sanitized.starts_with(prefix) {
                sanitized = sanitized[prefix.len()..].to_string();
                break;
            }
        }
        
        // Limit length (reasonable session name length)
        if sanitized.len() > 80 {
            sanitized = sanitized[..80].to_string();
            // Try to break at a word boundary
            if let Some(last_space) = sanitized.rfind(' ') {
                if last_space > 40 { // Don't make it too short
                    sanitized = sanitized[..last_space].to_string();
                }
            }
        }
        
        // Replace problematic characters for session names
        sanitized = sanitized
            .replace('\n', " ")
            .replace('\r', " ")
            .replace('\t', " ");
        
        // Collapse multiple spaces
        while sanitized.contains("  ") {
            sanitized = sanitized.replace("  ", " ");
        }
        
        sanitized.trim().to_string()
    }
    
    async fn handle_summarize(&mut self, session_name: Option<String>) -> Result<()> {
        // Determine which session to summarize
        let target_session = if let Some(ref name) = session_name {
            // Summarize specific session
            if let Some(session) = self.history.load_session(name) {
                session.clone()
            } else {
                return Err(anyhow::anyhow!("Session '{}' not found", name));
            }
        } else {
            // Summarize current session
            self.session.clone()
        };
        
        // Check if session has messages
        if target_session.messages.is_empty() {
            return Err(anyhow::anyhow!("Cannot summarize empty session"));
        }
        
        // Show session info
        let session_display = if let Some(ref name) = session_name {
            format!("\"{}\"", name)
        } else {
            "current session".to_string()
        };
        
        self.ui.print_info(&format!("Generating summary for {} ({} messages)...", 
            session_display, target_session.messages.len()));
        
        // Generate summary using LLM
        let summary = self.generate_session_summary(&target_session).await?;
        
        // Display the summary
        println!();
        println!("\x1b[1;36mSession Summary\x1b[0m");
        if let Some(name) = &target_session.name {
            println!("Session: {}", name);
        }
        println!("Messages: {}", target_session.messages.len());
        println!("Created: {}", target_session.created_at.format("%Y-%m-%d %H:%M UTC"));
        println!("Model: {}", target_session.current_model);
        println!();
        println!("{}", summary);
        println!();
        
        Ok(())
    }
    
    async fn generate_session_summary(&self, session: &ChatSession) -> Result<String> {
        // Determine which provider to use (prefer current session's provider)
        let provider_name = self.get_current_provider()
            .or_else(|| {
                // Fallback to any available provider, preferring Anthropic
                if self.providers.contains_key("anthropic") {
                    Some("anthropic")
                } else if self.providers.contains_key("openai") {
                    Some("openai")
                } else if self.providers.contains_key("gemini") {
                    Some("gemini")
                } else {
                    None
                }
            })
            .ok_or_else(|| anyhow::anyhow!("No provider available for summary generation"))?;
        
        let provider = self.providers.get(provider_name)
            .ok_or_else(|| anyhow::anyhow!("Provider '{}' not available", provider_name))?;
        
        // Choose a reliable model for summary generation
        let model = match provider_name {
            "anthropic" => "claude-3-5-sonnet-20241022",
            "openai" => "gpt-4o",
            "gemini" => "gemini-2.5-pro",
            _ => return Err(anyhow::anyhow!("Unknown provider: {}", provider_name)),
        };
        
        // Create system prompt for summary generation
        let system_prompt = "You are tasked with summarizing a conversation between a user and an AI assistant. Provide a concise but comprehensive summary that captures:

1. The main topics discussed
2. Key questions asked by the user
3. Important information or solutions provided
4. Any code, technical concepts, or specific domains covered
5. The overall flow and progression of the conversation

Format your summary in clear, readable paragraphs. Be objective and factual.";
        
        // Build conversation text from session messages
        let mut conversation_text = String::new();
        for (i, numbered_message) in session.messages.iter().enumerate() {
            let role = match numbered_message.message.role.as_str() {
                "user" => "User",
                "assistant" => "Assistant",
                "system" => "System",
                _ => "Unknown",
            };
            
            conversation_text.push_str(&format!("{}. {} ({}): {}\n\n", 
                i + 1, 
                role,
                numbered_message.timestamp.format("%H:%M:%S"),
                numbered_message.message.content
            ));
        }
        
        // Create messages for the request
        let messages = vec![
            Message {
                role: "system".to_string(),
                content: system_prompt.to_string(),
            },
            Message {
                role: "user".to_string(),
                content: format!("Please summarize the following conversation:\n\n{}", conversation_text),
            },
        ];
        
        // Create chat request (non-streaming for simplicity)
        let chat_request = ChatRequest {
            messages,
            model: model.to_string(),
            temperature: 0.3, // Lower temperature for more consistent summaries
            max_tokens: 1000, // Allow for detailed summaries
            thinking_budget: 0, // No thinking needed for summary generation
            stream: false,
            thinking: false,
        };
        
        // Send request and collect response
        let mut stream = provider.chat(chat_request).await?;
        let mut response = String::new();
        
        while let Some(chunk_result) = stream.next().await {
            match chunk_result {
                Ok(chunk) => {
                    // Handle both prefixed and non-prefixed content
                    if chunk.starts_with("content:") {
                        response.push_str(&chunk[8..]);
                    } else {
                        response.push_str(&chunk);
                    }
                }
                Err(e) => {
                    return Err(anyhow::anyhow!("Error generating summary: {}", e));
                }
            }
        }
        
        if response.trim().is_empty() {
            return Err(anyhow::anyhow!("Empty response from LLM"));
        }
        
        Ok(response.trim().to_string())
    }
}
