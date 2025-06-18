use anyhow::Result;
use regex::Regex;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum Command {
    Model(String),
    Models,
    ChatNew,
    ChatSave(String),
    ChatLoad(String),
    ChatList,
    ChatDelete(String),
    ChatContinue(Option<String>),
    ChatRecent,
    ChatFork(String),
    ChatMerge(String),
    Undo(Option<usize>),
    Goto(usize),
    History,
    Search(String),
    Block(usize),
    Copy(usize),
    Save(usize, String),
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
    Provider(String),
    Retry,
    Edit(usize),
    Quit,
}

pub struct CommandParser {
    model_regex: Regex,
    undo_regex: Regex,
    goto_regex: Regex,
    search_regex: Regex,
    block_regex: Regex,
    copy_regex: Regex,
    save_regex: Regex,
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
    provider_regex: Regex,
}

impl CommandParser {
    pub fn new() -> Result<Self> {
        Ok(Self {
            model_regex: Regex::new(r"^/model\s+(.+)$")?,
            undo_regex: Regex::new(r"^/undo(?:\s+(\d+))?$")?,
            goto_regex: Regex::new(r"^/goto\s+(\d+)$")?,
            search_regex: Regex::new(r"^/search\s+(.+)$")?,
            block_regex: Regex::new(r"^/block\s+(\d+)$")?,
            copy_regex: Regex::new(r"^/copy\s+(\d+)$")?,
            save_regex: Regex::new(r"^/save\s+(\d+)\s+(.+)$")?,
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
            chat_delete_regex: Regex::new(r"^/chat\s+delete\s+(.+)$")?,
            chat_continue_regex: Regex::new(r"^/chat\s+continue(?:\s+(.+))?$")?,
            chat_fork_regex: Regex::new(r"^/chat\s+fork\s+(.+)$")?,
            chat_merge_regex: Regex::new(r"^/chat\s+merge\s+(.+)$")?,
            provider_regex: Regex::new(r"^/provider\s+(\w+)$")?,
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
            "/history" => Some(Command::History),
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
                } else if let Some(caps) = self.copy_regex.captures(input) {
                    Some(Command::Copy(caps[1].parse().unwrap_or(1)))
                } else if let Some(caps) = self.save_regex.captures(input) {
                    Some(Command::Save(caps[1].parse().unwrap_or(1), caps[2].to_string()))
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
                    Some(Command::ChatSave(caps[1].to_string()))
                } else if let Some(caps) = self.chat_load_regex.captures(input) {
                    Some(Command::ChatLoad(caps[1].to_string()))
                } else if let Some(caps) = self.chat_delete_regex.captures(input) {
                    Some(Command::ChatDelete(caps[1].to_string()))
                } else if let Some(caps) = self.chat_continue_regex.captures(input) {
                    let name = caps.get(1).map(|m| m.as_str().to_string());
                    Some(Command::ChatContinue(name))
                } else if let Some(caps) = self.chat_fork_regex.captures(input) {
                    Some(Command::ChatFork(caps[1].to_string()))
                } else if let Some(caps) = self.chat_merge_regex.captures(input) {
                    Some(Command::ChatMerge(caps[1].to_string()))
                } else if let Some(caps) = self.provider_regex.captures(input) {
                    Some(Command::Provider(caps[1].to_string()))
                } else {
                    None
                }
            }
        }
    }
}
