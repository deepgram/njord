use anyhow::Result;
use std::io::{self, Write};

pub struct UI;

impl UI {
    pub fn new() -> Result<Self> {
        Ok(Self)
    }
    
    pub fn draw_welcome(&mut self) -> Result<()> {
        println!("\x1b[1;36mNjord\x1b[0m - Interactive LLM REPL");
        println!();
        println!("Named after the Norse god of the sea and sailors,");
        println!("Njord guides you through the vast ocean of AI conversations.");
        println!();
        println!("Type your message or use slash commands:");
        println!("  /help - Show all commands");
        println!("  /models - List available models");
        println!("  /quit - Exit Njord");
        println!();
        
        Ok(())
    }
    
    pub fn read_input(&mut self) -> Result<Option<String>> {
        print!("\x1b[1;32m>>> \x1b[0m");
        io::stdout().flush()?;
        
        let mut input = String::new();
        match io::stdin().read_line(&mut input) {
            Ok(0) => Ok(Some("/quit".to_string())), // EOF
            Ok(_) => {
                let input = input.trim().to_string();
                if input.is_empty() {
                    Ok(None)
                } else {
                    Ok(Some(input))
                }
            }
            Err(e) => Err(anyhow::anyhow!("Failed to read input: {}", e)),
        }
    }
    
    pub fn print_user_message(&self, number: usize, message: &str) {
        println!("\x1b[1;34mUser {}\x1b[0m: {}", number, message);
    }
    
    pub fn print_agent_prefix(&self, number: usize) {
        print!("\x1b[1;35mAgent {}\x1b[0m: ", number);
        io::stdout().flush().unwrap();
    }
    
    pub fn print_agent_chunk(&self, chunk: &str) {
        print!("{}", chunk);
        io::stdout().flush().unwrap();
    }
    
    pub fn print_agent_newline(&self) {
        println!();
    }
    
    pub fn print_error(&self, error: &str) {
        eprintln!("\x1b[1;31mError\x1b[0m: {}", error);
    }
    
    pub fn print_info(&self, info: &str) {
        println!("\x1b[1;33mInfo\x1b[0m: {}", info);
    }
}
