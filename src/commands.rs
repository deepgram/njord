use anyhow::Result;
use regex::Regex;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum Command {
    Model(String),
    Models,
    ChatNew,
    ChatSave(String),
    ChatLoad(SessionReference),
    ChatList,
    ChatDelete(Option<SessionReference>),
    ChatContinue(Option<SessionReference>),
    ChatRecent,
    ChatFork(Option<String>),
    ChatBranch(SessionReference, Option<String>), // (source_session_ref, optional_new_name)
    ChatRename(String, Option<SessionReference>), // (new_name, old_session_ref)
    ChatAutoRename(Option<SessionReference>), // (session_ref)
    ChatAutoRenameAll,
    ChatName(String),
    Summarize(Option<SessionReference>), // (session_ref)
    Undo(Option<usize>),
    Goto(usize),
    History,
    Search(String),
    Blocks,
    Block(usize),
    Copy(CopyType, Option<usize>), // (type, number)
    Save(SaveType, Option<usize>, String), // (type, number, filename)
    Exec(usize),
    System(String),
    Temperature(f32),
    MaxTokens(u32),
    ThinkingBudget(u32),
    Thinking(bool),
    Tokens,
    Export(String),
    Help,
    Clear,
    Stats,
    Status,
    Retry,
    Edit(usize),
    Quit,
    // File loading commands
    Load(String, Option<String>), // (filename, optional_variable_name)
    Variables,
    VariableShow(String),
    VariableDelete(String),
    VariableReload(Option<String>), // (optional_variable_name) - reload specific var or all vars
    // Prompt library commands
    PromptsList,
    PromptsShow(String),
    PromptsSave(String, Option<String>), // (name, optional_content)
    PromptsApply(String),
    PromptsDelete(String),
    PromptsRename(String, String), // (old_name, new_name)
    PromptsSearch(String),
    PromptsAutoName(Option<String>),
    PromptsEdit(String),
    PromptsImport(String), // filename
    PromptsExport(Option<String>), // optional filename
    // Input history commands
    InputHistory,
    InputHistoryClear,
    InputHistoryStats,
}

#[derive(Debug, Clone)]
pub enum CopyType {
    Agent,
    User,
    Block,
}

#[derive(Debug, Clone)]
pub enum SaveType {
    Agent,
    User,
    Block,
}

#[derive(Debug, Clone)]
pub enum SessionReference {
    Named(String),
    Ephemeral(usize),
    Invalid(String),
}

pub struct CommandParser {
    model_regex: Regex,
    undo_regex: Regex,
    goto_regex: Regex,
    search_regex: Regex,
    block_regex: Regex,
    copy_regex: Regex,
    copy_typed_regex: Regex,
    save_regex: Regex,
    save_typed_regex: Regex,
    exec_regex: Regex,
    system_regex: Regex,
    temp_regex: Regex,
    max_tokens_regex: Regex,
    thinking_budget_regex: Regex,
    thinking_regex: Regex,
    export_regex: Regex,
    edit_regex: Regex,
    chat_save_regex: Regex,
    chat_load_regex: Regex,
    chat_delete_regex: Regex,
    chat_continue_regex: Regex,
    chat_fork_regex: Regex,
    chat_branch_regex: Regex,
    chat_rename_regex: Regex,
    chat_auto_rename_regex: Regex,
    summarize_regex: Regex,
    // File loading regexes
    load_regex: Regex,
    variable_show_regex: Regex,
    variable_delete_regex: Regex,
    variable_reload_regex: Regex,
    // Prompt library regexes
    prompts_save_regex: Regex,
    prompts_show_regex: Regex,
    prompts_apply_regex: Regex,
    prompts_delete_regex: Regex,
    prompts_rename_regex: Regex,
    prompts_search_regex: Regex,
    prompts_auto_name_regex: Regex,
    prompts_edit_regex: Regex,
    prompts_import_regex: Regex,
    prompts_export_regex: Regex,
}

impl CommandParser {
    fn parse_session_reference(input: &str) -> SessionReference {
        let trimmed = input.trim();
        
        // Check for ephemeral reference (#1, #2, etc.)
        if let Some(stripped) = trimmed.strip_prefix('#') {
            if let Ok(number) = stripped.parse::<usize>() {
                return SessionReference::Ephemeral(number);
            }
        }
        
        // Check for quoted session name
        if (trimmed.starts_with('"') && trimmed.ends_with('"')) ||
           (trimmed.starts_with('\'') && trimmed.ends_with('\'')) {
            let unquoted = trimmed[1..trimmed.len()-1].to_string();
            // Validate that session names starting with # are quoted
            return SessionReference::Named(unquoted);
        }
        
        // Unquoted session name - must not start with #
        if trimmed.starts_with('#') {
            // This should be an error case - unquoted names can't start with #
            return SessionReference::Invalid(format!("Session names starting with # must be quoted: \"{}\"", trimmed));
        }
        
        SessionReference::Named(trimmed.to_string())
    }
    
    fn unquote_session_name(name: &str) -> String {
        let trimmed = name.trim();
        if (trimmed.starts_with('"') && trimmed.ends_with('"')) ||
           (trimmed.starts_with('\'') && trimmed.ends_with('\'')) {
            // Remove surrounding quotes
            trimmed[1..trimmed.len()-1].to_string()
        } else {
            trimmed.to_string()
        }
    }
    
    fn parse_load_arguments(args: &str) -> (String, Option<String>) {
        let args = args.trim();
        
        // Handle quoted first argument
        if args.starts_with('"') {
            // Find the closing quote
            if let Some(end_quote) = args[1..].find('"') {
                let filename = args[1..end_quote + 1].to_string();
                let remaining = args[end_quote + 2..].trim();
                if remaining.is_empty() {
                    (filename, None)
                } else {
                    let variable_name = Self::unquote_session_name(remaining);
                    (filename, Some(variable_name))
                }
            } else {
                // Unclosed quote - treat as unquoted
                let parts: Vec<&str> = args.split_whitespace().collect();
                if parts.len() >= 2 {
                    (Self::unquote_session_name(parts[0]), Some(Self::unquote_session_name(parts[1])))
                } else {
                    (Self::unquote_session_name(parts[0]), None)
                }
            }
        } else {
            // Unquoted first argument
            let parts: Vec<&str> = args.split_whitespace().collect();
            if parts.len() >= 2 {
                (parts[0].to_string(), Some(Self::unquote_session_name(parts[1])))
            } else {
                (parts[0].to_string(), None)
            }
        }
    }
    
    fn parse_prompts_save_arguments(args: &str) -> (String, Option<String>) {
        let args = args.trim();
        
        // Handle quoted first argument (name)
        if args.starts_with('"') {
            // Find the proper closing quote for the name, handling escaped quotes
            let mut end_quote_pos = None;
            let mut i = 1; // Start after opening quote
            let chars: Vec<char> = args.chars().collect();
            
            while i < chars.len() {
                if chars[i] == '\\' && i + 1 < chars.len() {
                    // Skip the escaped character
                    i += 2;
                } else if chars[i] == '"' {
                    // Found unescaped closing quote
                    end_quote_pos = Some(i);
                    break;
                } else {
                    i += 1;
                }
            }
            
            if let Some(end_quote) = end_quote_pos {
                // Extract the name and unescape quotes
                let name = args[1..end_quote].replace("\\\"", "\"");
                let remaining = args[end_quote + 1..].trim();
                if remaining.is_empty() {
                    (name, None)
                } else {
                    // Handle quoted content
                    if remaining.starts_with('"') {
                        // Find the proper closing quote for content, handling escaped quotes
                        let mut content_end_quote_pos = None;
                        let mut j = 1; // Start after opening quote
                        let remaining_chars: Vec<char> = remaining.chars().collect();
                        
                        while j < remaining_chars.len() {
                            if remaining_chars[j] == '\\' && j + 1 < remaining_chars.len() {
                                // Skip the escaped character
                                j += 2;
                            } else if remaining_chars[j] == '"' {
                                // Found unescaped closing quote
                                content_end_quote_pos = Some(j);
                                break;
                            } else {
                                j += 1;
                            }
                        }
                        
                        if let Some(content_end_quote) = content_end_quote_pos {
                            let content = remaining[1..content_end_quote].replace("\\\"", "\"");
                            (name, Some(content))
                        } else {
                            // Unclosed quote in content - take everything after the quote and unescape
                            let content = remaining[1..].replace("\\\"", "\"");
                            (name, Some(content))
                        }
                    } else {
                        // Unquoted content
                        (name, Some(remaining.to_string()))
                    }
                }
            } else {
                // Unclosed quote - treat as unquoted
                let parts: Vec<&str> = args.split_whitespace().collect();
                if parts.len() >= 2 {
                    let name = Self::unquote_session_name(parts[0]);
                    let content = parts[1..].join(" ");
                    let content = Self::unquote_session_name(&content);
                    (name, Some(content))
                } else {
                    let name = Self::unquote_session_name(parts[0]);
                    (name, None)
                }
            }
        } else {
            // Unquoted first argument (name)
            let parts: Vec<&str> = args.splitn(2, ' ').collect();
            if parts.len() >= 2 {
                let name = parts[0].to_string();
                let content = Self::unquote_session_name(parts[1]);
                (name, Some(content))
            } else {
                (parts[0].to_string(), None)
            }
        }
    }
    
    pub fn new() -> Result<Self> {
        Ok(Self {
            model_regex: Regex::new(r"^/model\s+(.+)$")?,
            undo_regex: Regex::new(r"^/undo(?:\s+(\d+))?$")?,
            goto_regex: Regex::new(r"^/goto\s+(\d+)$")?,
            search_regex: Regex::new(r"^/search\s+(.+)$")?,
            block_regex: Regex::new(r"^/block\s+(\d+)$")?,
            copy_regex: Regex::new(r"^/copy(?:\s+(\d+))?$")?,
            copy_typed_regex: Regex::new(r"^/copy\s+(agent|user|block)(?:\s+(\d+))?$")?,
            save_regex: Regex::new(r"^/save\s+(.+)$")?,
            save_typed_regex: Regex::new(r"^/save\s+(agent|user|block)(?:\s+(\d+))?\s+(.+)$")?,
            exec_regex: Regex::new(r"^/exec\s+(\d+)$")?,
            system_regex: Regex::new(r"^/system\s+(.+)$")?,
            temp_regex: Regex::new(r"^/temp\s+([\d.]+)$")?,
            max_tokens_regex: Regex::new(r"^/max-tokens\s+(\d+)$")?,
            thinking_budget_regex: Regex::new(r"^/thinking-budget\s+(\d+)$")?,
            thinking_regex: Regex::new(r"^/thinking\s+(on|off|true|false)$")?,
            export_regex: Regex::new(r"^/export\s+(\w+)$")?,
            edit_regex: Regex::new(r"^/edit\s+(\d+)$")?,
            chat_save_regex: Regex::new(r"^/chat\s+save\s+(.+)$")?,
            chat_load_regex: Regex::new(r"^/chat\s+load\s+(.+)$")?,
            chat_delete_regex: Regex::new(r"^/chat\s+delete(?:\s+(.+))?$")?,
            chat_continue_regex: Regex::new(r"^/chat\s+continue(?:\s+(.+))?$")?,
            chat_fork_regex: Regex::new(r"^/chat\s+fork(?:\s+(.+))?$")?,
            chat_branch_regex: Regex::new(r"^/chat\s+branch\s+(.+?)(?:\s+(.+))?$")?,
            chat_rename_regex: Regex::new(r"^/chat\s+rename\s+(.+?)(?:\s+(.+))?$")?,
            chat_auto_rename_regex: Regex::new(r"^/chat\s+auto-rename(?:\s+(.+))?$")?,
            summarize_regex: Regex::new(r"^/summarize(?:\s+(.+))?$")?,
            // File loading regexes
            load_regex: Regex::new(r"^/load\s+(.+)$")?,
            variable_show_regex: Regex::new(r"^/var\s+show\s+(.+)$")?,
            variable_delete_regex: Regex::new(r"^/var\s+delete\s+(.+)$")?,
            variable_reload_regex: Regex::new(r"^/var\s+reload(?:\s+(.+))?$")?,
            // Prompt library regexes
            prompts_save_regex: Regex::new(r"^/prompts\s+save\s+(.+)$")?,
            prompts_show_regex: Regex::new(r"^/prompts\s+show\s+(.+)$")?,
            prompts_apply_regex: Regex::new(r"^/prompts\s+apply\s+(.+)$")?,
            prompts_delete_regex: Regex::new(r"^/prompts\s+delete\s+(.+)$")?,
            prompts_rename_regex: Regex::new(r"^/prompts\s+rename\s+(.+?)\s+(.+)$")?,
            prompts_search_regex: Regex::new(r"^/prompts\s+search\s+(.+)$")?,
            prompts_auto_name_regex: Regex::new(r"^/prompts\s+auto-name(?:\s+(.+))?$")?,
            prompts_edit_regex: Regex::new(r"^/prompts\s+edit\s+(.+)$")?,
            prompts_import_regex: Regex::new(r"^/prompts\s+import\s+(.+)$")?,
            prompts_export_regex: Regex::new(r"^/prompts\s+export(?:\s+(.+))?$")?,
        })
    }
    
    pub fn parse(&self, input: &str) -> Option<Command> {
        let input = input.trim();
        
        if !input.starts_with('/') {
            return None;
        }
        
        match input {
            "/models" => Some(Command::Models),
            "/chat new" => Some(Command::ChatNew),
            "/chat list" => Some(Command::ChatList),
            "/chat recent" => Some(Command::ChatRecent),
            "/chat auto-rename-all" => Some(Command::ChatAutoRenameAll),
            "/history" => Some(Command::History),
            "/blocks" => Some(Command::Blocks),
            "/tokens" => Some(Command::Tokens),
            "/help" | "/commands" => Some(Command::Help),
            "/clear" => Some(Command::Clear),
            "/stats" => Some(Command::Stats),
            "/status" => Some(Command::Status),
            "/retry" => Some(Command::Retry),
            "/system" => Some(Command::System(String::new())),
            "/thinking" => Some(Command::Thinking(false)), // Toggle current state
            "/quit" | "/exit" => Some(Command::Quit),
            // File loading commands
            "/variables" | "/vars" => Some(Command::Variables),
            // Prompt library commands
            "/prompts list" => Some(Command::PromptsList),
            "/prompts auto-name" => Some(Command::PromptsAutoName(None)),
            "/input-history" => Some(Command::InputHistory),
            "/input-history clear" => Some(Command::InputHistoryClear),
            "/input-history stats" => Some(Command::InputHistoryStats),
            _ if input.starts_with("/chat name ") => {
                let name = input[11..].trim();
                if name.is_empty() {
                    return None;
                }
                let unquoted_name = if (name.starts_with('"') && name.ends_with('"')) ||
                                      (name.starts_with('\'') && name.ends_with('\'')) {
                    name[1..name.len()-1].to_string()
                } else {
                    name.to_string()
                };
                Some(Command::ChatName(unquoted_name))
            }
            _ => {
                if let Some(caps) = self.model_regex.captures(input) {
                    Some(Command::Model(caps[1].to_string()))
                } else if let Some(caps) = self.undo_regex.captures(input) {
                    let count = caps.get(1).map(|m| m.as_str().parse().unwrap_or(1));
                    Some(Command::Undo(count))
                } else if let Some(caps) = self.goto_regex.captures(input) {
                    Some(Command::Goto(caps[1].parse().unwrap_or(1)))
                } else if let Some(caps) = self.search_regex.captures(input) {
                    Some(Command::Search(caps[1].to_string()))
                } else if let Some(caps) = self.block_regex.captures(input) {
                    Some(Command::Block(caps[1].parse().unwrap_or(1)))
                } else if let Some(caps) = self.copy_typed_regex.captures(input) {
                    let copy_type = match caps[1].as_ref() {
                        "agent" => CopyType::Agent,
                        "user" => CopyType::User,
                        "block" => CopyType::Block,
                        _ => CopyType::Agent,
                    };
                    let number = caps.get(2).map(|m| m.as_str().parse().unwrap_or(1));
                    Some(Command::Copy(copy_type, number))
                } else if let Some(caps) = self.copy_regex.captures(input) {
                    // Default to agent type for backward compatibility
                    let number = caps.get(1).map(|m| m.as_str().parse().unwrap_or(1));
                    Some(Command::Copy(CopyType::Agent, number))
                } else if let Some(caps) = self.save_typed_regex.captures(input) {
                    let save_type = match caps[1].as_ref() {
                        "agent" => SaveType::Agent,
                        "user" => SaveType::User,
                        "block" => SaveType::Block,
                        _ => SaveType::Agent,
                    };
                    let number = caps.get(2).map(|m| m.as_str().parse().unwrap_or(1));
                    let filename = Self::unquote_session_name(&caps[3]);
                    Some(Command::Save(save_type, number, filename))
                } else if let Some(caps) = self.save_regex.captures(input) {
                    // Default to agent type for backward compatibility
                    let filename = Self::unquote_session_name(&caps[1]);
                    Some(Command::Save(SaveType::Agent, None, filename))
                } else if let Some(caps) = self.exec_regex.captures(input) {
                    Some(Command::Exec(caps[1].parse().unwrap_or(1)))
                } else if let Some(caps) = self.system_regex.captures(input) {
                    Some(Command::System(Self::unquote_session_name(&caps[1])))
                } else if let Some(caps) = self.temp_regex.captures(input) {
                    Some(Command::Temperature(caps[1].parse().unwrap_or(0.7)))
                } else if let Some(caps) = self.max_tokens_regex.captures(input) {
                    Some(Command::MaxTokens(caps[1].parse().unwrap_or(4096)))
                } else if let Some(caps) = self.thinking_budget_regex.captures(input) {
                    Some(Command::ThinkingBudget(caps[1].parse().unwrap_or(20000)))
                } else if let Some(caps) = self.thinking_regex.captures(input) {
                    let enable = matches!(caps[1].as_ref(), "on" | "true");
                    Some(Command::Thinking(enable))
                } else if let Some(caps) = self.export_regex.captures(input) {
                    Some(Command::Export(caps[1].to_string()))
                } else if let Some(caps) = self.edit_regex.captures(input) {
                    Some(Command::Edit(caps[1].parse().unwrap_or(1)))
                } else if let Some(caps) = self.chat_save_regex.captures(input) {
                    Some(Command::ChatSave(Self::unquote_session_name(&caps[1])))
                } else if let Some(caps) = self.chat_load_regex.captures(input) {
                    Some(Command::ChatLoad(Self::parse_session_reference(&caps[1])))
                } else if let Some(caps) = self.chat_delete_regex.captures(input) {
                    let session_ref = caps.get(1).map(|m| Self::parse_session_reference(m.as_str()));
                    Some(Command::ChatDelete(session_ref))
                } else if let Some(caps) = self.chat_continue_regex.captures(input) {
                    let session_ref = caps.get(1).map(|m| Self::parse_session_reference(m.as_str()));
                    Some(Command::ChatContinue(session_ref))
                } else if let Some(caps) = self.chat_fork_regex.captures(input) {
                    // Fork can have optional name
                    let name = caps.get(1).map(|m| {
                        let trimmed = m.as_str().trim();
                        if (trimmed.starts_with('"') && trimmed.ends_with('"')) ||
                           (trimmed.starts_with('\'') && trimmed.ends_with('\'')) {
                            trimmed[1..trimmed.len()-1].to_string()
                        } else {
                            trimmed.to_string()
                        }
                    });
                    Some(Command::ChatFork(name))
                } else if let Some(caps) = self.chat_branch_regex.captures(input) {
                    let source_session_ref = Self::parse_session_reference(&caps[1]);
                    let new_name = caps.get(2).map(|m| {
                        let trimmed = m.as_str().trim();
                        if (trimmed.starts_with('"') && trimmed.ends_with('"')) ||
                           (trimmed.starts_with('\'') && trimmed.ends_with('\'')) {
                            trimmed[1..trimmed.len()-1].to_string()
                        } else {
                            trimmed.to_string()
                        }
                    });
                    Some(Command::ChatBranch(source_session_ref, new_name))
                } else if let Some(caps) = self.chat_rename_regex.captures(input) {
                    let trimmed = caps[1].trim();
                    let new_name = if (trimmed.starts_with('"') && trimmed.ends_with('"')) ||
                                     (trimmed.starts_with('\'') && trimmed.ends_with('\'')) {
                        trimmed[1..trimmed.len()-1].to_string()
                    } else {
                        trimmed.to_string()
                    };
                    let old_session_ref = caps.get(2).map(|m| Self::parse_session_reference(m.as_str()));
                    Some(Command::ChatRename(new_name, old_session_ref))
                } else if let Some(caps) = self.chat_auto_rename_regex.captures(input) {
                    let session_ref = caps.get(1).map(|m| Self::parse_session_reference(m.as_str()));
                    Some(Command::ChatAutoRename(session_ref))
                } else if let Some(caps) = self.summarize_regex.captures(input) {
                    let session_ref = caps.get(1).map(|m| Self::parse_session_reference(m.as_str()));
                    Some(Command::Summarize(session_ref))
                } else if let Some(caps) = self.load_regex.captures(input) {
                    // Parse the arguments manually to handle quoted strings properly
                    let args_part = &caps[1];
                    let (filename, variable_name) = Self::parse_load_arguments(args_part);
                    Some(Command::Load(filename, variable_name))
                } else if let Some(caps) = self.variable_show_regex.captures(input) {
                    let name = Self::unquote_session_name(&caps[1]);
                    Some(Command::VariableShow(name))
                } else if let Some(caps) = self.variable_delete_regex.captures(input) {
                    let name = Self::unquote_session_name(&caps[1]);
                    Some(Command::VariableDelete(name))
                } else if let Some(caps) = self.variable_reload_regex.captures(input) {
                    let name = caps.get(1).map(|m| Self::unquote_session_name(m.as_str()));
                    Some(Command::VariableReload(name))
                } else if let Some(caps) = self.prompts_save_regex.captures(input) {
                    // Parse the arguments manually to handle quoted strings properly
                    let args_part = &caps[1];
                    let (name, content) = Self::parse_prompts_save_arguments(args_part);
                    Some(Command::PromptsSave(name, content))
                } else if let Some(caps) = self.prompts_show_regex.captures(input) {
                    let name = Self::unquote_session_name(&caps[1]);
                    Some(Command::PromptsShow(name))
                } else if let Some(caps) = self.prompts_apply_regex.captures(input) {
                    let name = Self::unquote_session_name(&caps[1]);
                    Some(Command::PromptsApply(name))
                } else if let Some(caps) = self.prompts_delete_regex.captures(input) {
                    let name = Self::unquote_session_name(&caps[1]);
                    Some(Command::PromptsDelete(name))
                } else if let Some(caps) = self.prompts_rename_regex.captures(input) {
                    let old_name = Self::unquote_session_name(&caps[1]);
                    let new_name = Self::unquote_session_name(&caps[2]);
                    Some(Command::PromptsRename(old_name, new_name))
                } else if let Some(caps) = self.prompts_search_regex.captures(input) {
                    Some(Command::PromptsSearch(Self::unquote_session_name(&caps[1])))
                } else if let Some(caps) = self.prompts_auto_name_regex.captures(input) {
                    let name = caps.get(1).map(|m| Self::unquote_session_name(m.as_str()));
                    Some(Command::PromptsAutoName(name))
                } else if let Some(caps) = self.prompts_edit_regex.captures(input) {
                    let name = Self::unquote_session_name(&caps[1]);
                    Some(Command::PromptsEdit(name))
                } else if let Some(caps) = self.prompts_import_regex.captures(input) {
                    let filename = Self::unquote_session_name(&caps[1]);
                    Some(Command::PromptsImport(filename))
                } else if let Some(caps) = self.prompts_export_regex.captures(input) {
                    let filename = caps.get(1).map(|m| Self::unquote_session_name(m.as_str()));
                    Some(Command::PromptsExport(filename))
                } else {
                    None
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_parser() -> CommandParser {
        CommandParser::new().unwrap()
    }

    #[test]
    fn test_basic_commands() {
        let parser = create_parser();
        
        assert!(matches!(parser.parse("/help"), Some(Command::Help)));
        assert!(matches!(parser.parse("/models"), Some(Command::Models)));
        assert!(matches!(parser.parse("/quit"), Some(Command::Quit)));
        assert!(matches!(parser.parse("/clear"), Some(Command::Clear)));
        assert!(matches!(parser.parse("/status"), Some(Command::Status)));
        assert!(matches!(parser.parse("/history"), Some(Command::History)));
        assert!(matches!(parser.parse("/blocks"), Some(Command::Blocks)));
    }

    #[test]
    fn test_model_command() {
        let parser = create_parser();
        
        if let Some(Command::Model(model)) = parser.parse("/model gpt-4") {
            assert_eq!(model, "gpt-4");
        } else {
            panic!("Expected Model command");
        }
        
        if let Some(Command::Model(model)) = parser.parse("/model claude-sonnet-4-20250514") {
            assert_eq!(model, "claude-sonnet-4-20250514");
        } else {
            panic!("Expected Model command");
        }
    }

    #[test]
    fn test_temperature_command() {
        let parser = create_parser();
        
        if let Some(Command::Temperature(temp)) = parser.parse("/temp 0.7") {
            assert_eq!(temp, 0.7);
        } else {
            panic!("Expected Temperature command");
        }
        
        if let Some(Command::Temperature(temp)) = parser.parse("/temp 1.5") {
            assert_eq!(temp, 1.5);
        } else {
            panic!("Expected Temperature command");
        }
    }

    #[test]
    fn test_undo_command() {
        let parser = create_parser();
        
        // Test undo without count
        if let Some(Command::Undo(count)) = parser.parse("/undo") {
            assert_eq!(count, None);
        } else {
            panic!("Expected Undo command");
        }
        
        // Test undo with count
        if let Some(Command::Undo(count)) = parser.parse("/undo 3") {
            assert_eq!(count, Some(3));
        } else {
            panic!("Expected Undo command");
        }
    }

    #[test]
    fn test_goto_command() {
        let parser = create_parser();
        
        if let Some(Command::Goto(number)) = parser.parse("/goto 5") {
            assert_eq!(number, 5);
        } else {
            panic!("Expected Goto command");
        }
    }

    #[test]
    fn test_search_command() {
        let parser = create_parser();
        
        if let Some(Command::Search(term)) = parser.parse("/search hello world") {
            assert_eq!(term, "hello world");
        } else {
            panic!("Expected Search command");
        }
    }

    #[test]
    fn test_copy_commands() {
        let parser = create_parser();
        
        // Test basic copy
        if let Some(Command::Copy(copy_type, number)) = parser.parse("/copy") {
            assert!(matches!(copy_type, CopyType::Agent));
            assert_eq!(number, None);
        } else {
            panic!("Expected Copy command");
        }
        
        // Test copy with number
        if let Some(Command::Copy(copy_type, number)) = parser.parse("/copy 2") {
            assert!(matches!(copy_type, CopyType::Agent));
            assert_eq!(number, Some(2));
        } else {
            panic!("Expected Copy command");
        }
        
        // Test typed copy
        if let Some(Command::Copy(copy_type, number)) = parser.parse("/copy user 3") {
            assert!(matches!(copy_type, CopyType::User));
            assert_eq!(number, Some(3));
        } else {
            panic!("Expected Copy command");
        }
        
        if let Some(Command::Copy(copy_type, number)) = parser.parse("/copy block 1") {
            assert!(matches!(copy_type, CopyType::Block));
            assert_eq!(number, Some(1));
        } else {
            panic!("Expected Copy command");
        }
    }

    #[test]
    fn test_session_references() {
        // Test named reference
        let named_ref = CommandParser::parse_session_reference("my-session");
        if let SessionReference::Named(name) = named_ref {
            assert_eq!(name, "my-session");
        } else {
            panic!("Expected Named reference");
        }
        
        // Test ephemeral reference
        let ephemeral_ref = CommandParser::parse_session_reference("#5");
        if let SessionReference::Ephemeral(number) = ephemeral_ref {
            assert_eq!(number, 5);
        } else {
            panic!("Expected Ephemeral reference");
        }
        
        // Test quoted reference
        let quoted_ref = CommandParser::parse_session_reference("\"session with spaces\"");
        if let SessionReference::Named(name) = quoted_ref {
            assert_eq!(name, "session with spaces");
        } else {
            panic!("Expected Named reference");
        }
        
        // Test quoted reference with hash
        let quoted_hash_ref = CommandParser::parse_session_reference("\"#special-session\"");
        if let SessionReference::Named(name) = quoted_hash_ref {
            assert_eq!(name, "#special-session");
        } else {
            panic!("Expected Named reference");
        }
    }

    #[test]
    fn test_chat_commands() {
        let parser = create_parser();
        
        // Test chat save
        if let Some(Command::ChatSave(name)) = parser.parse("/chat save my-session") {
            assert_eq!(name, "my-session");
        } else {
            panic!("Expected ChatSave command");
        }
        
        // Test chat load with named reference
        if let Some(Command::ChatLoad(session_ref)) = parser.parse("/chat load my-session") {
            if let SessionReference::Named(name) = session_ref {
                assert_eq!(name, "my-session");
            } else {
                panic!("Expected Named reference");
            }
        } else {
            panic!("Expected ChatLoad command");
        }
        
        // Test chat load with ephemeral reference
        if let Some(Command::ChatLoad(session_ref)) = parser.parse("/chat load #3") {
            if let SessionReference::Ephemeral(number) = session_ref {
                assert_eq!(number, 3);
            } else {
                panic!("Expected Ephemeral reference");
            }
        } else {
            panic!("Expected ChatLoad command");
        }
    }

    #[test]
    fn test_thinking_command() {
        let parser = create_parser();
        
        if let Some(Command::Thinking(enabled)) = parser.parse("/thinking on") {
            assert!(enabled);
        } else {
            panic!("Expected Thinking command");
        }
        
        if let Some(Command::Thinking(enabled)) = parser.parse("/thinking off") {
            assert!(!enabled);
        } else {
            panic!("Expected Thinking command");
        }
        
        if let Some(Command::Thinking(enabled)) = parser.parse("/thinking true") {
            assert!(enabled);
        } else {
            panic!("Expected Thinking command");
        }
        
        if let Some(Command::Thinking(enabled)) = parser.parse("/thinking false") {
            assert!(!enabled);
        } else {
            panic!("Expected Thinking command");
        }
    }

    #[test]
    fn test_save_commands() {
        let parser = create_parser();
        
        // Test save with quoted filename
        if let Some(Command::Save(save_type, number, filename)) = parser.parse("/save agent 1 \"My File.md\"") {
            assert!(matches!(save_type, SaveType::Agent));
            assert_eq!(number, Some(1));
            assert_eq!(filename, "My File.md");
        } else {
            panic!("Expected Save command");
        }
        
        // Test save with unquoted filename
        if let Some(Command::Save(save_type, number, filename)) = parser.parse("/save user 2 output.txt") {
            assert!(matches!(save_type, SaveType::User));
            assert_eq!(number, Some(2));
            assert_eq!(filename, "output.txt");
        } else {
            panic!("Expected Save command");
        }
        
        // Test basic save with quoted filename
        if let Some(Command::Save(save_type, number, filename)) = parser.parse("/save \"My File.md\"") {
            assert!(matches!(save_type, SaveType::Agent));
            assert_eq!(number, None);
            assert_eq!(filename, "My File.md");
        } else {
            panic!("Expected Save command");
        }
    }

    #[test]
    fn test_invalid_commands() {
        let parser = create_parser();
        
        assert!(parser.parse("hello").is_none());
        assert!(parser.parse("/invalid").is_none());
        assert!(parser.parse("/model").is_none()); // Missing argument
        assert!(parser.parse("/temp").is_none()); // Missing argument
        assert!(parser.parse("/goto").is_none()); // Missing argument
    }

    #[test]
    fn test_load_commands() {
        let parser = create_parser();
        
        // Test basic load
        if let Some(Command::Load(filename, var_name)) = parser.parse("/load config.json") {
            assert_eq!(filename, "config.json");
            assert_eq!(var_name, None);
        } else {
            panic!("Expected Load command");
        }
        
        // Test load with variable name
        if let Some(Command::Load(filename, var_name)) = parser.parse("/load data.txt mydata") {
            assert_eq!(filename, "data.txt");
            assert_eq!(var_name, Some("mydata".to_string()));
        } else {
            panic!("Expected Load command");
        }
        
        // Test load with quoted filename
        if let Some(Command::Load(filename, var_name)) = parser.parse("/load \"my file.txt\" myvar") {
            assert_eq!(filename, "my file.txt");
            assert_eq!(var_name, Some("myvar".to_string()));
        } else {
            panic!("Expected Load command");
        }
    }

    #[test]
    fn test_variable_commands() {
        let parser = create_parser();
        
        // Test variables list
        assert!(matches!(parser.parse("/variables"), Some(Command::Variables)));
        assert!(matches!(parser.parse("/vars"), Some(Command::Variables)));
        
        // Test variable show
        if let Some(Command::VariableShow(name)) = parser.parse("/var show myvar") {
            assert_eq!(name, "myvar");
        } else {
            panic!("Expected VariableShow command");
        }
        
        // Test variable delete
        if let Some(Command::VariableDelete(name)) = parser.parse("/var delete myvar") {
            assert_eq!(name, "myvar");
        } else {
            panic!("Expected VariableDelete command");
        }
    }

    #[test]
    fn test_whitespace_handling() {
        let parser = create_parser();
        
        // Test commands with extra whitespace
        assert!(matches!(parser.parse("  /help  "), Some(Command::Help)));
        assert!(matches!(parser.parse("/models   "), Some(Command::Models)));
        
        if let Some(Command::Model(model)) = parser.parse("/model   gpt-4   ") {
            assert_eq!(model, "gpt-4");
        } else {
            panic!("Expected Model command");
        }
    }
}
