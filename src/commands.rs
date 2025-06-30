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
    ChatFork(String),
    ChatMerge(SessionReference),
    ChatRename(String, Option<SessionReference>), // (new_name, old_session_ref)
    ChatAutoRename(Option<SessionReference>), // (session_ref)
    ChatAutoRenameAll,
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
    chat_merge_regex: Regex,
    chat_rename_regex: Regex,
    chat_auto_rename_regex: Regex,
    summarize_regex: Regex,
}

impl CommandParser {
    fn parse_session_reference(input: &str) -> SessionReference {
        let trimmed = input.trim();
        
        // Check for ephemeral reference (#1, #2, etc.)
        if trimmed.starts_with('#') {
            if let Ok(number) = trimmed[1..].parse::<usize>() {
                return SessionReference::Ephemeral(number);
            }
        }
        
        // Check for quoted session name
        if (trimmed.starts_with('"') && trimmed.ends_with('"')) ||
           (trimmed.starts_with('\'') && trimmed.ends_with('\'')) {
            let unquoted = trimmed[1..trimmed.len()-1].to_string();
            // Validate that session names starting with # are quoted
            if unquoted.starts_with('#') {
                return SessionReference::Named(unquoted);
            } else {
                return SessionReference::Named(unquoted);
            }
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
            chat_fork_regex: Regex::new(r"^/chat\s+fork\s+(.+)$")?,
            chat_merge_regex: Regex::new(r"^/chat\s+merge\s+(.+)$")?,
            chat_rename_regex: Regex::new(r"^/chat\s+rename\s+(.+?)(?:\s+(.+))?$")?,
            chat_auto_rename_regex: Regex::new(r"^/chat\s+auto-rename(?:\s+(.+))?$")?,
            summarize_regex: Regex::new(r"^/summarize(?:\s+(.+))?$")?,
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
            "/thinking" => Some(Command::Thinking(!true)), // Toggle current state
            "/quit" | "/exit" => Some(Command::Quit),
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
                    let filename = caps[3].to_string();
                    Some(Command::Save(save_type, number, filename))
                } else if let Some(caps) = self.save_regex.captures(input) {
                    // Default to agent type for backward compatibility
                    Some(Command::Save(SaveType::Agent, None, caps[1].to_string()))
                } else if let Some(caps) = self.exec_regex.captures(input) {
                    Some(Command::Exec(caps[1].parse().unwrap_or(1)))
                } else if let Some(caps) = self.system_regex.captures(input) {
                    Some(Command::System(caps[1].to_string()))
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
                    // Fork still uses string name for the new session
                    let trimmed = caps[1].trim();
                    let unquoted = if (trimmed.starts_with('"') && trimmed.ends_with('"')) ||
                                     (trimmed.starts_with('\'') && trimmed.ends_with('\'')) {
                        trimmed[1..trimmed.len()-1].to_string()
                    } else {
                        trimmed.to_string()
                    };
                    Some(Command::ChatFork(unquoted))
                } else if let Some(caps) = self.chat_merge_regex.captures(input) {
                    Some(Command::ChatMerge(Self::parse_session_reference(&caps[1])))
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
                } else {
                    None
                }
            }
        }
    }
}
