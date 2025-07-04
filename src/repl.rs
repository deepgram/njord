use anyhow::Result;
use std::collections::HashMap;
use futures::StreamExt;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use chrono::Utc;
use std::path::Path;

use crate::{
    commands::{Command, CommandParser, CopyType, SaveType, SessionReference},
    config::Config,
    history::History,
    providers::{create_provider, get_provider_for_model, LLMProvider, Message, ChatRequest},
    session::{ChatSession, CodeBlock},
    ui::{UI, CompletionContext},
    prompts::PromptLibrary,
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
    prompts: PromptLibrary,
    command_parser: CommandParser,
    ui: UI,
    queued_message: Option<String>,
    active_request_token: Option<CancellationToken>,
    interrupted_message: Option<String>,
    ctrl_c_rx: mpsc::UnboundedReceiver<()>,
    last_session_list: Vec<String>, // For ephemeral session references
    variables: HashMap<String, String>, // For file content variables
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
        
        let history = History::load(config.sessions_file())?;
        let prompts = PromptLibrary::load(config.prompts_file())?;
        
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
        let mut ui = if config.ephemeral {
            // In ephemeral mode, try to load existing input history but don't fail if it doesn't exist
            UI::with_input_history_file_ephemeral(config.inputs_file())?
        } else {
            UI::with_input_history_file(config.inputs_file())?
        };
        
        // Set up initial completion context
        let variables = HashMap::new();
        let completion_context = Self::build_completion_context(&providers, &history, &prompts, &variables);
        ui.update_completion_context(completion_context)?;
        
        // Auto-populate ephemeral session list on startup (newest first)
        let last_session_list = history.list_sessions().iter().map(|s| s.to_string()).collect();
        
        Ok(Self {
            config,
            providers,
            session,
            history,
            prompts,
            command_parser,
            ui,
            queued_message: None,
            active_request_token: None,
            interrupted_message: None,
            ctrl_c_rx,
            last_session_list,
            variables,
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
    
    fn build_completion_context(providers: &HashMap<String, Box<dyn LLMProvider>>, history: &History, prompts: &PromptLibrary, variables: &HashMap<String, String>) -> CompletionContext {
        let mut available_models = Vec::new();
        
        // Collect all models from all providers
        for provider in providers.values() {
            available_models.extend(provider.get_models());
        }
        
        // Sort models for better completion experience
        available_models.sort();
        
        // Get session names
        let session_names = history.list_sessions().into_iter().cloned().collect();
        
        // Get prompt names
        let prompt_names = prompts.list_prompts().into_iter().cloned().collect();
        
        // Get variable names
        let variable_names = variables.keys().cloned().collect();
        
        CompletionContext {
            available_models,
            session_names,
            prompt_names,
            variable_names,
        }
    }
    
    fn update_completion_context(&mut self) -> Result<()> {
        let context = Self::build_completion_context(&self.providers, &self.history, &self.prompts, &self.variables);
        self.ui.update_completion_context(context)
    }
    
    fn update_session_list(&mut self) {
        self.last_session_list = self.history.list_sessions().iter().map(|s| s.to_string()).collect();
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
    
    fn copy_content_to_clipboard(&self, content: &str, description: &str) -> Result<()> {
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
            self.ui.print_info(&format!("{} copied via: {}", 
                description, success_methods.join(", ")));
        } else {
            self.ui.print_error("Failed to copy to clipboard");
            self.ui.print_info("Content displayed below:");
            println!();
            println!("{}", content);
        }
        
        // Save input history on exit
        if let Err(e) = self.ui.save_input_history() {
            eprintln!("Warning: Failed to save input history on exit: {}", e);
        }
        
        Ok(())
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
        
        // Show input history stats on startup
        let (history_count, _) = self.ui.get_input_history_stats();
        if history_count > 0 {
            self.ui.print_info(&format!("Input history: {} entries loaded", history_count));
        }
        
        loop {
            // Determine what message to show in prompt
            let prompt_message = if let Some(interrupted) = &self.interrupted_message {
                Some((interrupted.as_str(), "interrupted"))
            } else {
                self.queued_message.as_ref().map(|queued| (queued.as_str(), "retry"))
            };
            
            // Determine session name for prompt
            let session_name = if self.session.messages.is_empty() {
                None // Don't show session name for empty sessions
            } else {
                self.session.name.as_deref()
            };
            
            if let Some(input) = self.ui.read_input(prompt_message, session_name, self.config.ephemeral)? {
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
                    // Clear input buffer to prevent processing queued input
                    self.ui.clear_input_buffer();
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
                    
                    // Clear input buffer after processing message to prevent
                    // any residual pasted content from being processed
                    self.ui.clear_input_buffer();
                }
                
                // Auto-save session if it has LLM interactions
                if let Err(e) = self.history.auto_save_session(&self.session) {
                    self.ui.print_error(&format!("Failed to auto-save session: {}", e));
                } else {
                    // Update completion context and session list after auto-save
                    let _ = self.update_completion_context();
                    self.update_session_list();
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
            println!("  State directory: {}", self.config.state_directory);
            
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
            } else if self.session.thinking_enabled { "enabled".to_string() } else { "disabled".to_string() }
        } else if self.session.thinking_enabled { "enabled".to_string() } else { "disabled".to_string() }
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
    
    fn get_agent_message(&self, agent_number: Option<usize>) -> Option<&str> {
        let target_number = agent_number.unwrap_or_else(|| {
            // Get the most recent agent message number
            self.session.messages.iter()
                .filter(|msg| msg.message.role == "assistant")
                .count()
        });
        
        if target_number == 0 {
            return None;
        }
        
        let mut agent_count = 0;
        for msg in &self.session.messages {
            if msg.message.role == "assistant" {
                agent_count += 1;
                if agent_count == target_number {
                    return Some(&msg.message.content);
                }
            }
        }
        None
    }
    
    fn get_user_message(&self, user_number: Option<usize>) -> Option<&str> {
        let target_number = user_number.unwrap_or_else(|| {
            // Get the most recent user message number
            self.session.messages.iter()
                .filter(|msg| msg.message.role == "user")
                .count()
        });
        
        if target_number == 0 {
            return None;
        }
        
        let mut user_count = 0;
        for msg in &self.session.messages {
            if msg.message.role == "user" {
                user_count += 1;
                if user_count == target_number {
                    return Some(&msg.message.content);
                }
            }
        }
        None
    }
    
    fn resolve_session_reference(&self, session_ref: &SessionReference) -> Result<String> {
        match session_ref {
            SessionReference::Named(name) => Ok(name.clone()),
            SessionReference::Ephemeral(number) => {
                if *number == 0 || *number > self.last_session_list.len() {
                    Err(anyhow::anyhow!("Invalid ephemeral session reference #{}", number))
                } else {
                    Ok(self.last_session_list[*number - 1].clone())
                }
            }
            SessionReference::Invalid(error) => {
                Err(anyhow::anyhow!("{}", error))
            }
        }
    }
    
    async fn handle_command(&mut self, command: Command) -> Result<bool> {
        match command {
            Command::Quit => return Ok(false),
            Command::Help => {
                self.ui.print_info("Available commands:");
                println!();
                
                // Basic Commands
                println!("\x1b[1;36mBasic Commands:\x1b[0m");
                println!("  /help - Show all commands");
                println!("  /status - Show current configuration");
                println!("  /models - List available models across all providers");
                println!("  /quit - Exit Njord");
                println!();
                
                // Session Management
                println!("\x1b[1;36mSession Management:\x1b[0m");
                println!("  /chat new - Start a new chat session");
                println!("  /chat save NAME - Save current session with given name");
                println!("  /chat load NAME|#N - Load a previously saved session");
                println!("  /chat continue [NAME|#N] - Continue most recent or specified session");
                println!("  /chat list - List all saved sessions with ephemeral numbers");
                println!("  /chat recent - Show recent sessions");
                println!("  /chat delete [NAME|#N] - Delete a saved session (defaults to current)");
                println!("  /chat fork NAME - Save current session and start fresh");
                println!("  /chat merge NAME - Merge another session into current");
                println!("  /chat rename NEW_NAME [OLD_NAME] - Rename a session");
                println!("  /chat auto-rename [NAME] - Auto-generate title for session");
                println!("  /chat auto-rename-all - Auto-generate titles for all anonymous sessions");
                println!("  /summarize [NAME] - Generate summary of session");
                println!();
                
                // Message Navigation
                println!("\x1b[1;36mMessage Navigation:\x1b[0m");
                println!("  /history - Show conversation history");
                println!("  /undo [N] - Undo last N agent responses (restores user message for editing)");
                println!("  /goto N - Jump back to Agent N (removes later messages)");
                println!("  /search TERM - Search through chat history");
                println!("  /retry - Regenerate last response");
                println!("  /edit N - Edit and resend message N");
                println!();
                
                // Content Management
                println!("\x1b[1;36mContent Management:\x1b[0m");
                println!("  /blocks - List all code blocks in session");
                println!("  /block N - Display code block N");
                println!("  /copy [TYPE] [N] - Copy message/block to clipboard");
                println!("    \x1b[1;32mEx:\x1b[0m /copy - Copy most recent agent response");
                println!("    \x1b[1;32mEx:\x1b[0m /copy agent 2 - Copy Agent #2 response");
                println!("    \x1b[1;32mEx:\x1b[0m /copy user 1 - Copy User #1 message");
                println!("    \x1b[1;32mEx:\x1b[0m /copy block 3 - Copy code block #3");
                println!("  /save [TYPE] [N] FILE - Save message/block to file");
                println!("    \x1b[1;32mEx:\x1b[0m /save response.md - Save most recent agent response");
                println!("    \x1b[1;32mEx:\x1b[0m /save agent 2 analysis.md - Save Agent #2 response");
                println!("    \x1b[1;32mEx:\x1b[0m /save user 1 question.txt - Save User #1 message");
                println!("    \x1b[1;32mEx:\x1b[0m /save block 3 code.py - Save code block #3");
                println!("  /exec N - Execute code block N (with confirmation)");
                println!("  /export FORMAT - Export chat (markdown, json, txt)");
                println!();
                
                // File & Variable Operations
                println!("\x1b[1;36mFile & Variable Operations:\x1b[0m");
                println!("  /load FILE [VAR] - Load file content into variable");
                println!("    \x1b[1;32mEx:\x1b[0m /load config.json - Load as {{{{config_json}}}}");
                println!("    \x1b[1;32mEx:\x1b[0m /load data.txt mydata - Load as {{{{mydata}}}}");
                println!("  /variables - List all loaded variables");
                println!("  /var show VAR - Show variable content");
                println!("  /var delete VAR - Delete a variable");
                println!("  /var reload [VAR] - Reload variable(s) from file(s)");
                println!("    \x1b[1;32mEx:\x1b[0m /var reload - Reload all variables");
                println!("    \x1b[1;32mEx:\x1b[0m /var reload myvar - Reload specific variable");
                println!();
                
                // System Prompts & Library
                println!("\x1b[1;36mSystem Prompts & Library:\x1b[0m");
                println!("  /system [PROMPT] - Set system prompt (empty to view, 'clear' to remove)");
                println!("  /prompts list - List all saved system prompts");
                println!("  /prompts show NAME - Display a specific prompt");
                println!("  /prompts save NAME [CONTENT] - Save current or specified system prompt");
                println!("  /prompts apply NAME - Apply a saved prompt to current session");
                println!("  /prompts delete NAME - Remove a saved prompt");
                println!("  /prompts rename OLD_NAME NEW_NAME - Rename a prompt");
                println!("  /prompts search TERM - Search prompt names and content");
                println!("  /prompts auto-name [NAME] - Auto-generate name for prompt");
                println!("  /prompts edit NAME - Edit an existing prompt");
                println!("  /prompts import FILE - Import prompts from JSON file");
                println!("  /prompts export [FILE] - Export prompts to JSON file");
                println!();
                
                // Model & Settings
                println!("\x1b[1;36mModel & Settings:\x1b[0m");
                println!("  /model MODEL - Switch to a different model (auto-detects provider)");
                println!("  /temp TEMPERATURE - Set temperature (0.0-2.0)");
                println!("  /max-tokens TOKENS - Set maximum output tokens");
                println!("  /thinking on|off - Enable/disable thinking for supported models");
                println!("  /thinking-budget TOKENS - Set thinking token budget");
                println!("  /tokens - Show token usage stats");
                println!("  /stats - Show session statistics");
                println!("  /clear - Clear terminal display (keep history)");
                println!();
                
                // Input History
                println!("\x1b[1;36mInput History:\x1b[0m");
                println!("  /input-history - Show input history information");
                println!("  /input-history clear - Clear all input history");
                println!("  /input-history stats - Show detailed input history statistics");
                println!();
                
                // Usage Tips
                println!("\x1b[1;36mUsage Tips:\x1b[0m");
                println!("  • Start with {{ for multi-line input (end with }} on its own line)");
                println!("  • Use {{{{VARIABLE_NAME}}}} in messages to reference loaded file content");
                println!("  • Use #N for ephemeral session references (e.g., /chat load #1)");
                println!("  • Quote session names with spaces (e.g., /chat load \"My Session\")");
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
                    // Update completion context and session list after auto-save
                    let _ = self.update_completion_context();
                    self.update_session_list();
                }
                
                self.session = ChatSession::new(self.config.default_model.clone(), self.config.temperature, self.config.max_tokens, self.config.thinking_budget);
                self.session.current_provider = get_provider_for_model(&self.session.current_model).map(|s| s.to_string());
                self.ui.print_info("Started new chat session");
            }
            Command::ChatContinue(session_ref_opt) => {
                if self.config.ephemeral {
                    self.ui.print_error("Cannot continue sessions in ephemeral mode (would modify original session)");
                    self.ui.print_info("Use '/chat load SESSION' to copy a session instead");
                    return Ok(true);
                }
                
                let target_session = if let Some(session_ref) = session_ref_opt {
                    // Continue specific session by reference
                    match self.resolve_session_reference(&session_ref) {
                        Ok(name) => self.history.load_session(&name).cloned(),
                        Err(e) => {
                            self.ui.print_error(&e.to_string());
                            return Ok(true);
                        }
                    }
                } else {
                    // Continue most recent session
                    self.history.get_most_recent_session().cloned()
                };
                
                if let Some(target_session) = target_session {
                    // Auto-save current session if it has interactions
                    if let Err(e) = self.history.auto_save_session(&self.session) {
                        self.ui.print_error(&format!("Failed to auto-save current session: {}", e));
                    } else {
                        // Update completion context and session list after auto-save
                        let _ = self.update_completion_context();
                        self.update_session_list();
                    }
                    
                    self.session = target_session;
                    // Update current provider based on session's model
                    self.session.current_provider = get_provider_for_model(&self.session.current_model).map(|s| s.to_string());
                            
                    // Restore session variables
                    let session_clone = self.session.clone();
                    self.restore_session_variables(&session_clone);
                            
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
                    self.ui.print_error("No sessions found to continue");
                }
            }
            Command::ChatRecent => {
                let recent_sessions = self.history.get_recent_sessions(10);
                if recent_sessions.is_empty() {
                    self.ui.print_info("No recent sessions found");
                } else {
                    self.ui.print_info("Recent sessions:");
                    
                    // Update the ephemeral session list with recent sessions
                    self.last_session_list = recent_sessions.iter().map(|(name, _)| (*name).clone()).collect();
                    
                    for (index, (name, session)) in recent_sessions.iter().enumerate() {
                        let message_count = session.messages.len();
                        let updated = session.updated_at.format("%Y-%m-%d %H:%M");
                        println!("  #{}: \"{}\" ({} messages, updated {})", 
                            index + 1, name, message_count, updated);
                    }
                    println!();
                    self.ui.print_info("Use '/chat continue #N' to continue by number or '/chat continue \"name\"' to continue by name");
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
                            // Update completion context and session list with new session
                            let _ = self.update_completion_context();
                            self.update_session_list();
                        }
                        Err(e) => {
                            self.ui.print_error(&format!("Failed to fork session: {}", e));
                        }
                    }
                }
            }
            Command::ChatMerge(session_ref) => {
                match self.resolve_session_reference(&session_ref) {
                    Ok(name) => {
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
                    Err(e) => {
                        self.ui.print_error(&e.to_string());
                    }
                }
            }
            Command::ChatRename(new_name, old_session_ref) => {
                if new_name.trim().is_empty() {
                    self.ui.print_error("New session name cannot be empty");
                } else {
                    let target_name = if let Some(ref session_ref) = old_session_ref {
                        // Rename specific session by reference
                        match self.resolve_session_reference(session_ref) {
                            Ok(name) => name,
                            Err(e) => {
                                self.ui.print_error(&e.to_string());
                                return Ok(true);
                            }
                        }
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
                            if old_session_ref.is_none() || self.session.name.as_ref() == Some(&target_name) {
                                self.session.name = Some(new_name.clone());
                                self.ui.print_info(&format!("Current session: \"{}\" ({} messages)", 
                                    new_name, self.session.messages.len()));
                            }
                        
                            // Update completion context and session list with new session name
                            let _ = self.update_completion_context();
                            self.update_session_list();
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
            Command::ChatAutoRename(session_ref_opt) => {
                match self.handle_auto_rename(session_ref_opt.as_ref()).await {
                    Ok(()) => {
                        // Success message already printed in handle_auto_rename
                    }
                    Err(e) => {
                        self.ui.print_error(&format!("Failed to auto-rename session: {}", e));
                    }
                }
            }
            Command::ChatAutoRenameAll => {
                match self.handle_auto_rename_all().await {
                    Ok(()) => {
                        // Success message already printed in handle_auto_rename_all
                    }
                    Err(e) => {
                        self.ui.print_error(&format!("Failed to auto-rename sessions: {}", e));
                    }
                }
            }
            Command::Summarize(session_ref_opt) => {
                match self.handle_summarize(session_ref_opt.as_ref()).await {
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
                            // Update completion context and session list with new session
                            let _ = self.update_completion_context();
                            self.update_session_list();
                        }
                        Err(e) => {
                            self.ui.print_error(&format!("Failed to save session: {}", e));
                        }
                    }
                }
            }
            Command::ChatLoad(session_ref) => {
                match self.resolve_session_reference(&session_ref) {
                    Ok(name) => {
                        // First, clone the session if it exists
                        let session_to_load = self.history.load_session(&name).cloned();
                        
                        if let Some(session) = session_to_load {
                            // Auto-save current session if it has interactions
                            if let Err(e) = self.history.auto_save_session(&self.session) {
                                self.ui.print_error(&format!("Failed to auto-save current session: {}", e));
                            } else {
                                // Update completion context and session list after auto-save
                                let _ = self.update_completion_context();
                                self.update_session_list();
                            }
                            
                            // Create a copy of the session (new ID, no name, fresh timestamps)
                            self.session = session.create_copy();
                            
                            // Update current provider based on session's model
                            self.session.current_provider = get_provider_for_model(&self.session.current_model).map(|s| s.to_string());
                            
                            // Restore session variables
                            let session_clone = self.session.clone();
                            self.restore_session_variables(&session_clone);
                            
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
                    Err(e) => {
                        self.ui.print_error(&e.to_string());
                    }
                }
            }
            Command::ChatList => {
                let sessions = self.history.list_sessions();
                if sessions.is_empty() {
                    self.ui.print_info("No saved sessions");
                } else {
                    self.ui.print_info("Saved sessions:");
                    
                    // Update the ephemeral session list
                    self.last_session_list = sessions.iter().map(|s| s.to_string()).collect();
                    
                    for (index, session_name) in sessions.iter().enumerate() {
                        if let Some(session) = self.history.load_session(session_name) {
                            let message_count = session.messages.len();
                            let code_blocks = session.messages.iter()
                                .map(|msg| msg.code_blocks.len())
                                .sum::<usize>();
                            
                            let updated = session.updated_at.format("%Y-%m-%d %H:%M");
                            println!("  #{}: \"{}\" ({} messages, {} blocks, updated {})", 
                                index + 1, session_name, message_count, code_blocks, updated);
                        }
                    }
                    println!();
                    self.ui.print_info("Sessions ordered by most recent update. Use '/chat load #N' to load by number or '/chat load \"name\"' to load by name");
                }
            }
            Command::ChatDelete(session_ref_opt) => {
                if let Some(session_ref) = session_ref_opt {
                    // Delete specific session by reference
                    let target_name = match self.resolve_session_reference(&session_ref) {
                        Ok(name) => name,
                        Err(e) => {
                            self.ui.print_error(&e.to_string());
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
                            
                            // Update completion context and session list after deletion
                            let _ = self.update_completion_context();
                            self.update_session_list();
                        }
                        Ok(false) => {
                            self.ui.print_error(&format!("Session '{}' not found", target_name));
                        }
                        Err(e) => {
                            self.ui.print_error(&format!("Failed to delete session: {}", e));
                        }
                    }
                } else {
                    // Delete current session (no session reference provided)
                    if let Some(ref current_name) = self.session.name {
                        // Current session has a name - delete it from history
                        match self.history.delete_session(current_name) {
                            Ok(true) => {
                                self.ui.print_info(&format!("Session '{}' deleted", current_name));
                                
                                // Reset to a new anonymous session
                                self.session = ChatSession::new(
                                    self.config.default_model.clone(), 
                                    self.config.temperature, 
                                    self.config.max_tokens, 
                                    self.config.thinking_budget
                                );
                                self.session.current_provider = get_provider_for_model(&self.session.current_model).map(|s| s.to_string());
                                self.ui.print_info("Started new anonymous session");
                                
                                // Update completion context and session list after deletion
                                let _ = self.update_completion_context();
                                self.update_session_list();
                            }
                            Ok(false) => {
                                self.ui.print_error(&format!("Session '{}' not found", current_name));
                            }
                            Err(e) => {
                                self.ui.print_error(&format!("Failed to delete session: {}", e));
                            }
                        }
                    } else {
                        // Current session is anonymous - just clear it and start fresh
                        let message_count = self.session.messages.len();
                        self.session = ChatSession::new(
                            self.config.default_model.clone(), 
                            self.config.temperature, 
                            self.config.max_tokens, 
                            self.config.thinking_budget
                        );
                        self.session.current_provider = get_provider_for_model(&self.session.current_model).map(|s| s.to_string());
                        
                        if message_count > 0 {
                            self.ui.print_info(&format!("Current session cleared ({} messages discarded) - started new session", message_count));
                        } else {
                            self.ui.print_info("Started new session");
                        }
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
                if !(0.0..=2.0).contains(&temp) {
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
            Command::Copy(copy_type, number) => {
                match copy_type {
                    CopyType::Agent => {
                        if let Some(content) = self.get_agent_message(number) {
                            let display_number = number.unwrap_or_else(|| {
                                self.session.messages.iter()
                                    .filter(|msg| msg.message.role == "assistant")
                                    .count()
                            });
                            self.copy_content_to_clipboard(content, &format!("Agent #{}", display_number))?;
                        } else {
                            let display_number = number.unwrap_or(1);
                            self.ui.print_error(&format!("Agent #{} not found", display_number));
                        }
                    }
                    CopyType::User => {
                        if let Some(content) = self.get_user_message(number) {
                            let display_number = number.unwrap_or_else(|| {
                                self.session.messages.iter()
                                    .filter(|msg| msg.message.role == "user")
                                    .count()
                            });
                            self.copy_content_to_clipboard(content, &format!("User #{}", display_number))?;
                        } else {
                            let display_number = number.unwrap_or(1);
                            self.ui.print_error(&format!("User #{} not found", display_number));
                        }
                    }
                    CopyType::Block => {
                        let block_number = number.unwrap_or(1);
                        let all_blocks = self.get_all_code_blocks();
                        if let Some(block) = all_blocks.get(block_number.saturating_sub(1)) {
                            self.copy_content_to_clipboard(&block.code_block.content, &format!("Code block #{}", block_number))?;
                        } else {
                            self.ui.print_error(&format!("Code block {} not found. Use /blocks to list all code blocks.", block_number));
                        }
                    }
                }
            }
            Command::Save(save_type, number, filename) => {
                match save_type {
                    SaveType::Agent => {
                        if let Some(content) = self.get_agent_message(number) {
                            match std::fs::write(&filename, content) {
                                Ok(()) => {
                                    let display_number = number.unwrap_or_else(|| {
                                        self.session.messages.iter()
                                            .filter(|msg| msg.message.role == "assistant")
                                            .count()
                                    });
                                    self.ui.print_info(&format!("Agent #{} saved to '{}'", display_number, filename));
                                }
                                Err(e) => {
                                    self.ui.print_error(&format!("Failed to save agent response: {}", e));
                                }
                            }
                        } else {
                            let display_number = number.unwrap_or(1);
                            self.ui.print_error(&format!("Agent #{} not found", display_number));
                        }
                    }
                    SaveType::User => {
                        if let Some(content) = self.get_user_message(number) {
                            match std::fs::write(&filename, content) {
                                Ok(()) => {
                                    let display_number = number.unwrap_or_else(|| {
                                        self.session.messages.iter()
                                            .filter(|msg| msg.message.role == "user")
                                            .count()
                                    });
                                    self.ui.print_info(&format!("User #{} saved to '{}'", display_number, filename));
                                }
                                Err(e) => {
                                    self.ui.print_error(&format!("Failed to save user message: {}", e));
                                }
                            }
                        } else {
                            let display_number = number.unwrap_or(1);
                            self.ui.print_error(&format!("User #{} not found", display_number));
                        }
                    }
                    SaveType::Block => {
                        let block_number = number.unwrap_or(1);
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
                    
                    if let Some(confirmation) = self.ui.read_input(None, None, self.config.ephemeral)? {
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
            // Prompt library commands
            Command::PromptsList => {
                let prompt_names = self.prompts.list_prompts();
                if prompt_names.is_empty() {
                    self.ui.print_info("No saved prompts found");
                } else {
                    self.ui.print_info(&format!("Saved prompts ({} total):", prompt_names.len()));
                    for name in prompt_names {
                        if let Some(prompt) = self.prompts.get_prompt(name) {
                            let usage_info = if prompt.usage_count > 0 {
                                format!(" (used {} times)", prompt.usage_count)
                            } else {
                                String::new()
                            };
                            
                            let preview = if prompt.content.len() > 60 {
                                format!("{}...", &prompt.content[..60].replace('\n', " "))
                            } else {
                                prompt.content.replace('\n', " ")
                            };
                            
                            println!("  \"{}\"{}: {}", name, usage_info, preview);
                        }
                    }
                    println!();
                    self.ui.print_info("Use '/prompts show NAME' to view, '/prompts apply NAME' to use");
                }
            }
            Command::PromptsShow(name) => {
                if let Some(prompt) = self.prompts.get_prompt(&name) {
                    self.ui.print_info(&format!("Prompt: \"{}\"", name));
                    if let Some(ref description) = prompt.description {
                        println!("Description: {}", description);
                    }
                    if !prompt.tags.is_empty() {
                        println!("Tags: {}", prompt.tags.join(", "));
                    }
                    println!("Created: {}", prompt.created_at.format("%Y-%m-%d %H:%M UTC"));
                    println!("Updated: {}", prompt.updated_at.format("%Y-%m-%d %H:%M UTC"));
                    println!("Usage count: {}", prompt.usage_count);
                    println!();
                    println!("{}", prompt.content);
                } else {
                    self.ui.print_error(&format!("Prompt '{}' not found", name));
                    let available_prompts = self.prompts.list_prompts();
                    if !available_prompts.is_empty() {
                        self.ui.print_info("Available prompts:");
                        for prompt_name in available_prompts.iter().take(5) {
                            println!("  {}", prompt_name);
                        }
                    }
                }
            }
            Command::PromptsSave(name, content_opt) => {
                let content = if let Some(content) = content_opt {
                    content
                } else if let Some(ref current_prompt) = self.session.system_prompt {
                    current_prompt.clone()
                } else {
                    self.ui.print_error("No system prompt is currently set and no content provided");
                    return Ok(true);
                };
                
                if content.trim().is_empty() {
                    self.ui.print_error("Cannot save empty prompt");
                    return Ok(true);
                }
                
                // Ensure unique name
                let unique_name = self.prompts.ensure_unique_prompt_name(&name);
                if unique_name != name {
                    self.ui.print_info(&format!("Name '{}' already exists, using '{}'", name, unique_name));
                }
                
                match self.prompts.save_prompt(unique_name.clone(), content) {
                    Ok(()) => {
                        self.ui.print_info(&format!("Prompt saved as '{}'", unique_name));
                        // Update completion context with new prompt
                        let _ = self.update_completion_context();
                    }
                    Err(e) => {
                        self.ui.print_error(&format!("Failed to save prompt: {}", e));
                    }
                }
            }
            Command::PromptsApply(name) => {
                if let Some(content) = self.prompts.apply_prompt(&name) {
                    self.session.system_prompt = Some(content.clone());
                    self.ui.print_info(&format!("Applied prompt '{}' to current session", name));
                    
                    // Show a preview of the applied prompt
                    let preview = if content.len() > 100 {
                        format!("{}...", &content[..100].replace('\n', " "))
                    } else {
                        content.replace('\n', " ")
                    };
                    self.ui.print_info(&format!("System prompt: {}", preview));
                } else {
                    self.ui.print_error(&format!("Prompt '{}' not found", name));
                    let available_prompts = self.prompts.list_prompts();
                    if !available_prompts.is_empty() {
                        self.ui.print_info("Available prompts:");
                        for prompt_name in available_prompts.iter().take(5) {
                            println!("  {}", prompt_name);
                        }
                    }
                }
            }
            Command::PromptsDelete(name) => {
                match self.prompts.delete_prompt(&name) {
                    Ok(true) => {
                        self.ui.print_info(&format!("Prompt '{}' deleted", name));
                        // Update completion context after deletion
                        let _ = self.update_completion_context();
                    }
                    Ok(false) => {
                        self.ui.print_error(&format!("Prompt '{}' not found", name));
                    }
                    Err(e) => {
                        self.ui.print_error(&format!("Failed to delete prompt: {}", e));
                    }
                }
            }
            Command::PromptsRename(old_name, new_name) => {
                if new_name.trim().is_empty() {
                    self.ui.print_error("New prompt name cannot be empty");
                } else {
                    match self.prompts.rename_prompt(&old_name, &new_name) {
                        Ok(true) => {
                            self.ui.print_info(&format!("Prompt '{}' renamed to '{}'", old_name, new_name));
                            // Update completion context with new name
                            let _ = self.update_completion_context();
                        }
                        Ok(false) => {
                            self.ui.print_error(&format!("Prompt '{}' not found", old_name));
                        }
                        Err(e) => {
                            self.ui.print_error(&format!("Failed to rename prompt: {}", e));
                        }
                    }
                }
            }
            Command::PromptsSearch(term) => {
                if term.trim().is_empty() {
                    self.ui.print_error("Search term cannot be empty");
                } else {
                    let results = self.prompts.search_prompts(&term);
                    
                    if results.is_empty() {
                        self.ui.print_info(&format!("No prompts found matching '{}'", term));
                    } else {
                        self.ui.print_info(&format!("Search results for '{}' ({} matches):", term, results.len()));
                        println!();
                        
                        for result in results {
                            let usage_info = if result.prompt.usage_count > 0 {
                                format!(" (used {} times)", result.prompt.usage_count)
                            } else {
                                String::new()
                            };
                            
                            let preview = if result.prompt.content.len() > 80 {
                                format!("{}...", &result.prompt.content[..80].replace('\n', " "))
                            } else {
                                result.prompt.content.replace('\n', " ")
                            };
                            
                            println!("  \x1b[1;36m\"{}\"\x1b[0m{} [{}]: {}", 
                                result.name, 
                                usage_info,
                                result.matched_fields.join(", "),
                                preview
                            );
                        }
                        
                        println!();
                        self.ui.print_info("Use '/prompts show NAME' to view full prompt");
                    }
                }
            }
            Command::PromptsAutoName(name_opt) => {
                match self.handle_prompt_auto_name(name_opt.as_ref()).await {
                    Ok(()) => {
                        // Success message already printed in handle_prompt_auto_name
                    }
                    Err(e) => {
                        self.ui.print_error(&format!("Failed to auto-name prompt: {}", e));
                    }
                }
            }
            Command::PromptsEdit(name) => {
                if let Some(prompt) = self.prompts.get_prompt(&name) {
                    self.ui.print_info(&format!("Current content of prompt '{}':", name));
                    println!();
                    println!("{}", prompt.content);
                    println!();
                    self.ui.print_info("Enter new content (use {{ and }} for multi-line input):");
                    
                    if let Some(new_content) = self.ui.read_input(None, None, self.config.ephemeral)? {
                        if !new_content.trim().is_empty() {
                            match self.prompts.update_prompt_content(&name, new_content) {
                                Ok(true) => {
                                    self.ui.print_info(&format!("Prompt '{}' updated", name));
                                }
                                Ok(false) => {
                                    self.ui.print_error(&format!("Prompt '{}' not found", name));
                                }
                                Err(e) => {
                                    self.ui.print_error(&format!("Failed to update prompt: {}", e));
                                }
                            }
                        } else {
                            self.ui.print_info("Edit cancelled - empty content");
                        }
                    }
                } else {
                    self.ui.print_error(&format!("Prompt '{}' not found", name));
                }
            }
            Command::PromptsImport(filename) => {
                match self.prompts.import_prompts(&filename, false) {
                    Ok(result) => {
                        self.ui.print_info(&format!("Import complete: {} imported, {} skipped, {} overwritten", 
                            result.imported_count, result.skipped_count, result.overwritten_count));
                        
                        if result.skipped_count > 0 {
                            self.ui.print_info("Use '/prompts import --overwrite FILE' to overwrite existing prompts");
                        }
                        
                        // Update completion context with new prompts
                        let _ = self.update_completion_context();
                    }
                    Err(e) => {
                        self.ui.print_error(&format!("Failed to import prompts: {}", e));
                    }
                }
            }
            Command::PromptsExport(filename_opt) => {
                match self.prompts.export_prompts(filename_opt.as_deref()) {
                    Ok(message) => {
                        if filename_opt.is_some() {
                            self.ui.print_info(&message);
                        } else {
                            // Print to stdout
                            println!("{}", message);
                        }
                    }
                    Err(e) => {
                        self.ui.print_error(&format!("Failed to export prompts: {}", e));
                    }
                }
            }
            Command::InputHistory => {
                let (count, last_entry) = self.ui.get_input_history_stats();
                if count == 0 {
                    self.ui.print_info("No input history found");
                } else {
                    self.ui.print_info(&format!("Input history: {} entries", count));
                    if let Some(last) = last_entry {
                        let preview = if last.len() > 80 {
                            format!("{}...", &last[..80].replace('\n', " "))
                        } else {
                            last.replace('\n', " ")
                        };
                        self.ui.print_info(&format!("Most recent: {}", preview));
                    }
                    println!();
                    self.ui.print_info("Use up/down arrows to navigate history");
                    self.ui.print_info("Use '/input-history clear' to clear all history");
                }
            }
            Command::InputHistoryClear => {
                match self.ui.clear_input_history() {
                    Ok(()) => {
                        self.ui.print_info("Input history cleared");
                    }
                    Err(e) => {
                        self.ui.print_error(&format!("Failed to clear input history: {}", e));
                    }
                }
            }
            Command::InputHistoryStats => {
                let (count, last_entry) = self.ui.get_input_history_stats();
                self.ui.print_info("Input history statistics:");
                println!("  Total entries: {}", count);
                println!("  Max entries: 1000");
                if let Some(last) = last_entry {
                    let preview = if last.len() > 60 {
                        format!("{}...", &last[..60].replace('\n', " "))
                    } else {
                        last.replace('\n', " ")
                    };
                    println!("  Most recent: {}", preview);
                }
            }
            Command::Load(filename, variable_name_opt) => {
                match std::fs::read_to_string(&filename) {
                    Ok(content) => {
                        let variable_name = if let Some(name) = variable_name_opt {
                            name
                        } else {
                            self.generate_variable_name_from_filename(&filename)
                        };
                        
                        self.variables.insert(variable_name.clone(), content.clone());
                        
                        // Store the binding in the session for persistence
                        self.session.variable_bindings.insert(filename.clone(), variable_name.clone());
                        
                        let preview = if content.len() > 100 {
                            format!("{}...", &content[..100].replace('\n', " "))
                        } else {
                            content.replace('\n', " ")
                        };
                        
                        self.ui.print_info(&format!("Loaded '{}' as {{{{{}}}}} ({} chars): {}", 
                            filename, variable_name, content.len(), preview));
                        
                        // Update completion context with new variable
                        let _ = self.update_completion_context();
                    }
                    Err(e) => {
                        self.ui.print_error(&format!("Failed to load file '{}': {}", filename, e));
                    }
                }
            }
            Command::Variables => {
                if self.variables.is_empty() {
                    self.ui.print_info("No variables loaded");
                } else {
                    self.ui.print_info(&format!("Loaded variables ({} total):", self.variables.len()));
                    
                    // Create reverse mapping from variable name to filename
                    let mut var_to_file: HashMap<String, String> = HashMap::new();
                    for (filename, var_name) in &self.session.variable_bindings {
                        var_to_file.insert(var_name.clone(), filename.clone());
                    }
                    
                    for (name, content) in &self.variables {
                        let preview = if content.len() > 80 {
                            format!("{}...", &content[..80].replace('\n', " "))
                        } else {
                            content.replace('\n', " ")
                        };
                        
                        if let Some(filename) = var_to_file.get(name) {
                            println!("  {{{{{}}}}} (from '{}'): {} chars - {}", name, filename, content.len(), preview);
                        } else {
                            println!("  {{{{{}}}}}: {} chars - {}", name, content.len(), preview);
                        }
                    }
                    println!();
                    self.ui.print_info("Use {{VARIABLE_NAME}} in your messages to reference content");
                }
            }
            Command::VariableShow(name) => {
                if let Some(content) = self.variables.get(&name) {
                    self.ui.print_info(&format!("Variable {{{}}} ({} chars):", name, content.len()));
                    println!();
                    println!("{}", content);
                } else {
                    self.ui.print_error(&format!("Variable '{}' not found", name));
                    if !self.variables.is_empty() {
                        self.ui.print_info("Available variables:");
                        for var_name in self.variables.keys() {
                            println!("  {}", var_name);
                        }
                    }
                }
            }
            Command::VariableDelete(name) => {
                if self.variables.remove(&name).is_some() {
                    // Also remove from session bindings
                    self.session.variable_bindings.retain(|_, var_name| var_name != &name);
                    
                    self.ui.print_info(&format!("Variable '{}' deleted", name));
                    // Update completion context after deletion
                    let _ = self.update_completion_context();
                } else {
                    self.ui.print_error(&format!("Variable '{}' not found", name));
                }
            }
            Command::VariableReload(name_opt) => {
                if let Some(name) = name_opt {
                    // Reload specific variable
                    self.reload_specific_variable(&name);
                } else {
                    // Reload all variables
                    self.reload_all_variables();
                }
            }
            _ => {
                self.ui.print_info(&format!("Command not yet implemented: {:?}", command));
            }
        }
        
        Ok(true)
    }
    
    async fn handle_message(&mut self, message: String) -> Result<()> {
        // Substitute variables in the message before processing
        let processed_message = self.substitute_variables(&message);
        
        // Show substitution info if variables were replaced
        if processed_message != message {
            let var_count = self.variables.keys()
                .filter(|var_name| message.contains(&format!("{{{{{}}}}}", var_name)))
                .count();
            self.ui.print_info(&format!("Substituted {} variable(s) in message", var_count));
        }
        
        // Create cancellation token for this request
        let cancel_token = CancellationToken::new();
        self.active_request_token = Some(cancel_token.clone());
        
        // Split the receiver to avoid borrow checker issues
        let mut ctrl_c_rx = std::mem::replace(&mut self.ctrl_c_rx, tokio::sync::mpsc::unbounded_channel().1);
        
        // Try to send the processed message with retry logic
        let result = tokio::select! {
            result = self.send_message_with_retry(&processed_message, 3, cancel_token.clone()) => {
                // Restore the receiver
                self.ctrl_c_rx = ctrl_c_rx;
                result
            },
            _ = cancel_token.cancelled() => {
                // Restore the receiver
                self.ctrl_c_rx = ctrl_c_rx;
                // Request was cancelled - queue as interrupted message (original, not processed)
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
                
                // All retries failed - queue the original message for retry
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
            
            // Start spinner for this attempt
            let spinner_message = if attempt == 1 {
                "Sending message...".to_string()
            } else {
                format!("Retrying... (attempt {}/{})", attempt, max_retries)
            };
            let spinner = self.ui.start_spinner(&spinner_message);
            
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
                            // Stop spinner once we start receiving response
                            spinner.stop().await;
                            
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
                                                            if let Some(thinking_text) = content.strip_prefix("thinking:") {
                                                                if !thinking_started {
                                                                    self.ui.print_thinking_prefix(agent_number);
                                                                    thinking_started = true;
                                                                    has_thinking = true;
                                                                }
                                                                self.ui.print_thinking_chunk(thinking_text);
                                                            } else if let Some(content_text) = content.strip_prefix("content:") {
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
                                                        spinner.stop().await;
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
                                        spinner.stop().await;
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
                            
                            // Print newline after successful stream completion
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
                            } else if attempt < max_retries {
                                self.ui.print_error("Empty response received, retrying...");
                                continue;
                            } else {
                                // User message was never added to history, so nothing to remove
                                return Err(anyhow::anyhow!("Empty response on final attempt"));
                            }
                        }
                        Err(e) => {
                            // Stop spinner on error
                            spinner.stop().await;
                            
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
    
    async fn handle_auto_rename(&mut self, session_ref_opt: Option<&SessionReference>) -> Result<()> {
        // Determine which session to rename
        let (target_session, is_current_session, session_name) = if let Some(session_ref) = session_ref_opt {
            // Auto-rename specific session by reference
            let name = self.resolve_session_reference(session_ref)?;
            if let Some(session) = self.history.load_session(&name) {
                (session.clone(), false, name)
            } else {
                return Err(anyhow::anyhow!("Session '{}' not found", name));
            }
        } else {
            // Auto-rename current session - it must be saved first
            if let Some(ref current_name) = self.session.name {
                (self.session.clone(), true, current_name.clone())
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
        
        // Ensure the title is unique
        let unique_title = self.ensure_unique_session_name(&sanitized_title);
        
        // Use the session_name we already resolved
        let current_name = session_name;
        
        // Rename the session with auto-generated source
        match self.history.rename_session_with_source(&current_name, &unique_title, crate::session::NameSource::AutoGenerated) {
            Ok(true) => {
                self.ui.print_info(&format!("Session \"{}\" auto-renamed to \"{}\"", current_name, unique_title));
                
                // If we renamed the current session, update its name and show context
                if is_current_session {
                    self.session.name = Some(unique_title.clone());
                    self.session.name_source = Some(crate::session::NameSource::AutoGenerated);
                    self.ui.print_info(&format!("Current session: \"{}\" ({} messages)", 
                        unique_title, self.session.messages.len()));
                }
                
                // Update completion context and session list with new session name
                let _ = self.update_completion_context();
                self.update_session_list();
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
                    if let Some(stripped) = chunk.strip_prefix("content:") {
                        response.push_str(stripped);
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
    
    fn ensure_unique_session_name(&self, base_name: &str) -> String {
        if self.history.load_session(base_name).is_none() {
            return base_name.to_string();
        }
        
        for i in 2..=999 {
            let candidate = format!("{} ({})", base_name, i);
            if self.history.load_session(&candidate).is_none() {
                return candidate;
            }
        }
        
        // Fallback with timestamp if we somehow hit 999 duplicates
        format!("{} ({})", base_name, Utc::now().format("%H:%M:%S"))
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
        sanitized = sanitized.replace(['\n', '\r', '\t'], " ");
        
        // Collapse multiple spaces
        while sanitized.contains("  ") {
            sanitized = sanitized.replace("  ", " ");
        }
        
        sanitized.trim().to_string()
    }
    
    async fn handle_summarize(&mut self, session_ref_opt: Option<&SessionReference>) -> Result<()> {
        // Determine which session to summarize
        let (target_session, session_display) = if let Some(session_ref) = session_ref_opt {
            // Summarize specific session by reference
            let name = self.resolve_session_reference(session_ref)?;
            if let Some(session) = self.history.load_session(&name) {
                (session.clone(), format!("\"{}\"", name))
            } else {
                return Err(anyhow::anyhow!("Session '{}' not found", name));
            }
        } else {
            // Summarize current session
            (self.session.clone(), "current session".to_string())
        };
        
        // Check if session has messages
        if target_session.messages.is_empty() {
            return Err(anyhow::anyhow!("Cannot summarize empty session"));
        }
        
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
                    if let Some(stripped) = chunk.strip_prefix("content:") {
                        response.push_str(stripped);
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
    
    async fn handle_auto_rename_all(&mut self) -> Result<()> {
        // Get all sessions that are candidates for auto-renaming
        let candidates = self.history.get_sessions_for_auto_rename();
        
        if candidates.is_empty() {
            self.ui.print_info("No sessions found that need auto-renaming");
            return Ok(());
        }
        
        self.ui.print_info(&format!("Found {} sessions to auto-rename...", candidates.len()));
        
        let mut renamed_count = 0;
        let mut failed_count = 0;
        let mut skipped_count = 0;
        
        // Collect session names and data to avoid borrow checker issues
        let candidate_data: Vec<(String, ChatSession)> = candidates
            .into_iter()
            .map(|(name, session)| (name.clone(), session.clone()))
            .collect();
        
        // Process each candidate session
        for (session_name, session) in candidate_data {
            // Skip sessions with no messages
            if session.messages.is_empty() {
                self.ui.print_info(&format!("Skipping \"{}\" (no messages)", session_name));
                skipped_count += 1;
                continue;
            }
            
            // Find the first user message
            let first_user_message = session.messages.iter()
                .find(|msg| msg.message.role == "user");
            
            if first_user_message.is_none() {
                self.ui.print_info(&format!("Skipping \"{}\" (no user messages)", session_name));
                skipped_count += 1;
                continue;
            }
            
            self.ui.print_info(&format!("Generating title for \"{}\"...", session_name));
            
            // Generate title using LLM
            match self.generate_session_title(&first_user_message.unwrap().message.content).await {
                Ok(generated_title) => {
                    // Sanitize the title
                    let sanitized_title = self.sanitize_session_title(&generated_title);
                    
                    if sanitized_title.is_empty() {
                        self.ui.print_error(&format!("Generated title for \"{}\" is empty after sanitization", session_name));
                        failed_count += 1;
                        continue;
                    }
                    
                    // Ensure the title is unique
                    let unique_title = self.ensure_unique_session_name(&sanitized_title);
                    
                    // Rename the session
                    match self.history.rename_session_with_source(&session_name, &unique_title, crate::session::NameSource::AutoGenerated) {
                        Ok(true) => {
                            self.ui.print_info(&format!("\"{}\" → \"{}\"", session_name, unique_title));
                            renamed_count += 1;
                            
                            // If this is the current session, update it
                            if self.session.name.as_ref() == Some(&session_name) {
                                self.session.name = Some(unique_title.clone());
                                self.session.name_source = Some(crate::session::NameSource::AutoGenerated);
                            }
                        }
                        Ok(false) => {
                            self.ui.print_error(&format!("Session \"{}\" not found during rename", session_name));
                            failed_count += 1;
                        }
                        Err(e) => {
                            self.ui.print_error(&format!("Failed to rename \"{}\": {}", session_name, e));
                            failed_count += 1;
                        }
                    }
                }
                Err(e) => {
                    self.ui.print_error(&format!("Failed to generate title for \"{}\": {}", session_name, e));
                    failed_count += 1;
                }
            }
        }
        
        // Update completion context and session list with new session names
        let _ = self.update_completion_context();
        self.update_session_list();
        
        // Print summary
        println!();
        self.ui.print_info(&format!("Auto-rename complete: {} renamed, {} failed, {} skipped", 
            renamed_count, failed_count, skipped_count));
        
        Ok(())
    }
    
    async fn handle_prompt_auto_name(&mut self, name_opt: Option<&String>) -> Result<()> {
        // Determine which prompt to auto-name
        let (target_prompt, _prompt_name) = if let Some(name) = name_opt {
            // Auto-name specific prompt
            if let Some(prompt) = self.prompts.get_prompt(name) {
                (prompt.clone(), name.clone())
            } else {
                return Err(anyhow::anyhow!("Prompt '{}' not found", name));
            }
        } else {
            // Auto-name current system prompt
            if let Some(ref current_prompt) = self.session.system_prompt {
                // Create a temporary prompt for naming
                let temp_prompt = crate::prompts::SystemPrompt::new("temp".to_string(), current_prompt.clone());
                (temp_prompt, "current system prompt".to_string())
            } else {
                return Err(anyhow::anyhow!("No system prompt is currently set"));
            }
        };
        
        if target_prompt.content.trim().is_empty() {
            return Err(anyhow::anyhow!("Cannot auto-name empty prompt"));
        }
        
        self.ui.print_info("Generating name for prompt...");
        
        // Generate name using LLM
        let generated_name = self.generate_prompt_name(&target_prompt.content).await?;
        
        // Sanitize the name
        let sanitized_name = self.sanitize_prompt_name(&generated_name);
        
        if sanitized_name.is_empty() {
            return Err(anyhow::anyhow!("Generated name is empty after sanitization"));
        }
        
        // Ensure the name is unique
        let unique_name = self.prompts.ensure_unique_prompt_name(&sanitized_name);
        
        if name_opt.is_some() {
            // Rename existing prompt
            let old_name = name_opt.unwrap();
            match self.prompts.rename_prompt(old_name, &unique_name) {
                Ok(true) => {
                    self.ui.print_info(&format!("Prompt \"{}\" auto-renamed to \"{}\"", old_name, unique_name));
                    // Update completion context with new name
                    let _ = self.update_completion_context();
                    Ok(())
                }
                Ok(false) => {
                    Err(anyhow::anyhow!("Prompt '{}' not found", old_name))
                }
                Err(e) => {
                    Err(e)
                }
            }
        } else {
            // Save current system prompt with generated name
            match self.prompts.save_prompt(unique_name.clone(), target_prompt.content) {
                Ok(()) => {
                    self.ui.print_info(&format!("Current system prompt saved as \"{}\"", unique_name));
                    // Update completion context with new prompt
                    let _ = self.update_completion_context();
                    Ok(())
                }
                Err(e) => {
                    Err(e)
                }
            }
        }
    }
    
    async fn generate_prompt_name(&self, prompt_content: &str) -> Result<String> {
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
            .ok_or_else(|| anyhow::anyhow!("No provider available for name generation"))?;
        
        let provider = self.providers.get(provider_name)
            .ok_or_else(|| anyhow::anyhow!("Provider '{}' not available", provider_name))?;
        
        // Choose a reliable model for name generation
        let model = match provider_name {
            "anthropic" => "claude-3-5-sonnet-20241022",
            "openai" => "gpt-4o",
            "gemini" => "gemini-2.5-pro",
            _ => return Err(anyhow::anyhow!("Unknown provider: {}", provider_name)),
        };
        
        // Create system prompt for name generation
        let system_prompt = "Generate a concise, descriptive name (2-6 words) for a system prompt based on its content and purpose. The name should clearly indicate what role or task the prompt is designed for. Respond with ONLY the name, no quotes, no explanation, no additional text.";
        
        // Create messages for the request
        let messages = vec![
            crate::providers::Message {
                role: "system".to_string(),
                content: system_prompt.to_string(),
            },
            crate::providers::Message {
                role: "user".to_string(),
                content: format!("Generate a name for this system prompt:\n\n{}", prompt_content),
            },
        ];
        
        // Create chat request (non-streaming for simplicity)
        let chat_request = crate::providers::ChatRequest {
            messages,
            model: model.to_string(),
            temperature: 0.7,
            max_tokens: 30, // Short response expected
            thinking_budget: 0, // No thinking needed for name generation
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
                    if let Some(stripped) = chunk.strip_prefix("content:") {
                        response.push_str(stripped);
                    } else {
                        response.push_str(&chunk);
                    }
                }
                Err(e) => {
                    return Err(anyhow::anyhow!("Error generating name: {}", e));
                }
            }
        }
        
        if response.trim().is_empty() {
            return Err(anyhow::anyhow!("Empty response from LLM"));
        }
        
        Ok(response.trim().to_string())
    }
    
    fn sanitize_prompt_name(&self, name: &str) -> String {
        let mut sanitized = name.trim().to_string();
        
        // Remove surrounding quotes if present
        if (sanitized.starts_with('"') && sanitized.ends_with('"')) ||
           (sanitized.starts_with('\'') && sanitized.ends_with('\'')) {
            sanitized = sanitized[1..sanitized.len()-1].to_string();
        }
        
        // Remove common prefixes that LLMs might add
        let prefixes_to_remove = [
            "Name: ",
            "name: ",
            "Prompt: ",
            "prompt: ",
            "System: ",
            "system: ",
        ];
        
        for prefix in &prefixes_to_remove {
            if sanitized.starts_with(prefix) {
                sanitized = sanitized[prefix.len()..].to_string();
                break;
            }
        }
        
        // Limit length (reasonable prompt name length)
        if sanitized.len() > 60 {
            sanitized = sanitized[..60].to_string();
            // Try to break at a word boundary
            if let Some(last_space) = sanitized.rfind(' ') {
                if last_space > 20 { // Don't make it too short
                    sanitized = sanitized[..last_space].to_string();
                }
            }
        }
        
        // Replace problematic characters for prompt names
        sanitized = sanitized.replace(['\n', '\r', '\t'], " ");
        
        // Collapse multiple spaces
        while sanitized.contains("  ") {
            sanitized = sanitized.replace("  ", " ");
        }
        
        sanitized.trim().to_string()
    }
    
    fn generate_variable_name_from_filename(&self, filename: &str) -> String {
        let path = Path::new(filename);
        let stem = path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("file");
        
        // Convert to valid variable name: alphanumeric + underscore, start with letter
        let mut var_name = String::new();
        let mut chars = stem.chars();
        
        // Ensure first character is a letter
        if let Some(first_char) = chars.next() {
            if first_char.is_alphabetic() {
                var_name.push(first_char.to_ascii_lowercase());
            } else {
                var_name.push('f'); // Default prefix
                if first_char.is_alphanumeric() {
                    var_name.push(first_char);
                }
            }
        }
        
        // Process remaining characters
        for ch in chars {
            if ch.is_alphanumeric() {
                var_name.push(ch.to_ascii_lowercase());
            } else if ch == '-' || ch == ' ' || ch == '.' {
                var_name.push('_');
            }
            // Skip other characters
        }
        
        // Ensure we have a valid name
        if var_name.is_empty() {
            var_name = "file".to_string();
        }
        
        // Make unique if already exists
        let mut unique_name = var_name.clone();
        let mut counter = 1;
        while self.variables.contains_key(&unique_name) {
            counter += 1;
            unique_name = format!("{}_{}", var_name, counter);
        }
        
        unique_name
    }
    
    fn substitute_variables(&self, input: &str) -> String {
        let mut result = input.to_string();
        
        // Replace {{VARIABLE_NAME}} with variable content
        for (var_name, var_content) in &self.variables {
            let pattern = format!("{{{{{}}}}}", var_name);
            result = result.replace(&pattern, var_content);
        }
        
        result
    }
    
    fn restore_session_variables(&mut self, session: &ChatSession) {
        // Clear current variables
        self.variables.clear();
        
        // Restore variables from session bindings
        for (filename, variable_name) in &session.variable_bindings {
            match std::fs::read_to_string(filename) {
                Ok(content) => {
                    self.variables.insert(variable_name.clone(), content);
                }
                Err(_) => {
                    // File no longer exists, but we'll keep the binding in case it comes back
                    self.ui.print_info(&format!("Warning: File '{}' for variable '{}' not found", filename, variable_name));
                }
            }
        }
        
        // Update completion context with restored variables
        let _ = self.update_completion_context();
    }
    
    fn reload_specific_variable(&mut self, var_name: &str) {
        // Find the filename for this variable
        let filename = self.session.variable_bindings.iter()
            .find(|(_, v)| v == &var_name)
            .map(|(f, _)| f.clone());
        
        if let Some(filename) = filename {
            match std::fs::read_to_string(&filename) {
                Ok(content) => {
                    let old_size = self.variables.get(var_name).map(|c| c.len()).unwrap_or(0);
                    self.variables.insert(var_name.to_string(), content.clone());
                    
                    let preview = if content.len() > 100 {
                        format!("{}...", &content[..100].replace('\n', " "))
                    } else {
                        content.replace('\n', " ")
                    };
                    
                    self.ui.print_info(&format!("Reloaded variable '{{{}}}' from '{}' ({} → {} chars): {}", 
                        var_name, filename, old_size, content.len(), preview));
                }
                Err(e) => {
                    // Remove the variable since file is not accessible
                    self.variables.remove(var_name);
                    self.ui.print_error(&format!("Failed to reload variable '{}' from '{}': {}", var_name, filename, e));
                    self.ui.print_info(&format!("Variable '{}' removed from active variables", var_name));
                }
            }
        } else {
            self.ui.print_error(&format!("Variable '{}' not found or has no associated file", var_name));
            if !self.variables.is_empty() {
                self.ui.print_info("Available variables:");
                for var_name in self.variables.keys() {
                    println!("  {}", var_name);
                }
            }
        }
        
        // Update completion context
        let _ = self.update_completion_context();
    }
    
    fn reload_all_variables(&mut self) {
        if self.session.variable_bindings.is_empty() {
            self.ui.print_info("No variables to reload");
            return;
        }
        
        let mut reloaded_count = 0;
        let mut failed_count = 0;
        let mut removed_count = 0;
        
        // Collect bindings to avoid borrow checker issues
        let bindings: Vec<(String, String)> = self.session.variable_bindings.iter()
            .map(|(f, v)| (f.clone(), v.clone()))
            .collect();
        
        for (filename, var_name) in bindings {
            match std::fs::read_to_string(&filename) {
                Ok(content) => {
                    let old_size = self.variables.get(&var_name).map(|c| c.len()).unwrap_or(0);
                    self.variables.insert(var_name.clone(), content.clone());
                    
                    if old_size != content.len() {
                        self.ui.print_info(&format!("Reloaded '{{{}}}' from '{}' ({} → {} chars)", 
                            var_name, filename, old_size, content.len()));
                    }
                    reloaded_count += 1;
                }
                Err(_) => {
                    // Remove the variable since file is not accessible
                    if self.variables.remove(&var_name).is_some() {
                        removed_count += 1;
                        self.ui.print_info(&format!("Removed variable '{{{}}}' (file '{}' not found)", var_name, filename));
                    } else {
                        failed_count += 1;
                    }
                }
            }
        }
        
        self.ui.print_info(&format!("Variable reload complete: {} reloaded, {} removed, {} failed", 
            reloaded_count, removed_count, failed_count));
        
        // Update completion context
        let _ = self.update_completion_context();
    }
}
