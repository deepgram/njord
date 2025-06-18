use anyhow::Result;
use std::collections::HashMap;
use futures::StreamExt;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::{
    commands::{Command, CommandParser},
    config::Config,
    history::History,
    providers::{create_provider, LLMProvider, Message, ChatRequest},
    session::ChatSession,
    ui::UI,
};

pub struct Repl {
    config: Config,
    providers: HashMap<String, Box<dyn LLMProvider>>,
    current_provider: Option<String>,
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
        let mut current_provider = None;
        
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
        
        let mut session = if config.new_session {
            ChatSession::new(config.default_model.clone(), config.temperature)
        } else if let Some(session_name) = &config.load_session {
            history.load_session(session_name)
                .cloned()
                .unwrap_or_else(|| ChatSession::new(config.default_model.clone(), config.temperature))
        } else {
            history.current_session
                .clone()
                .unwrap_or_else(|| ChatSession::new(config.default_model.clone(), config.temperature))
        };
        
        // Restore provider from session if available and valid
        if let Some(session_provider) = &session.current_provider {
            if providers.contains_key(session_provider) {
                current_provider = Some(session_provider.clone());
            }
        }
        
        // If no provider is set, choose the first available one as default
        if current_provider.is_none() && !providers.is_empty() {
            current_provider = providers.keys().next().cloned();
        }
        
        // Update session with current provider
        session.current_provider = current_provider.clone();
        
        // Ensure we have a valid model for the current provider
        if let Some(provider_name) = &current_provider {
            if let Some(provider) = providers.get(provider_name) {
                let available_models = provider.get_models();
                if !available_models.contains(&session.current_model) && !available_models.is_empty() {
                    session.current_model = available_models[0].clone();
                }
            }
        }
        
        // If loading a new session or no session exists, use config temperature
        // Otherwise, keep the session's stored temperature
        if config.new_session || config.load_session.is_none() && history.current_session.is_none() {
            session.temperature = config.temperature;
        }
        
        let command_parser = CommandParser::new()?;
        let ui = UI::new()?;
        
        Ok(Self {
            config,
            providers,
            current_provider,
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
    
    pub async fn run(&mut self) -> Result<()> {
        self.ui.draw_welcome()?;
        
        // Display current session status at startup
        self.display_startup_status();
        
        loop {
            // Determine what message to show in prompt
            let prompt_message = if let Some(interrupted) = &self.interrupted_message {
                Some((interrupted.as_str(), "interrupted"))
            } else if let Some(queued) = &self.queued_message {
                Some((queued.as_str(), "retry"))
            } else {
                None
            };
            
            if let Some(input) = self.ui.read_input(prompt_message)? {
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
                
                // Save history after each interaction
                self.history.set_current_session(self.session.clone());
                if let Err(e) = self.history.save() {
                    self.ui.print_error(&format!("Failed to save history: {}", e));
                }
            }
        }
        
        Ok(())
    }
    
    fn display_startup_status(&self) {
        if let Some(provider_name) = &self.current_provider {
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
    
    fn get_temperature_display(&self) -> String {
        if let Some(provider_name) = &self.current_provider {
            if let Some(provider) = self.providers.get(provider_name) {
                let supports_temp = match provider_name.as_str() {
                    "openai" => {
                        if let Some(openai_provider) = provider.as_any().downcast_ref::<crate::providers::openai::OpenAIProvider>() {
                            openai_provider.supports_temperature(&self.session.current_model)
                        } else { true }
                    }
                    "anthropic" => true, // All Anthropic models support temperature
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
        if let Some(provider_name) = &self.current_provider {
            if let Some(provider) = self.providers.get(provider_name) {
                let supports_thinking = match provider_name.as_str() {
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
    
    async fn handle_command(&mut self, command: Command) -> Result<bool> {
        match command {
            Command::Quit => return Ok(false),
            Command::Help => {
                self.ui.print_info("Available commands:");
                println!("  /model MODEL - Switch to a different model");
                println!("  /models - List available models");
                println!("  /provider PROVIDER - Switch provider (openai, anthropic, gemini)");
                println!("  /status - Show current provider and model");
                println!("  /chat new - Start a new chat session");
                println!("  /chat save NAME - Save current session with given name");
                println!("  /chat load NAME - Load a previously saved session");
                println!("  /chat list - List all saved sessions");
                println!("  /chat delete NAME - Delete a saved session");
                println!("  /undo [N] - Remove last N responses (default 1)");
                println!("  /goto N - Jump back to message N");
                println!("  /history - Show conversation history");
                println!("  /system [PROMPT] - Set system prompt (empty to view, 'clear' to remove)");
                println!("  /temp TEMPERATURE - Set temperature (0.0-2.0)");
                println!("  /thinking on|off - Enable/disable thinking for supported models");
                println!("  /quit - Exit Njord");
                println!();
                println!("Input tips:");
                println!("  Start with ``` for multi-line input (end with ``` on its own line)");
                println!("  Use this for code, long prompts, or formatted text");
            }
            Command::Models => {
                if let Some(provider_name) = &self.current_provider {
                    if let Some(provider) = self.providers.get(provider_name) {
                        self.ui.print_info(&format!("Available models for {}:", provider_name));
                        for model in provider.get_models() {
                            println!("  {}", model);
                        }
                    }
                }
            }
            Command::ChatNew => {
                self.session = ChatSession::new(self.config.default_model.clone(), self.config.temperature);
                self.session.current_provider = self.current_provider.clone();
                self.ui.print_info("Started new chat session");
            }
            Command::Undo(count) => {
                let count = count.unwrap_or(1);
                self.session.undo(count)?;
                self.ui.print_info(&format!("Removed last {} message(s)", count));
            }
            Command::Model(model_name) => {
                // Validate model exists for current provider
                if let Some(provider_name) = &self.current_provider {
                    if let Some(provider) = self.providers.get(provider_name) {
                        let available_models = provider.get_models();
                        if available_models.contains(&model_name) {
                            self.session.current_model = model_name.clone();
                            self.ui.print_info(&format!("Switched to model: {}", model_name));
                        } else {
                            self.ui.print_error(&format!("Model '{}' not available for provider '{}'. Available models: {}", 
                                model_name, provider_name, available_models.join(", ")));
                        }
                    } else {
                        self.ui.print_error("No provider available");
                    }
                } else {
                    self.ui.print_error("No provider selected");
                }
            }
            Command::Status => {
                if let Some(provider_name) = &self.current_provider {
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
                } else {
                    self.ui.print_error("No provider selected");
                }
            }
            Command::Provider(provider_name) => {
                if self.providers.contains_key(&provider_name) {
                    self.current_provider = Some(provider_name.clone());
                    self.session.current_provider = Some(provider_name.clone());
                    
                    // Switch to a default model for this provider if current model isn't available
                    if let Some(provider) = self.providers.get(&provider_name) {
                        let available_models = provider.get_models();
                        if !available_models.contains(&self.session.current_model) && !available_models.is_empty() {
                            let old_model = self.session.current_model.clone();
                            self.session.current_model = available_models[0].clone();
                            self.ui.print_info(&format!("Switched to provider: {} (model changed from {} to {})", 
                                provider_name, old_model, self.session.current_model));
                        } else {
                            self.ui.print_info(&format!("Switched to provider: {}", provider_name));
                        }
                    } else {
                        self.ui.print_info(&format!("Switched to provider: {}", provider_name));
                    }
                } else {
                    let available_providers: Vec<String> = self.providers.keys().cloned().collect();
                    self.ui.print_error(&format!("Provider '{}' not available. Available providers: {}", 
                        provider_name, 
                        available_providers.join(", ")));
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
                        }
                        Err(e) => {
                            self.ui.print_error(&format!("Failed to save session: {}", e));
                        }
                    }
                }
            }
            Command::ChatLoad(name) => {
                if let Some(session) = self.history.load_session(&name).cloned() {
                    self.session = session;
                    // Restore provider from session if available and valid
                    if let Some(session_provider) = &self.session.current_provider {
                        if self.providers.contains_key(session_provider) {
                            self.current_provider = Some(session_provider.clone());
                            self.ui.print_info(&format!("Loaded session '{}' (provider: {})", name, session_provider));
                        } else {
                            self.ui.print_info(&format!("Loaded session '{}' (provider '{}' not available)", name, session_provider));
                        }
                    } else {
                        self.ui.print_info(&format!("Loaded session '{}'", name));
                    }
                    self.ui.print_info(&format!("Session has {} messages", self.session.messages.len()));
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
            Command::ChatDelete(name) => {
                match self.history.delete_session(&name) {
                    Ok(true) => {
                        self.ui.print_info(&format!("Session '{}' deleted", name));
                    }
                    Ok(false) => {
                        self.ui.print_error(&format!("Session '{}' not found", name));
                    }
                    Err(e) => {
                        self.ui.print_error(&format!("Failed to delete session: {}", e));
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
                    
                    for numbered_message in &self.session.messages {
                        let timestamp = numbered_message.timestamp.format("%H:%M:%S");
                        let role_color = if numbered_message.message.role == "user" {
                            "\x1b[1;34m" // Blue for user
                        } else {
                            "\x1b[1;35m" // Magenta for assistant
                        };
                        
                        let mut header = format!("{}[{}] {} {}", 
                            role_color,
                            numbered_message.number,
                            numbered_message.message.role.chars().next().unwrap().to_uppercase().collect::<String>() + &numbered_message.message.role[1..],
                            timestamp
                        );
                        
                        // Add provider/model info for assistant messages
                        if numbered_message.message.role == "assistant" {
                            if let (Some(provider), Some(model)) = (&numbered_message.provider, &numbered_message.model) {
                                header.push_str(&format!(" ({}:{})", provider, model));
                            } else if let Some(provider) = &numbered_message.provider {
                                header.push_str(&format!(" ({})", provider));
                            }
                        }
                        
                        println!("{}\x1b[0m: {}", header, numbered_message.message.content);
                        println!();
                    }
                }
            }
            Command::Goto(message_number) => {
                match self.session.goto(message_number) {
                    Ok(()) => {
                        self.ui.print_info(&format!("Jumped to message {}, removed {} later messages", 
                            message_number, 
                            self.session.messages.len().saturating_sub(message_number)));
                    }
                    Err(e) => {
                        self.ui.print_error(&e.to_string());
                    }
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
            Command::Thinking(enable) => {
                self.session.thinking_enabled = enable;
                self.ui.print_info(&format!("Thinking {}", if enable { "enabled" } else { "disabled" }));
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
        let message_number = self.session.messages.len() + 1;
        self.ui.print_user_message(message_number, &message);
        
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
            if let Some(provider_name) = &self.current_provider {
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
                                                                    self.ui.print_thinking_prefix(message_number + 1);
                                                                    thinking_started = true;
                                                                    has_thinking = true;
                                                                }
                                                                self.ui.print_thinking_chunk(thinking_text);
                                                            } else if content.starts_with("content:") {
                                                                let content_text = &content[8..]; // Remove "content:" prefix
                                                                if has_thinking && !has_content {
                                                                    self.ui.print_thinking_end();
                                                                    self.ui.print_agent_prefix(message_number + 1);
                                                                    has_content = true;
                                                                } else if !has_content {
                                                                    self.ui.print_agent_prefix(message_number + 1);
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
                                                                    self.ui.print_agent_prefix(message_number + 1);
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
                                // Now that we have a successful response, add both user and assistant messages
                                self.session.add_message(user_message.clone());
                                let assistant_message = Message {
                                    role: "assistant".to_string(),
                                    content: full_response,
                                };
                                self.session.add_message_with_metadata(
                                    assistant_message,
                                    self.current_provider.clone(),
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
}
