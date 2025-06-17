use anyhow::Result;
use std::collections::HashMap;
use futures::StreamExt;

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
}

impl Repl {
    pub async fn new(config: Config) -> Result<Self> {
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
        
        let session = if config.new_session {
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
        })
    }
    
    pub async fn run(&mut self) -> Result<()> {
        self.ui.draw_welcome()?;
        
        loop {
            if let Some(input) = self.ui.read_input()? {
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
                    self.ui.print_info(&format!("Loaded session '{}'", name));
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
                        
                        println!("{}[{}] {} {}\x1b[0m: {}", 
                            role_color,
                            numbered_message.number,
                            numbered_message.message.role.chars().next().unwrap().to_uppercase().collect::<String>() + &numbered_message.message.role[1..],
                            timestamp,
                            numbered_message.message.content
                        );
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
        let user_message = Message {
            role: "user".to_string(),
            content: message,
        };
        
        let message_number = self.session.add_message(user_message);
        self.ui.print_user_message(message_number, &self.session.messages.last().unwrap().message.content);
        
        // Send to LLM provider and handle streaming response
        if let Some(provider_name) = &self.current_provider {
            if let Some(provider) = self.providers.get(provider_name) {
                let chat_request = ChatRequest {
                    messages: self.session.messages.iter().map(|nm| nm.message.clone()).collect(),
                    model: self.session.current_model.clone(),
                    temperature: self.session.temperature,
                    stream: true, // Re-enable streaming
                };
                
                match provider.chat(chat_request).await {
                    Ok(mut stream) => {
                        self.ui.print_agent_prefix(message_number + 1);
                        let mut full_response = String::new();
                        
                        while let Some(chunk) = stream.next().await {
                            match chunk {
                                Ok(content) => {
                                    if !content.is_empty() {
                                        self.ui.print_agent_chunk(&content);
                                        full_response.push_str(&content);
                                    }
                                }
                                Err(e) => {
                                    self.ui.print_error(&format!("Stream error: {}", e));
                                    break;
                                }
                            }
                        }
                        self.ui.print_agent_newline();
                        
                        // Add the complete response to the session
                        if !full_response.is_empty() {
                            let assistant_message = Message {
                                role: "assistant".to_string(),
                                content: full_response,
                            };
                            self.session.add_message(assistant_message);
                        }
                    }
                    Err(e) => {
                        self.ui.print_error(&format!("Error calling LLM provider: {}", e));
                    }
                }
            }
        }
        
        Ok(())
    }
}
