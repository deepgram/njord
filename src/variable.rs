use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Source of a variable's value
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum VariableSource {
    /// Static literal value
    Literal(String),
    /// Read from file
    File(PathBuf),
    /// Execute shell command
    Command {
        cmd: String,
        #[serde(default = "default_timeout")]
        timeout_secs: u64,
    },
}

fn default_timeout() -> u64 {
    30
}

impl VariableSource {
    /// Parse a source string with prefix (=, @, !)
    pub fn parse(input: &str) -> Result<Self> {
        if let Some(value) = input.strip_prefix('=') {
            Ok(VariableSource::Literal(value.to_string()))
        } else if let Some(path) = input.strip_prefix('@') {
            Ok(VariableSource::File(PathBuf::from(path)))
        } else if let Some(cmd) = input.strip_prefix('!') {
            Ok(VariableSource::Command {
                cmd: cmd.to_string(),
                timeout_secs: default_timeout(),
            })
        } else {
            Err(anyhow!(
                "Missing source prefix. Use:\n  \
                 =text    - literal value\n  \
                 @path    - file contents\n  \
                 !cmd     - command output"
            ))
        }
    }

    /// Create a command source with custom timeout
    pub fn command_with_timeout(cmd: String, timeout_secs: u64) -> Self {
        VariableSource::Command { cmd, timeout_secs }
    }

    /// Get a display string for the source type
    pub fn type_indicator(&self) -> &'static str {
        match self {
            VariableSource::Literal(_) => "=",
            VariableSource::File(_) => "@",
            VariableSource::Command { .. } => "!",
        }
    }

    /// Get a short description for display
    pub fn display_source(&self) -> String {
        match self {
            VariableSource::Literal(s) => {
                if s.len() > 20 {
                    format!("{}...", &s[..20])
                } else {
                    s.clone()
                }
            }
            VariableSource::File(p) => p.display().to_string(),
            VariableSource::Command { cmd, .. } => {
                if cmd.len() > 30 {
                    format!("{}...", &cmd[..30])
                } else {
                    cmd.clone()
                }
            }
        }
    }
}

/// A variable with its source and frozen state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Variable {
    pub name: String,
    pub source: VariableSource,
    #[serde(default)]
    pub frozen_value: Option<String>,
}

impl Variable {
    pub fn new(name: String, source: VariableSource) -> Self {
        Self {
            name,
            source,
            frozen_value: None,
        }
    }

    pub fn is_frozen(&self) -> bool {
        self.frozen_value.is_some()
    }

    pub fn freeze(&mut self, value: String) {
        self.frozen_value = Some(value);
    }

    pub fn unfreeze(&mut self) {
        self.frozen_value = None;
    }

    /// Get status string for display
    pub fn status(&self) -> &'static str {
        match (&self.source, self.is_frozen()) {
            (VariableSource::Literal(_), _) => "[static]",
            (_, true) => "[frozen]",
            (_, false) => "[live]",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_source_literal() {
        let source = VariableSource::parse("=hello world").unwrap();
        assert!(matches!(source, VariableSource::Literal(s) if s == "hello world"));
    }

    #[test]
    fn test_parse_source_file() {
        let source = VariableSource::parse("@src/main.rs").unwrap();
        assert!(matches!(source, VariableSource::File(p) if p == std::path::PathBuf::from("src/main.rs")));
    }

    #[test]
    fn test_parse_source_command() {
        let source = VariableSource::parse("!echo hello").unwrap();
        assert!(matches!(source, VariableSource::Command { cmd, timeout_secs } if cmd == "echo hello" && timeout_secs == 30));
    }

    #[test]
    fn test_parse_source_no_prefix_error() {
        let result = VariableSource::parse("no_prefix");
        assert!(result.is_err());
    }

    #[test]
    fn test_variable_new() {
        let var = Variable::new("test".to_string(), VariableSource::Literal("value".to_string()));
        assert_eq!(var.name, "test");
        assert!(!var.is_frozen());
    }

    #[test]
    fn test_variable_freeze_unfreeze() {
        let mut var = Variable::new("test".to_string(), VariableSource::Literal("value".to_string()));
        assert!(!var.is_frozen());

        var.freeze("frozen_value".to_string());
        assert!(var.is_frozen());
        assert_eq!(var.frozen_value, Some("frozen_value".to_string()));

        var.unfreeze();
        assert!(!var.is_frozen());
        assert_eq!(var.frozen_value, None);
    }
}
