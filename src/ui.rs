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
        println!("For multi-line input, start with ``` and end with ``` on its own line.");
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
                let input = input.trim();
                if input.is_empty() {
                    Ok(None)
                } else if input.starts_with("```") {
                    // Multi-line input mode
                    self.read_multiline_input(input.to_string())
                } else {
                    Ok(Some(input.to_string()))
                }
            }
            Err(e) => Err(anyhow::anyhow!("Failed to read input: {}", e)),
        }
    }
    
    fn read_multiline_input(&mut self, first_line: String) -> Result<Option<String>> {
        let mut lines = vec![first_line];
        
        println!("\x1b[2m(Multi-line mode - end with ``` on its own line)\x1b[0m");
        
        loop {
            print!("\x1b[1;32m... \x1b[0m");
            io::stdout().flush()?;
            
            let mut line = String::new();
            match io::stdin().read_line(&mut line) {
                Ok(0) => break, // EOF
                Ok(_) => {
                    let line = line.trim_end_matches('\n').trim_end_matches('\r');
                    
                    // Check for end of code block
                    if line.trim() == "```" {
                        lines.push(line.to_string());
                        break;
                    }
                    
                    lines.push(line.to_string());
                }
                Err(e) => return Err(anyhow::anyhow!("Failed to read input: {}", e)),
            }
        }
        
        // Remove the opening and closing ``` markers
        if lines.len() >= 2 && lines[0].starts_with("```") && lines.last().unwrap().trim() == "```" {
            lines.remove(0); // Remove opening ```
            lines.pop(); // Remove closing ```
        }
        
        let full_input = lines.join("\n");
        if full_input.trim().is_empty() {
            Ok(None)
        } else {
            Ok(Some(full_input))
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
