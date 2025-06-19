use anyhow::Result;
use rustyline::error::ReadlineError;
use rustyline::{Editor, Helper, DefaultHistory};
use rustyline::completion::{Completer, Pair};
use rustyline::hint::Hinter;
use rustyline::highlight::Highlighter;
use rustyline::validate::Validator;
use std::io::{self, Write};

pub struct UI {
    editor: Editor<NjordCompleter, DefaultHistory>,
}

#[derive(Clone)]
pub struct CompletionContext {
    pub available_models: Vec<String>,
    pub session_names: Vec<String>,
    pub providers: Vec<String>,
}

impl CompletionContext {
    pub fn new() -> Self {
        Self {
            available_models: Vec::new(),
            session_names: Vec::new(),
            providers: Vec::new(),
        }
    }
}

pub struct NjordCompleter {
    context: CompletionContext,
}

impl NjordCompleter {
    pub fn new(context: CompletionContext) -> Self {
        Self { context }
    }
    
    pub fn update_context(&mut self, context: CompletionContext) {
        self.context = context;
    }
    
    fn complete_command(&self, line: &str, pos: usize) -> Vec<Pair> {
        let input = &line[..pos];
        
        // Basic commands
        let commands = vec![
            "/help", "/models", "/model", "/status", "/quit", "/clear", "/history",
            "/chat", "/undo", "/goto", "/search", "/system", "/temp", "/max-tokens",
            "/thinking-budget", "/thinking", "/retry", "/stats", "/tokens", "/export",
            "/block", "/copy", "/save", "/exec", "/edit"
        ];
        
        if input.starts_with('/') && !input.contains(' ') {
            // Complete basic command
            let matches: Vec<_> = commands.iter()
                .filter(|cmd| cmd.starts_with(input))
                .map(|cmd| {
                    if matches!(cmd, &"/chat" | &"/model" | &"/system" | &"/temp" | 
                                    &"/max-tokens" | &"/thinking-budget" | &"/thinking" |
                                    &"/undo" | &"/goto" | &"/search" | &"/export" |
                                    &"/block" | &"/copy" | &"/save" | &"/exec" | &"/edit") {
                        Pair {
                            display: cmd.to_string(),
                            replacement: format!("{} ", cmd),
                        }
                    } else {
                        Pair {
                            display: cmd.to_string(),
                            replacement: cmd.to_string(),
                        }
                    }
                })
                .collect();
            return matches;
        }
        
        // Handle specific command completions
        if input.starts_with("/chat ") {
            return self.complete_chat_command(input);
        } else if input.starts_with("/model ") {
            return self.complete_model_command(input);
        } else if input.starts_with("/thinking ") {
            return self.complete_thinking_command(input);
        } else if input.starts_with("/export ") {
            return self.complete_export_command(input);
        }
        
        Vec::new()
    }
    
    fn complete_chat_command(&self, input: &str) -> Vec<Pair> {
        let parts: Vec<&str> = input.split_whitespace().collect();
        
        if parts.len() == 1 || (parts.len() == 2 && !input.ends_with(' ')) {
            // Complete chat subcommand
            let subcommands = vec![
                "new", "save", "load", "list", "delete", "continue", "recent", "fork", "merge"
            ];
            
            let prefix = if parts.len() == 2 { parts[1] } else { "" };
            
            return subcommands.iter()
                .filter(|cmd| cmd.starts_with(prefix))
                .map(|cmd| {
                    if matches!(cmd, &"save" | &"load" | &"delete" | &"continue" | &"fork" | &"merge") {
                        Pair {
                            display: cmd.to_string(),
                            replacement: format!("/chat {} ", cmd),
                        }
                    } else {
                        Pair {
                            display: cmd.to_string(),
                            replacement: format!("/chat {}", cmd),
                        }
                    }
                })
                .collect();
        } else if parts.len() >= 2 {
            // Complete session names for commands that need them
            let subcommand = parts[1];
            if matches!(subcommand, "load" | "delete" | "continue" | "merge") {
                let prefix = if parts.len() >= 3 { parts[2] } else { "" };
                
                return self.context.session_names.iter()
                    .filter(|name| name.starts_with(prefix))
                    .map(|name| Pair {
                        display: name.clone(),
                        replacement: format!("/chat {} {}", subcommand, name),
                    })
                    .collect();
            }
        }
        
        Vec::new()
    }
    
    fn complete_model_command(&self, input: &str) -> Vec<Pair> {
        let parts: Vec<&str> = input.split_whitespace().collect();
        let prefix = if parts.len() >= 2 { parts[1] } else { "" };
        
        self.context.available_models.iter()
            .filter(|model| model.starts_with(prefix))
            .map(|model| Pair {
                display: model.clone(),
                replacement: format!("/model {}", model),
            })
            .collect()
    }
    
    fn complete_thinking_command(&self, input: &str) -> Vec<Pair> {
        let parts: Vec<&str> = input.split_whitespace().collect();
        let prefix = if parts.len() >= 2 { parts[1] } else { "" };
        
        vec!["on", "off"]
            .iter()
            .filter(|option| option.starts_with(prefix))
            .map(|option| Pair {
                display: option.to_string(),
                replacement: format!("/thinking {}", option),
            })
            .collect()
    }
    
    fn complete_export_command(&self, input: &str) -> Vec<Pair> {
        let parts: Vec<&str> = input.split_whitespace().collect();
        let prefix = if parts.len() >= 2 { parts[1] } else { "" };
        
        vec!["markdown", "json", "txt"]
            .iter()
            .filter(|format| format.starts_with(prefix))
            .map(|format| Pair {
                display: format.to_string(),
                replacement: format!("/export {}", format),
            })
            .collect()
    }
}

impl Completer for NjordCompleter {
    type Candidate = Pair;
    
    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &rustyline::Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Self::Candidate>)> {
        let completions = self.complete_command(line, pos);
        Ok((0, completions))
    }
}

impl Hinter for NjordCompleter {
    type Hint = String;
    
    fn hint(&self, _line: &str, _pos: usize, _ctx: &rustyline::Context<'_>) -> Option<Self::Hint> {
        None
    }
}

impl Highlighter for NjordCompleter {}

impl Validator for NjordCompleter {}

impl Helper for NjordCompleter {}

impl UI {
    pub fn new() -> Result<Self> {
        let mut editor = Editor::new()?;
        let completer = NjordCompleter::new(CompletionContext::new());
        editor.set_helper(Some(completer));
        Ok(Self { editor })
    }
    
    pub fn update_completion_context(&mut self, context: CompletionContext) -> Result<()> {
        let completer = NjordCompleter::new(context);
        self.editor.set_helper(Some(completer));
        Ok(())
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
    
    pub fn read_input(&mut self, prompt_info: Option<(&str, &str)>) -> Result<Option<String>> {
        let (prompt, initial_input) = if let Some((message, status)) = prompt_info {
            let color = match status {
                "retry" => "\x1b[1;33m", // Yellow for retry
                "interrupted" => "\x1b[1;31m", // Red for interrupted
                _ => "\x1b[1;32m", // Green default
            };
            (format!("{}>>> ({}) \x1b[0m", color, status), message)
        } else {
            ("\x1b[1;32m>>> \x1b[0m".to_string(), "")
        };
        
        match self.editor.readline_with_initial(&prompt, (initial_input, "")) {
            Ok(line) => {
                let input = line.trim();
                if input.is_empty() && initial_input.is_empty() {
                    Ok(None)
                } else if input.is_empty() && !initial_input.is_empty() {
                    // User pressed Enter on pre-filled input without changes
                    self.editor.add_history_entry(initial_input)?;
                    Ok(Some(initial_input.to_string()))
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
                // Ctrl-C - return special signal to clear input
                Ok(Some("__CTRL_C__".to_string()))
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
    
    pub fn print_thinking_prefix(&self, number: usize) {
        print!("\x1b[1;35mAgent {} (thinking)\x1b[0m: ", number);
        io::stdout().flush().unwrap();
    }
    
    pub fn print_thinking_chunk(&self, chunk: &str) {
        print!("\x1b[2;3m{}\x1b[0m", chunk); // Dimmed and italic text for thinking
        io::stdout().flush().unwrap();
    }
    
    pub fn print_thinking_end(&self) {
        println!();
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
