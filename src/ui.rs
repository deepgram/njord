use anyhow::Result;
use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;
use std::io::{self, Write};

pub struct UI {
    editor: DefaultEditor,
}

impl UI {
    pub fn new() -> Result<Self> {
        let editor = DefaultEditor::new()?;
        Ok(Self { editor })
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
    
    pub fn read_input(&mut self, queued_message: Option<&str>) -> Result<Option<String>> {
        let prompt = if queued_message.is_some() {
            "\x1b[1;33m>>> (retry) \x1b[0m"
        } else {
            "\x1b[1;32m>>> \x1b[0m"
        };
        
        let initial_input = queued_message.unwrap_or("");
        
        match self.editor.readline_with_initial(prompt, (initial_input, "")) {
            Ok(line) => {
                let input = line.trim();
                if input.is_empty() {
                    Ok(None)
                } else {
                    // Add to history for arrow key navigation
                    self.editor.add_history_entry(&line)?;
                    
                    if input.starts_with("```") {
                        // Multi-line input mode
                        self.read_multiline_input(input.to_string())
                    } else {
                        Ok(Some(input.to_string()))
                    }
                }
            }
            Err(ReadlineError::Interrupted) => {
                // Ctrl-C
                Ok(Some("/quit".to_string()))
            }
            Err(ReadlineError::Eof) => {
                // Ctrl-D
                Ok(Some("/quit".to_string()))
            }
            Err(err) => Err(anyhow::anyhow!("Failed to read input: {}", err)),
        }
    }
    
    fn read_multiline_input(&mut self, first_line: String) -> Result<Option<String>> {
        let mut lines = vec![first_line];
        
        println!("\x1b[2m(Multi-line mode - end with ``` on its own line)\x1b[0m");
        
        loop {
            match self.editor.readline("\x1b[1;32m... \x1b[0m") {
                Ok(line) => {
                    let line = line.trim_end_matches('\n').trim_end_matches('\r');
                    
                    // Check for end of code block
                    if line.trim() == "```" {
                        lines.push(line.to_string());
                        break;
                    }
                    
                    lines.push(line.to_string());
                }
                Err(ReadlineError::Interrupted) | Err(ReadlineError::Eof) => {
                    break; // Exit multi-line mode on Ctrl-C or Ctrl-D
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
