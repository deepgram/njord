use anyhow::Result;
use std::collections::HashMap;
use futures::StreamExt;
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
    global_cancel_token: CancellationToken,
}

impl Repl {
    pub async fn new(config: Config, global_cancel_token: CancellationToken) -> Result<Self> {
        let mut providers = HashMap::new();
        let mut current_provider = None;
        
        // Initialize providers based on available API keys
        // Prefer OpenAI as the default provider since it's fully implemented
        let provider_priority = ["openai", "anthropic", "gemini"];
        
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
        
        // Set current provider based on priority (OpenAI first)
        for preferred_provider in &provider_priority {
            if providers.contains_key(*preferred_provider) {
                current_provider = Some(preferred_provider.to_string());
                break;
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
        
        // Update session with current provider
        session.current_provider = current_provider.clone();
        
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
            global_cancel_token,
        })
    }
    
    pub async fn run(&mut self) -> Result<()> {
        self.ui.draw_welcome()?;
        
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
                
                // Check if global cancellation was triggered
                if self.global_cancel_token.is_cancelled() {
                    // Reset the global token for next time
                    self.global_cancel_token = CancellationToken::new();
                    // Clear any queued/interrupted messages
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
                // For now, just update the session model
                // TODO: Validate model exists for current provider
                self.session.current_model = model_name.clone();
                self.ui.print_info(&format!("Switched to model: {}", model_name));
            }
            Command::Status => {
                if let Some(provider_name) = &self.current_provider {
                    self.ui.print_info(&format!("Current provider: {}", provider_name));
                    self.ui.print_info(&format!("Current model: {}", self.session.current_model));
                    self.ui.print_info(&format!("Temperature: {}", self.session.temperature));
                } else {
                    self.ui.print_error("No provider selected");
                }
            }
            Command::Provider(provider_name) => {
                if self.providers.contains_key(&provider_name) {
                    self.current_provider = Some(provider_name.clone());
                    self.session.current_provider = Some(provider_name.clone());
                    self.ui.print_info(&format!("Switched to provider: {}", provider_name));
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
            _ => {
                self.ui.print_info(&format!("Command not yet implemented: {:?}", command));
            }
        }
        
        Ok(true)
    }
    
    async fn handle_message(&mut self, message: String) -> Result<()> {
        // Create cancellation token for this request, linked to global token
        let cancel_token = CancellationToken::new();
        let global_token = self.global_cancel_token.clone();
        self.active_request_token = Some(cancel_token.clone());
        
        // Try to send the message with retry logic
        let result = tokio::select! {
            result = self.send_message_with_retry(&message, 3, cancel_token.clone()) => result,
            _ = cancel_token.cancelled() => {
                // Request was cancelled - queue as interrupted message
                self.interrupted_message = Some(message);
                self.ui.print_info("Request interrupted. Message available for editing.");
                return Ok(());
            }
            _ = global_token.cancelled() => {
                // Global cancellation (Ctrl-C) - cancel local token and queue message
                cancel_token.cancel();
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
                if cancel_token.is_cancelled() || global_token.is_cancelled() {
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
        
        let mut user_message_added = false;
        let mut message_number = 0;
        
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
            
            // Only add to session history on first attempt
            if attempt == 1 {
                message_number = self.session.add_message(user_message.clone());
                user_message_added = true;
                self.ui.print_user_message(message_number, &message);
            }
            
            // Send to LLM provider and handle streaming response
            if let Some(provider_name) = &self.current_provider {
                if let Some(provider) = self.providers.get(provider_name) {
                    let chat_request = ChatRequest {
                        messages: self.session.messages.iter().map(|nm| nm.message.clone()).collect(),
                        model: self.session.current_model.clone(),
                        temperature: self.session.temperature,
                        stream: true,
                    };
                    
                    match provider.chat(chat_request).await {
                        Ok(mut stream) => {
                            self.ui.print_agent_prefix(message_number + 1);
                            let mut full_response = String::new();
                            let mut stream_error = false;
                            
                            loop {
                                tokio::select! {
                                    chunk_result = stream.next() => {
                                        match chunk_result {
                                            Some(chunk) => {
                                                match chunk {
                                                    Ok(content) => {
                                                        if !content.is_empty() {
                                                            self.ui.print_agent_chunk(&content);
                                                            full_response.push_str(&content);
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
                                        // Remove user message from history when cancelled
                                        if user_message_added && self.session.messages.last().map(|m| &m.message.role) == Some(&"user".to_string()) {
                                            self.session.messages.pop();
                                        }
                                        return Err(anyhow::anyhow!("Request cancelled"));
                                    }
                                }
                            }
                            
                            if stream_error {
                                if attempt < max_retries {
                                    continue; // Retry on stream error
                                } else {
                                    // Remove user message from history on final failure
                                    if user_message_added && self.session.messages.last().map(|m| &m.message.role) == Some(&"user".to_string()) {
                                        self.session.messages.pop();
                                    }
                                    return Err(anyhow::anyhow!("Stream error on final attempt"));
                                }
                            }
                            
                            self.ui.print_agent_newline();
                            
                            // Add the complete response to the session with metadata
                            if !full_response.is_empty() {
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
                                    // Remove user message from history on final failure
                                    if user_message_added && self.session.messages.last().map(|m| &m.message.role) == Some(&"user".to_string()) {
                                        self.session.messages.pop();
                                    }
                                    return Err(anyhow::anyhow!("Empty response on final attempt"));
                                }
                            }
                        }
                        Err(e) => {
                            if attempt < max_retries {
                                self.ui.print_error(&format!("API error (attempt {}/{}): {}", attempt, max_retries, e));
                                continue; // Retry on API error
                            } else {
                                // Remove user message from history on final failure
                                if user_message_added && self.session.messages.last().map(|m| &m.message.role) == Some(&"user".to_string()) {
                                    self.session.messages.pop();
                                }
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
