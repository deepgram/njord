use anyhow::Result;
use rustyline::error::ReadlineError;
use rustyline::{Editor, Helper};
use rustyline::history::DefaultHistory;
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
}

impl CompletionContext {
    pub fn new() -> Self {
        Self {
            available_models: Vec::new(),
            session_names: Vec::new(),
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
    
    
    fn complete_command(&self, line: &str, pos: usize) -> Vec<Pair> {
        let start_pos = self.find_completion_start(line, pos);
        let current_word = &line[start_pos..pos];
        
        // Basic commands
        let commands = vec![
            "/help", "/models", "/model", "/status", "/quit", "/clear", "/history",
            "/chat", "/undo", "/goto", "/search", "/system", "/temp", "/max-tokens",
            "/thinking-budget", "/thinking", "/retry", "/stats", "/tokens", "/export",
            "/block", "/copy", "/save", "/exec", "/edit"
        ];
        
        if line[..pos].starts_with('/') && !line[..pos].contains(' ') {
            // Complete basic command
            let matches: Vec<_> = commands.iter()
                .filter(|cmd| cmd.starts_with(current_word))
                .map(|cmd| {
                    Pair {
                        display: cmd.to_string(),
                        replacement: cmd.to_string(),
                    }
                })
                .collect();
            return matches;
        }
        
        // Handle specific command completions
        if line[..pos].starts_with("/chat ") {
            return self.complete_chat_command(line, pos);
        } else if line[..pos].starts_with("/model ") {
            return self.complete_model_command(line, pos);
        } else if line[..pos].starts_with("/thinking ") {
            return self.complete_thinking_command(line, pos);
        } else if line[..pos].starts_with("/export ") {
            return self.complete_export_command(line, pos);
        }
        
        Vec::new()
    }
    
    fn complete_chat_command(&self, line: &str, pos: usize) -> Vec<Pair> {
        let start_pos = self.find_completion_start(line, pos);
        let current_word = &line[start_pos..pos];
        let input = &line[..pos];
        let parts: Vec<&str> = input.split_whitespace().collect();
        
        if parts.len() == 1 || (parts.len() == 2 && !input.ends_with(' ')) {
            // Complete chat subcommand
            let subcommands = vec![
                "new", "save", "load", "list", "delete", "continue", "recent", "fork", "merge"
            ];
            
            return subcommands.iter()
                .filter(|cmd| cmd.starts_with(current_word))
                .map(|cmd| Pair {
                    display: cmd.to_string(),
                    replacement: cmd.to_string(),
                })
                .collect();
        } else if parts.len() >= 2 {
            // Complete session names for commands that need them
            let subcommand = parts[1];
            if matches!(subcommand, "load" | "delete" | "continue" | "merge") {
                return self.context.session_names.iter()
                    .filter(|name| name.starts_with(current_word))
                    .map(|name| Pair {
                        display: name.clone(),
                        replacement: name.clone(),
                    })
                    .collect();
            }
        }
        
        Vec::new()
    }
    
    fn complete_model_command(&self, line: &str, pos: usize) -> Vec<Pair> {
        let start_pos = self.find_completion_start(line, pos);
        let current_word = &line[start_pos..pos];
        
        // For /model command, we want to complete the model name after "/model "
        self.context.available_models.iter()
            .filter(|model| model.starts_with(current_word))
            .map(|model| Pair {
                display: model.clone(),
                replacement: model.clone(),
            })
            .collect()
    }
    
    fn complete_thinking_command(&self, line: &str, pos: usize) -> Vec<Pair> {
        let start_pos = self.find_completion_start(line, pos);
        let current_word = &line[start_pos..pos];
        
        vec!["on", "off"]
            .iter()
            .filter(|option| option.starts_with(current_word))
            .map(|option| Pair {
                display: option.to_string(),
                replacement: option.to_string(),
            })
            .collect()
    }
    
    fn complete_export_command(&self, line: &str, pos: usize) -> Vec<Pair> {
        let start_pos = self.find_completion_start(line, pos);
        let current_word = &line[start_pos..pos];
        
        vec!["markdown", "json", "txt"]
            .iter()
            .filter(|format| format.starts_with(current_word))
            .map(|format| Pair {
                display: format.to_string(),
                replacement: format.to_string(),
            })
            .collect()
    }
    
    fn find_completion_start(&self, line: &str, pos: usize) -> usize {
        let line_up_to_pos = &line[..pos];
        
        // For commands, find the start of the current word
        if let Some(last_space) = line_up_to_pos.rfind(' ') {
            last_space + 1
        } else if line_up_to_pos.starts_with('/') {
            0
        } else {
            pos
        }
    }
    
    fn find_longest_common_prefix(&self, completions: &[Pair], current_word: &str) -> String {
        if completions.is_empty() {
            return current_word.to_string();
        }
        
        // Extract the actual completion text from each candidate
        let completion_texts: Vec<&str> = completions.iter()
            .map(|pair| pair.replacement.as_str())
            .collect();
        
        // Start with the first completion
        let mut prefix = completion_texts[0].to_string();
        
        // Find common prefix with all other completions
        for &completion in &completion_texts[1..] {
            let mut common_len = 0;
            let prefix_chars: Vec<char> = prefix.chars().collect();
            let completion_chars: Vec<char> = completion.chars().collect();
            
            for (i, (&p_char, &c_char)) in prefix_chars.iter().zip(completion_chars.iter()).enumerate() {
                if p_char == c_char {
                    common_len = i + 1;
                } else {
                    break;
                }
            }
            
            if common_len < prefix_chars.len() {
                prefix = prefix_chars[..common_len].iter().collect();
            }
        }
        
        // Ensure the prefix at least includes the current word
        if prefix.len() < current_word.len() || !prefix.starts_with(current_word) {
            current_word.to_string()
        } else {
            prefix
        }
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
        
        if completions.is_empty() {
            return Ok((pos, vec![]));
        }
        
        // Find the start position of the word being completed
        let start_pos = self.find_completion_start(line, pos);
        let current_word = &line[start_pos..pos];
        
        // Find the longest common prefix among all completions
        let longest_prefix = self.find_longest_common_prefix(&completions, current_word);
        
        // Only auto-complete if there's a unique match or a clear common prefix extension
        if completions.len() == 1 && longest_prefix.len() > current_word.len() {
            // Single match - auto-complete it
            let extension = &longest_prefix[current_word.len()..];
            Ok((pos, vec![Pair {
                display: longest_prefix.clone(),
                replacement: extension.to_string(),
            }]))
        } else if completions.len() > 1 && longest_prefix.len() > current_word.len() {
            // Multiple matches with common prefix - extend to common prefix only
            let extension = &longest_prefix[current_word.len()..];
            Ok((pos, vec![Pair {
                display: longest_prefix.clone(),
                replacement: extension.to_string(),
            }]))
        } else {
            // No auto-completion - return empty to prevent cycling
            Ok((pos, vec![]))
        }
    }
}

impl Hinter for NjordCompleter {
    type Hint = String;
    
    fn hint(&self, line: &str, pos: usize, _ctx: &rustyline::Context<'_>) -> Option<Self::Hint> {
        let completions = self.complete_command(line, pos);
        
        if completions.len() > 1 {
            // Multiple completions available - show them as a hint
            let completion_names: Vec<String> = completions.iter()
                .map(|pair| pair.display.clone())
                .collect();
            
            // Limit to first 5 completions to avoid overwhelming the display
            let display_completions = if completion_names.len() > 5 {
                let mut limited = completion_names[..5].to_vec();
                limited.push(format!("... ({} more)", completion_names.len() - 5));
                limited
            } else {
                completion_names
            };
            
            Some(format!(" [{}]", display_completions.join(" ")))
        } else {
            None
        }
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
        // Check if this chunk contains code block markers and style them
        if chunk.contains("```") {
            let styled_chunk = self.style_code_blocks(chunk);
            print!("{}", styled_chunk);
        } else {
            print!("{}", chunk);
        }
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
    
    fn style_code_blocks(&self, text: &str) -> String {
        let mut result = String::new();
        let mut in_code_block = false;
        let mut current_language = None;
        
        for line in text.lines() {
            if line.starts_with("```") {
                if in_code_block {
                    // End of code block
                    result.push_str("\x1b[0;36m```\x1b[0m\n"); // Cyan closing fence
                    in_code_block = false;
                    current_language = None;
                } else {
                    // Start of code block
                    let language = if line.len() > 3 {
                        let lang = line[3..].trim();
                        if !lang.is_empty() {
                            Some(lang.to_string())
                        } else {
                            None
                        }
                    } else {
                        None
                    };
                    
                    if let Some(ref lang) = language {
                        result.push_str(&format!("\x1b[0;36m```{}\x1b[0m\n", lang)); // Cyan opening fence with language
                    } else {
                        result.push_str("\x1b[0;36m```\x1b[0m\n"); // Cyan opening fence
                    }
                    in_code_block = true;
                    current_language = language;
                }
            } else if in_code_block {
                // Inside code block - apply syntax-aware styling
                let styled_line = self.style_code_line(line, current_language.as_deref());
                result.push_str(&format!("\x1b[48;5;236m{}\x1b[0m\n", styled_line)); // Dark gray background
            } else {
                // Regular text
                result.push_str(line);
                result.push('\n');
            }
        }
        
        // Remove the last newline if the original didn't end with one
        if !text.ends_with('\n') && result.ends_with('\n') {
            result.pop();
        }
        
        result
    }
    
    fn style_code_line(&self, line: &str, language: Option<&str>) -> String {
        match language {
            Some("rust") => self.style_rust_line(line),
            Some("python") | Some("py") => self.style_python_line(line),
            Some("javascript") | Some("js") => self.style_javascript_line(line),
            Some("bash") | Some("sh") => self.style_bash_line(line),
            Some("json") => self.style_json_line(line),
            _ => format!("\x1b[0;37m{}\x1b[0m", line), // Default white text
        }
    }
    
    fn style_rust_line(&self, line: &str) -> String {
        let mut result = String::new();
        let trimmed = line.trim_start();
        let indent = &line[..line.len() - trimmed.len()];
        
        result.push_str(indent);
        
        if trimmed.starts_with("//") {
            // Comments in green
            result.push_str(&format!("\x1b[0;32m{}\x1b[0m", trimmed));
        } else if trimmed.starts_with("fn ") || trimmed.contains(" fn ") {
            // Function definitions in yellow
            result.push_str(&format!("\x1b[0;33m{}\x1b[0m", trimmed));
        } else if trimmed.starts_with("let ") || trimmed.starts_with("const ") || trimmed.starts_with("static ") {
            // Variable declarations in cyan
            result.push_str(&format!("\x1b[0;36m{}\x1b[0m", trimmed));
        } else {
            result.push_str(&format!("\x1b[0;37m{}\x1b[0m", trimmed));
        }
        
        result
    }
    
    fn style_python_line(&self, line: &str) -> String {
        let mut result = String::new();
        let trimmed = line.trim_start();
        let indent = &line[..line.len() - trimmed.len()];
        
        result.push_str(indent);
        
        if trimmed.starts_with("#") {
            // Comments in green
            result.push_str(&format!("\x1b[0;32m{}\x1b[0m", trimmed));
        } else if trimmed.starts_with("def ") || trimmed.starts_with("class ") {
            // Function/class definitions in yellow
            result.push_str(&format!("\x1b[0;33m{}\x1b[0m", trimmed));
        } else if trimmed.starts_with("import ") || trimmed.starts_with("from ") {
            // Imports in magenta
            result.push_str(&format!("\x1b[0;35m{}\x1b[0m", trimmed));
        } else {
            result.push_str(&format!("\x1b[0;37m{}\x1b[0m", trimmed));
        }
        
        result
    }
    
    fn style_javascript_line(&self, line: &str) -> String {
        let mut result = String::new();
        let trimmed = line.trim_start();
        let indent = &line[..line.len() - trimmed.len()];
        
        result.push_str(indent);
        
        if trimmed.starts_with("//") || trimmed.starts_with("/*") {
            // Comments in green
            result.push_str(&format!("\x1b[0;32m{}\x1b[0m", trimmed));
        } else if trimmed.starts_with("function ") || trimmed.contains("=> ") {
            // Function definitions in yellow
            result.push_str(&format!("\x1b[0;33m{}\x1b[0m", trimmed));
        } else if trimmed.starts_with("const ") || trimmed.starts_with("let ") || trimmed.starts_with("var ") {
            // Variable declarations in cyan
            result.push_str(&format!("\x1b[0;36m{}\x1b[0m", trimmed));
        } else {
            result.push_str(&format!("\x1b[0;37m{}\x1b[0m", trimmed));
        }
        
        result
    }
    
    fn style_bash_line(&self, line: &str) -> String {
        let mut result = String::new();
        let trimmed = line.trim_start();
        let indent = &line[..line.len() - trimmed.len()];
        
        result.push_str(indent);
        
        if trimmed.starts_with("#") {
            // Comments in green
            result.push_str(&format!("\x1b[0;32m{}\x1b[0m", trimmed));
        } else if trimmed.starts_with("export ") || trimmed.starts_with("alias ") {
            // Exports and aliases in magenta
            result.push_str(&format!("\x1b[0;35m{}\x1b[0m", trimmed));
        } else {
            result.push_str(&format!("\x1b[0;37m{}\x1b[0m", trimmed));
        }
        
        result
    }
    
    fn style_json_line(&self, line: &str) -> String {
        let trimmed = line.trim();
        if trimmed.starts_with("\"") && trimmed.contains(":") {
            // JSON keys in cyan
            format!("\x1b[0;36m{}\x1b[0m", line)
        } else {
            format!("\x1b[0;37m{}\x1b[0m", line)
        }
    }
    
    pub fn print_styled_code_block(&self, content: &str, language: Option<&str>) {
        // Print opening border
        println!("\x1b[0;36m┌─────────────────────────────────────────────────────────────────────────────────┐\x1b[0m");
        
        if let Some(lang) = language {
            println!("\x1b[0;36m│\x1b[0m \x1b[1;33m{}\x1b[0m", lang);
            println!("\x1b[0;36m├─────────────────────────────────────────────────────────────────────────────────┤\x1b[0m");
        }
        
        // Print each line with styling and border
        for line in content.lines() {
            let styled_line = self.style_code_line(line, language);
            println!("\x1b[0;36m│\x1b[0m \x1b[48;5;236m{:<79}\x1b[0m \x1b[0;36m│\x1b[0m", styled_line);
        }
        
        // Print closing border
        println!("\x1b[0;36m└─────────────────────────────────────────────────────────────────────────────────┘\x1b[0m");
    }
}
