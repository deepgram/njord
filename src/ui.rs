use anyhow::Result;
use rustyline::error::ReadlineError;
use rustyline::{Editor, Helper, Config};
use rustyline::history::DefaultHistory;
use rustyline::completion::{Completer, Pair};
use rustyline::hint::Hinter;
use rustyline::highlight::Highlighter;
use rustyline::validate::Validator;
use std::io::{self, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::time::Duration;
use crate::input_history::InputHistory;

pub struct UI {
    editor: Editor<NjordCompleter, DefaultHistory>,
    spinner_active: Arc<AtomicBool>,
    input_history: InputHistory,
}

#[derive(Clone)]
pub struct CompletionContext {
    pub available_models: Vec<String>,
    pub session_names: Vec<String>,
    pub prompt_names: Vec<String>,
    pub variable_names: Vec<String>,
}

impl CompletionContext {
    pub fn new() -> Self {
        Self {
            available_models: Vec::new(),
            session_names: Vec::new(),
            prompt_names: Vec::new(),
            variable_names: Vec::new(),
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
            "/block", "/blocks", "/copy", "/save", "/exec", "/edit", "/summarize",
            "/prompts", "/load", "/variables", "/var"
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
        } else if line[..pos].starts_with("/summarize ") {
            return self.complete_summarize_command(line, pos);
        } else if line[..pos].starts_with("/copy ") {
            return self.complete_copy_command(line, pos);
        } else if line[..pos].starts_with("/save ") {
            return self.complete_save_command(line, pos);
        } else if line[..pos].starts_with("/prompts ") {
            return self.complete_prompts_command(line, pos);
        } else if line[..pos].starts_with("/load ") {
            return self.complete_load_command(line, pos);
        } else if line[..pos].starts_with("/var ") {
            return self.complete_var_command(line, pos);
        }
        
        // Check for variable references in regular text
        if !line[..pos].starts_with('/') {
            return self.complete_variable_references(line, pos);
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
            let subcommands = [
                "new", "save", "load", "list", "delete", "continue", "recent", "fork", "merge", "rename", "auto-rename", "auto-rename-all"
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
            if matches!(subcommand, "load" | "delete" | "continue" | "merge" | "auto-rename") {
                return self.complete_session_references(current_word);
            } else if subcommand == "rename" && parts.len() >= 3 {
                // For rename command, complete session names for the second argument (old_name)
                return self.complete_session_references(current_word);
            } else if matches!(subcommand, "save" | "fork") {
                // These commands take new session names, no completion needed
                return Vec::new();
            }
        }
        
        Vec::new()
    }
    
    fn complete_session_names(&self, current_word: &str) -> Vec<Pair> {
        // Remove quotes from current_word for matching
        let unquoted_current = self.unquote_for_matching(current_word);
        let is_quoted_input = current_word.trim().starts_with('"') || current_word.trim().starts_with('\'');
        
        self.context.session_names.iter()
            .filter(|name| name.starts_with(&unquoted_current))
            .map(|name| {
                let (display, replacement) = if is_quoted_input {
                    // User started with quotes, so complete with quotes
                    let quoted = format!("\"{}\"", name);
                    (quoted.clone(), quoted)
                } else if name.contains(' ') {
                    // Auto-quote session names with spaces only if user didn't start with quotes
                    let quoted = format!("\"{}\"", name);
                    (quoted.clone(), quoted)
                } else {
                    // No quotes needed
                    (name.clone(), name.clone())
                };
                
                Pair {
                    display,
                    replacement,
                }
            })
            .collect()
    }
    
    fn unquote_for_matching(&self, word: &str) -> String {
        // Don't trim the word - we need to preserve trailing spaces for matching
        if word.is_empty() {
            return word.to_string();
        }
        
        if let Some(stripped) = word.strip_prefix('"') {
            if word.len() == 1 {
                // Just a quote - return empty string for matching
                String::new()
            } else if word.ends_with('"') && word.len() > 1 {
                // Fully quoted - remove quotes for matching, preserving internal spaces
                word[1..word.len()-1].to_string()
            } else {
                // Partial quote - remove opening quote for matching, preserving trailing spaces
                stripped.to_string()
            }
        } else if let Some(stripped) = word.strip_prefix('\'') {
            if word.len() == 1 {
                // Just a quote - return empty string for matching
                String::new()
            } else if word.ends_with('\'') && word.len() > 1 {
                // Fully quoted - remove quotes for matching, preserving internal spaces
                word[1..word.len()-1].to_string()
            } else {
                // Partial quote - remove opening quote for matching, preserving trailing spaces
                stripped.to_string()
            }
        } else {
            // No quotes - return as-is, preserving any trailing spaces
            word.to_string()
        }
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
        
        ["on", "off"]
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
        
        ["markdown", "json", "txt"]
            .iter()
            .filter(|format| format.starts_with(current_word))
            .map(|format| Pair {
                display: format.to_string(),
                replacement: format.to_string(),
            })
            .collect()
    }
    
    fn complete_summarize_command(&self, line: &str, pos: usize) -> Vec<Pair> {
        let start_pos = self.find_completion_start(line, pos);
        let current_word = &line[start_pos..pos];
        
        // Complete session names for summarize command
        self.complete_session_references(current_word)
    }
    
    fn complete_session_references(&self, current_word: &str) -> Vec<Pair> {
        let mut completions = Vec::new();
        
        // Add ephemeral references (#1, #2, etc.) if current word starts with #
        if let Some(_number_part) = current_word.strip_prefix('#') {
            for i in 1..=10 { // Show up to 10 ephemeral references
                let ephemeral_ref = format!("#{}", i);
                if ephemeral_ref.starts_with(current_word) {
                    completions.push(Pair {
                        display: ephemeral_ref.clone(),
                        replacement: ephemeral_ref,
                    });
                }
            }
        }
        
        // Add regular session name completions
        completions.extend(self.complete_session_names(current_word));
        
        completions
    }
    
    fn complete_copy_command(&self, line: &str, pos: usize) -> Vec<Pair> {
        let start_pos = self.find_completion_start(line, pos);
        let current_word = &line[start_pos..pos];
        let input = &line[..pos];
        let parts: Vec<&str> = input.split_whitespace().collect();
        
        if parts.len() == 1 || (parts.len() == 2 && !input.ends_with(' ')) {
            // Complete copy type
            let types = ["agent", "user", "block"];
            types.iter()
                .filter(|t| t.starts_with(current_word))
                .map(|t| Pair {
                    display: t.to_string(),
                    replacement: t.to_string(),
                })
                .collect()
        } else {
            // No further completion needed for numbers
            Vec::new()
        }
    }
    
    fn complete_save_command(&self, line: &str, pos: usize) -> Vec<Pair> {
        let start_pos = self.find_completion_start(line, pos);
        let current_word = &line[start_pos..pos];
        let input = &line[..pos];
        let parts: Vec<&str> = input.split_whitespace().collect();
        
        if parts.len() == 1 || (parts.len() == 2 && !input.ends_with(' ')) {
            // Complete save type
            let types = ["agent", "user", "block"];
            types.iter()
                .filter(|t| t.starts_with(current_word))
                .map(|t| Pair {
                    display: t.to_string(),
                    replacement: t.to_string(),
                })
                .collect()
        } else {
            // No further completion for numbers or filenames
            Vec::new()
        }
    }
    
    fn complete_prompts_command(&self, line: &str, pos: usize) -> Vec<Pair> {
        let start_pos = self.find_completion_start(line, pos);
        let current_word = &line[start_pos..pos];
        let input = &line[..pos];
        let parts: Vec<&str> = input.split_whitespace().collect();
        
        if parts.len() == 1 || (parts.len() == 2 && !input.ends_with(' ')) {
            // Complete prompts subcommand
            let subcommands = [
                "list", "show", "save", "apply", "delete", "rename", "search", 
                "auto-name", "edit", "import", "export"
            ];
            
            return subcommands.iter()
                .filter(|cmd| cmd.starts_with(current_word))
                .map(|cmd| Pair {
                    display: cmd.to_string(),
                    replacement: cmd.to_string(),
                })
                .collect();
        } else if parts.len() >= 2 {
            // Complete prompt names for commands that need them
            let subcommand = parts[1];
            if matches!(subcommand, "show" | "apply" | "delete" | "edit" | "auto-name") {
                return self.complete_prompt_names(current_word);
            } else if subcommand == "rename" && parts.len() >= 3 {
                // For rename command, complete prompt names for both arguments
                return self.complete_prompt_names(current_word);
            } else if matches!(subcommand, "save" | "import" | "export") {
                // These commands take names/filenames, no specific completion
                return Vec::new();
            }
        }
        
        Vec::new()
    }
    
    fn complete_prompt_names(&self, current_word: &str) -> Vec<Pair> {
        // Remove quotes from current_word for matching
        let unquoted_current = self.unquote_for_matching(current_word);
        let is_quoted_input = current_word.trim().starts_with('"') || current_word.trim().starts_with('\'');
        
        self.context.prompt_names.iter()
            .filter(|name| name.starts_with(&unquoted_current))
            .map(|name| {
                let (display, replacement) = if is_quoted_input {
                    // User started with quotes, so complete with quotes
                    let quoted = format!("\"{}\"", name);
                    (quoted.clone(), quoted)
                } else if name.contains(' ') {
                    // Auto-quote prompt names with spaces only if user didn't start with quotes
                    let quoted = format!("\"{}\"", name);
                    (quoted.clone(), quoted)
                } else {
                    // No quotes needed
                    (name.clone(), name.clone())
                };
                
                Pair {
                    display,
                    replacement,
                }
            })
            .collect()
    }
    
    fn find_completion_start(&self, line: &str, pos: usize) -> usize {
        let line_up_to_pos = &line[..pos];
        
        // Handle quoted strings - if we're inside quotes, start from the quote
        if let Some(quote_pos) = line_up_to_pos.rfind('"') {
            // Check if this quote is the start of a quoted string (not escaped)
            let before_quote = &line_up_to_pos[..quote_pos];
            if before_quote.is_empty() || before_quote.ends_with(' ') {
                return quote_pos;
            }
        }
        
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
        
        // Check if we need to handle quoted completions
        let current_is_quoted = current_word.trim().starts_with('"') || current_word.trim().starts_with('\'');
        let completions_are_quoted = completions.iter().any(|c| c.replacement.starts_with('"'));
        
        if !current_is_quoted && completions_are_quoted {
            // Handle the case where current word is unquoted but completions are quoted
            let unquoted_current = current_word.trim();
            let unquoted_completions: Vec<String> = completions.iter()
                .map(|c| {
                    let replacement = &c.replacement;
                    if replacement.starts_with('"') && replacement.ends_with('"') && replacement.len() > 1 {
                        replacement[1..replacement.len()-1].to_string()
                    } else {
                        replacement.clone()
                    }
                })
                .collect();
            
            // Find common prefix among unquoted completions
            if let Some(first_unquoted) = unquoted_completions.first() {
                let mut common_prefix = first_unquoted.clone();
                
                for completion in &unquoted_completions[1..] {
                    let mut common_len = 0;
                    let prefix_chars: Vec<char> = common_prefix.chars().collect();
                    let completion_chars: Vec<char> = completion.chars().collect();
                    
                    for (i, (&p_char, &c_char)) in prefix_chars.iter().zip(completion_chars.iter()).enumerate() {
                        if p_char == c_char {
                            common_len = i + 1;
                        } else {
                            break;
                        }
                    }
                    
                    if common_len < prefix_chars.len() {
                        common_prefix = prefix_chars[..common_len].iter().collect();
                    }
                }
                
                // Return the quoted common prefix if it's longer than current input
                if common_prefix.len() > unquoted_current.len() {
                    // Only add opening quote for partial completions (no closing quote)
                    let mut result = String::from("\"");
                    result.push_str(&common_prefix);
                    return result;
                }
            }
            
            // No meaningful extension, return current word
            return current_word.to_string();
        }
        
        // Normal case: extract completion texts and find common prefix
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
        
        // For single completion, return the full replacement
        if completions.len() == 1 {
            let completion = &completions[0];
            return Ok((start_pos, vec![Pair {
                display: completion.display.clone(),
                replacement: completion.replacement.clone(),
            }]));
        }
        
        // For multiple completions, find the longest common prefix
        let longest_prefix = self.find_longest_common_prefix(&completions, current_word);
        
        if longest_prefix.len() > current_word.len() {
            // We have a meaningful partial completion
            Ok((start_pos, vec![Pair {
                display: longest_prefix.clone(),
                replacement: longest_prefix,
            }]))
        } else {
            // No meaningful partial completion - return empty to prevent cycling
            Ok((pos, vec![]))
        }
    }
}

impl Hinter for NjordCompleter {
    type Hint = String;
    
    fn hint(&self, line: &str, pos: usize, _ctx: &rustyline::Context<'_>) -> Option<Self::Hint> {
        let completions = self.complete_command(line, pos);
        
        match completions.len() {
            1 => {
                // Single completion available - show it as a hint
                Some(format!(" [{}]", completions[0].display))
            }
            n if n > 1 => {
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
            }
            _ => {
                // No completions available
                None
            }
        }
    }
}

impl Highlighter for NjordCompleter {}

impl Validator for NjordCompleter {}

impl Helper for NjordCompleter {}

impl UI {
    pub fn with_input_history_file(input_history_file: String) -> Result<Self> {
        // Create config with bracketed paste enabled
        let config = Config::builder()
            .bracketed_paste(true)
            .build();
        
        let mut editor = Editor::with_config(config)?;
        
        let completer = NjordCompleter::new(CompletionContext::new());
        editor.set_helper(Some(completer));
        
        // Load persistent input history
        let input_history = InputHistory::load(input_history_file)?;
        
        // Load history entries into rustyline
        for entry in input_history.get_entries() {
            let _ = editor.add_history_entry(&entry);
        }
        
        Ok(Self { 
            editor,
            spinner_active: Arc::new(AtomicBool::new(false)),
            input_history,
        })
    }
    
    // Add buffer clearing method
    pub fn clear_input_buffer(&mut self) {
        // This helps clear any residual input after processing
        // Note: rustyline doesn't have a direct way to clear input buffer,
        // but the bracketed paste mode should handle most paste issues
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
        println!("For multi-line input, start with {{ and end with }} on its own line.");
        println!("You can also use {{TAG and TAG}} for tagged blocks (e.g., {{python and python}}).");
        println!();
        
        Ok(())
    }
    
    pub fn read_input(&mut self, prompt_info: Option<(&str, &str)>, session_name: Option<&str>) -> Result<Option<String>> {
        let (prompt, initial_input) = if let Some((message, status)) = prompt_info {
            let color = match status {
                "retry" => "\x1b[1;33m", // Yellow for retry
                "interrupted" => "\x1b[1;31m", // Red for interrupted
                _ => "\x1b[1;32m", // Green default
            };
            let session_prefix = if let Some(name) = session_name {
                format!("[{}] ", name)
            } else {
                String::new()
            };
            (format!("{}{}>>> ({}) \x1b[0m", color, session_prefix, status), message)
        } else {
            let session_prefix = if let Some(name) = session_name {
                format!("[{}] ", name)
            } else {
                String::new()
            };
            (format!("\x1b[1;32m{}>>> \x1b[0m", session_prefix), "")
        };
        
        match self.editor.readline_with_initial(&prompt, (initial_input, "")) {
            Ok(line) => {
                let input = line.trim();
                if input.is_empty() && initial_input.is_empty() {
                    Ok(None)
                } else if input.is_empty() && !initial_input.is_empty() {
                    // User pressed Enter on pre-filled input without changes
                    self.editor.add_history_entry(initial_input)?;
                    self.add_to_persistent_history(initial_input.to_string());
                    Ok(Some(initial_input.to_string()))
                } else {
                    // Add to history for arrow key navigation
                    self.editor.add_history_entry(&line)?;
                    self.add_to_persistent_history(line.clone());
                    
                    // Check if input contains newlines (paste detection)
                    if input.contains('\n') {
                        let line_count = input.lines().count();
                        println!("\x1b[1;33mDetected multi-line paste ({} lines). Processing as single message.\x1b[0m", line_count);
                        
                        // Keep newlines intact for the LLM - don't replace with spaces
                        Ok(Some(input.to_string()))
                    } else if input.starts_with("{") {
                        // Multi-line input mode
                        self.read_multiline_input(input.to_string(), session_name)
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
    
    fn read_multiline_input(&mut self, first_line: String, session_name: Option<&str>) -> Result<Option<String>> {
        // Parse the opening tag from the first line
        let tag = self.parse_opening_tag(&first_line);
        let end_marker = if let Some(ref tag_name) = tag {
            format!("{}}}", tag_name)
        } else {
            "}".to_string()
        };
        
        let mut lines = Vec::new();
        
        // Show helpful message about the expected end marker
        if let Some(ref _tag_name) = tag {
            println!("\x1b[2m(Multi-line mode - end with '{}' on its own line)\x1b[0m", end_marker);
        } else {
            println!("\x1b[2m(Multi-line mode - end with '}}' on its own line)\x1b[0m");
        }
        
        let session_prefix = if let Some(name) = session_name {
            format!("[{}] ", name)
        } else {
            String::new()
        };
        
        loop {
            match self.editor.readline(&format!("\x1b[1;32m{}... \x1b[0m", session_prefix)) {
                Ok(line) => {
                    let line = line.trim_end_matches('\n').trim_end_matches('\r');
                    
                    // Check for end marker
                    if line.trim() == end_marker {
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
        
        let full_input = lines.join("\n");
        if full_input.trim().is_empty() {
            Ok(None)
        } else {
            // Add multiline input to persistent history
            self.add_to_persistent_history(full_input.clone());
            Ok(Some(full_input))
        }
    }
    
    fn parse_opening_tag(&self, line: &str) -> Option<String> {
        let trimmed = line.trim();
        
        // Check for simple "{" case
        if trimmed == "{" {
            return None;
        }
        
        // Check for "{TAG" pattern
        if trimmed.starts_with('{') && trimmed.len() > 1 {
            let tag = &trimmed[1..];
            // Any contiguous set of non-space characters is a valid tag
            if !tag.contains(char::is_whitespace) && !tag.is_empty() {
                return Some(tag.to_string());
            }
        }
        
        None
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
        println!("\x1b[2m────────────────────────────────────────\x1b[0m"); // Dimmed horizontal rule
        println!();
    }
    
    pub fn print_error(&self, error: &str) {
        eprintln!("\x1b[1;31mError\x1b[0m: {}", error);
    }
    
    pub fn print_info(&self, info: &str) {
        println!("\x1b[1;33mInfo\x1b[0m: {}", info);
    }
    
    pub fn style_code_blocks(&self, text: &str) -> String {
        let mut result = String::new();
        let mut in_code_block = false;
        
        for line in text.lines() {
            if line.starts_with("```") {
                if in_code_block {
                    // End of code block
                    result.push_str("\x1b[0;36m```\x1b[0m\n"); // Cyan closing fence
                    in_code_block = false;
                } else {
                    // Start of code block
                    result.push_str(&format!("\x1b[0;36m{}\x1b[0m\n", line)); // Cyan opening fence with language
                    in_code_block = true;
                }
            } else if in_code_block {
                // Inside code block - apply cyan coloring to entire line
                result.push_str(&format!("\x1b[0;36m{}\x1b[0m\n", line));
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
    
    pub fn print_styled_code_block(&self, content: &str, language: Option<&str>) {
        // Print opening fence with language
        if let Some(lang) = language {
            println!("\x1b[0;36m```{}\x1b[0m", lang);
        } else {
            println!("\x1b[0;36m```\x1b[0m");
        }
        
        // Print each line with cyan coloring
        for line in content.lines() {
            println!("\x1b[0;36m{}\x1b[0m", line);
        }
        
        // Print closing fence
        println!("\x1b[0;36m```\x1b[0m");
    }
    
    fn add_to_persistent_history(&mut self, input: String) {
        self.input_history.add_entry(input);
        // Save immediately to ensure persistence even if app crashes
        if let Err(e) = self.input_history.save() {
            eprintln!("Warning: Failed to save input history: {}", e);
        }
    }
    
    pub fn save_input_history(&self) -> Result<()> {
        self.input_history.save()
    }
    
    pub fn clear_input_history(&mut self) -> Result<()> {
        self.input_history.clear();
        self.input_history.save()
    }
    
    pub fn get_input_history_stats(&self) -> (usize, Option<String>) {
        let count = self.input_history.len();
        let last_entry = self.input_history.get_entries().last().cloned();
        (count, last_entry)
    }

    pub fn start_spinner(&self, message: &str) -> SpinnerHandle {
        let spinner_active = Arc::clone(&self.spinner_active);
        spinner_active.store(true, Ordering::Relaxed);
        
        let message = message.to_string();
        let spinner_active_clone = Arc::clone(&spinner_active);
        
        let handle = tokio::spawn(async move {
            let spinner_chars = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
            let mut interval = tokio::time::interval(Duration::from_millis(100));
            let mut frame = 0;
            
            // Print initial message
            print!("\r\x1b[2K\x1b[1;33m{} {}\x1b[0m", spinner_chars[0], message);
            io::stdout().flush().unwrap();
            
            while spinner_active_clone.load(Ordering::Relaxed) {
                interval.tick().await;
                frame = (frame + 1) % spinner_chars.len();
                
                if spinner_active_clone.load(Ordering::Relaxed) {
                    print!("\r\x1b[2K\x1b[1;33m{} {}\x1b[0m", spinner_chars[frame], message);
                    io::stdout().flush().unwrap();
                }
            }
            
            // Clear the spinner line when done
            print!("\r\x1b[2K");
            io::stdout().flush().unwrap();
        });
        
        SpinnerHandle {
            handle,
            spinner_active,
        }
    }
}

pub struct SpinnerHandle {
    handle: tokio::task::JoinHandle<()>,
    spinner_active: Arc<AtomicBool>,
}

impl SpinnerHandle {
    pub async fn stop(self) {
        self.spinner_active.store(false, Ordering::Relaxed);
        let _ = self.handle.await;
    }
}
