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
        for (provider_name, api_key) in &config.api_keys {
            match create_provider(provider_name, api_key) {
                Ok(provider) => {
                    providers.insert(provider_name.clone(), provider);
                    if current_provider.is_none() {
                        current_provider = Some(provider_name.clone());
                    }
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
                if let Some(command) = self.command_parser.parse(&input) {
                    match self.handle_command(command).await {
                        Ok(should_continue) => {
                            if !should_continue {
                                break;
                            }
                        }
                        Err(e) => {
                            eprintln!("Error: {}", e);
                        }
                    }
                } else {
                    // Handle regular chat message
                    self.handle_message(input).await?;
                }
                
                // Save history after each interaction
                self.history.set_current_session(self.session.clone());
                self.history.save()?;
            }
        }
        
        Ok(())
    }
    
    async fn handle_command(&mut self, command: Command) -> Result<bool> {
        match command {
            Command::Quit => return Ok(false),
            Command::Help => {
                println!("Available commands:");
                println!("  /model MODEL - Switch to a different model");
                println!("  /models - List available models");
                println!("  /chat new - Start a new chat session");
                println!("  /chat save NAME - Save current session");
                println!("  /chat load NAME - Load a saved session");
                println!("  /chat list - List saved sessions");
                println!("  /undo [N] - Remove last N responses (default 1)");
                println!("  /goto N - Jump back to message N");
                println!("  /history - Show conversation history");
                println!("  /quit - Exit Njord");
            }
            Command::Models => {
                if let Some(provider_name) = &self.current_provider {
                    if let Some(provider) = self.providers.get(provider_name) {
                        println!("Available models for {}:", provider_name);
                        for model in provider.get_models() {
                            println!("  {}", model);
                        }
                    }
                }
            }
            Command::ChatNew => {
                self.session = ChatSession::new(self.config.default_model.clone(), self.config.temperature);
                println!("Started new chat session");
            }
            Command::Undo(count) => {
                let count = count.unwrap_or(1);
                self.session.undo(count)?;
                println!("Removed last {} message(s)", count);
            }
            _ => {
                println!("Command not yet implemented: {:?}", command);
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
        println!("User {}: {}", message_number, self.session.messages.last().unwrap().message.content);
        
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
                        print!("Agent {}: ", message_number + 1);
                        let mut full_response = String::new();
                        
                        while let Some(chunk) = stream.next().await {
                            match chunk {
                                Ok(content) => {
                                    if !content.is_empty() {
                                        print!("{}", content);
                                        full_response.push_str(&content);
                                        // Flush stdout to show streaming effect
                                        use std::io::{self, Write};
                                        io::stdout().flush().unwrap();
                                    }
                                }
                                Err(e) => {
                                    eprintln!("\nError in stream: {}", e);
                                    break;
                                }
                            }
                        }
                        println!(); // New line after response
                        
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
                        eprintln!("Error calling LLM provider: {}", e);
                    }
                }
            }
        }
        
        Ok(())
    }
}
