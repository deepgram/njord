# Live Variables Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace the current variable system with one that stores templates, evaluates sources live, and supports literals, files, and commands.

**Architecture:** New `VariableSource` enum with three variants (Literal, File, Command). Variables store source definitions, not evaluated content. Substitution happens at LLM send time. Session stores `HashMap<String, Variable>` instead of `HashMap<String, String>`.

**Tech Stack:** Rust, serde for serialization, std::process::Command for shell execution, tokio for async timeout handling.

---

## Task 1: Define Variable Data Structures

**Files:**
- Create: `src/variable.rs`
- Modify: `src/main.rs` (add module)

**Step 1: Write tests for Variable struct**

In `src/variable.rs`:

```rust
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
```

**Step 2: Run tests to verify they fail**

Run: `nix develop -c sh -c "cargo test variable::tests --no-run" 2>&1`
Expected: Compilation error (module doesn't exist)

**Step 3: Implement Variable types**

In `src/variable.rs`:

```rust
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
```

**Step 4: Add module to main.rs**

In `src/main.rs`, add with other module declarations:

```rust
mod variable;
```

**Step 5: Run tests to verify they pass**

Run: `nix develop -c sh -c "cargo test variable::tests"`
Expected: All 6 tests pass

**Step 6: Commit**

```bash
git add src/variable.rs src/main.rs
git commit -m "feat(variables): add Variable and VariableSource types

New data structures for the live variables feature:
- VariableSource enum: Literal, File, Command variants
- Variable struct with name, source, and frozen_value
- Source parsing with =, @, ! prefixes"
```

---

## Task 2: Implement Variable Evaluation

**Files:**
- Modify: `src/variable.rs`

**Step 1: Write tests for evaluation**

Add to `src/variable.rs` tests:

```rust
    #[test]
    fn test_evaluate_literal() {
        let source = VariableSource::Literal("hello".to_string());
        let result = source.evaluate_sync().unwrap();
        assert_eq!(result, "hello");
    }

    #[test]
    fn test_evaluate_file() {
        // Create a temp file
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        std::fs::write(&file_path, "file content").unwrap();

        let source = VariableSource::File(file_path);
        let result = source.evaluate_sync().unwrap();
        assert_eq!(result, "file content");
    }

    #[test]
    fn test_evaluate_file_not_found() {
        let source = VariableSource::File(PathBuf::from("/nonexistent/path.txt"));
        let result = source.evaluate_sync();
        assert!(result.is_err());
    }

    #[test]
    fn test_evaluate_command() {
        let source = VariableSource::Command {
            cmd: "echo hello".to_string(),
            timeout_secs: 5,
        };
        let result = source.evaluate_sync().unwrap();
        assert_eq!(result.trim(), "hello");
    }

    #[test]
    fn test_evaluate_command_non_zero_exit() {
        let source = VariableSource::Command {
            cmd: "sh -c 'echo output; exit 1'".to_string(),
            timeout_secs: 5,
        };
        // Should still return output even with non-zero exit
        let result = source.evaluate_sync();
        assert!(result.is_ok());
        assert!(result.unwrap().contains("output"));
    }
```

**Step 2: Run tests to verify they fail**

Run: `nix develop -c sh -c "cargo test variable::tests::test_evaluate --no-run" 2>&1`
Expected: Compilation error (evaluate_sync doesn't exist)

**Step 3: Implement evaluate_sync**

Add to `VariableSource` impl in `src/variable.rs`:

```rust
    /// Evaluate the source synchronously, returning the value
    pub fn evaluate_sync(&self) -> Result<String> {
        match self {
            VariableSource::Literal(value) => Ok(value.clone()),
            VariableSource::File(path) => {
                std::fs::read_to_string(path)
                    .map_err(|e| anyhow!("Failed to read file '{}': {}", path.display(), e))
            }
            VariableSource::Command { cmd, timeout_secs } => {
                self.execute_command(cmd, *timeout_secs)
            }
        }
    }

    fn execute_command(&self, cmd: &str, timeout_secs: u64) -> Result<String> {
        use std::process::{Command, Stdio};
        use std::time::Duration;

        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());

        let mut child = Command::new(&shell)
            .arg("-c")
            .arg(cmd)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| anyhow!("Failed to spawn command '{}': {}", cmd, e))?;

        // Wait with timeout using a simple polling approach
        let start = std::time::Instant::now();
        let timeout = Duration::from_secs(timeout_secs);

        loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    // Process finished
                    let output = child.wait_with_output()
                        .map_err(|e| anyhow!("Failed to read command output: {}", e))?;

                    let stdout = String::from_utf8_lossy(&output.stdout).to_string();

                    // Non-zero exit is a warning, not an error - we still return the output
                    if !status.success() {
                        // The warning will be printed by the caller
                    }

                    return Ok(stdout);
                }
                Ok(None) => {
                    // Still running
                    if start.elapsed() > timeout {
                        let _ = child.kill();
                        return Err(anyhow!("Command '{}' timed out after {}s", cmd, timeout_secs));
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(e) => {
                    return Err(anyhow!("Error waiting for command: {}", e));
                }
            }
        }
    }
```

Also add `tempfile` to dev-dependencies. Check if it exists first.

**Step 4: Run tests to verify they pass**

Run: `nix develop -c sh -c "cargo test variable::tests::test_evaluate"`
Expected: All 5 evaluation tests pass

**Step 5: Commit**

```bash
git add src/variable.rs
git commit -m "feat(variables): implement source evaluation

- Literal: returns value directly
- File: reads from filesystem
- Command: executes via \$SHELL with timeout
- Non-zero exit codes return output (warning only)"
```

---

## Task 3: Update Session to Use New Variable Type

**Files:**
- Modify: `src/session.rs`
- Modify: `src/variable.rs` (add use statement)

**Step 1: Write migration test**

Add to `src/session.rs` tests:

```rust
    #[test]
    fn test_variable_bindings_migration() {
        // Old format: HashMap<String, String> (filename -> var_name)
        let json = r#"{
            "id": "00000000-0000-0000-0000-000000000000",
            "created_at": "2024-01-01T00:00:00Z",
            "updated_at": "2024-01-01T00:00:00Z",
            "messages": [],
            "current_model": "gpt-4",
            "temperature": 0.7,
            "max_tokens": 1000,
            "thinking_budget": 5000,
            "thinking_enabled": false,
            "variable_bindings": {
                "src/main.rs": "code"
            }
        }"#;

        let session: ChatSession = serde_json::from_str(json).unwrap();
        assert!(session.variables.contains_key("code"));
        let var = session.variables.get("code").unwrap();
        assert!(matches!(&var.source, crate::variable::VariableSource::File(p) if p == &std::path::PathBuf::from("src/main.rs")));
    }
```

**Step 2: Run test to verify it fails**

Run: `nix develop -c sh -c "cargo test session::tests::test_variable_bindings_migration --no-run" 2>&1`
Expected: Compilation error (session.variables doesn't exist)

**Step 3: Update ChatSession struct**

In `src/session.rs`, replace:

```rust
    #[serde(default)]
    pub variable_bindings: std::collections::HashMap<String, String>, // filename -> variable_name
```

With:

```rust
    #[serde(default, deserialize_with = "deserialize_variables", alias = "variable_bindings")]
    pub variables: std::collections::HashMap<String, crate::variable::Variable>,
```

Add the deserialize function and imports at the top of the file:

```rust
use crate::variable::{Variable, VariableSource};

fn deserialize_variables<'de, D>(deserializer: D) -> Result<std::collections::HashMap<String, Variable>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Error;

    // Try to deserialize as new format first
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum VariablesFormat {
        New(std::collections::HashMap<String, Variable>),
        Old(std::collections::HashMap<String, String>),
    }

    match VariablesFormat::deserialize(deserializer)? {
        VariablesFormat::New(vars) => Ok(vars),
        VariablesFormat::Old(old_bindings) => {
            // Migrate old format: filename -> var_name becomes var_name -> Variable with @filename source
            let mut new_vars = std::collections::HashMap::new();
            for (filename, var_name) in old_bindings {
                let var = Variable::new(
                    var_name.clone(),
                    VariableSource::File(std::path::PathBuf::from(filename)),
                );
                new_vars.insert(var_name, var);
            }
            Ok(new_vars)
        }
    }
}
```

Update `ChatSession::new()` to use `variables: std::collections::HashMap::new()`.

Update `create_copy()` to copy `variables` instead of `variable_bindings`.

**Step 4: Run tests to verify they pass**

Run: `nix develop -c sh -c "cargo test session::tests"`
Expected: All session tests pass including migration test

**Step 5: Commit**

```bash
git add src/session.rs
git commit -m "feat(session): migrate to new Variable type

- Replace variable_bindings HashMap<String, String> with variables HashMap<String, Variable>
- Add deserialize_variables for backward compatibility with old sessions
- Old format (filename -> var_name) auto-converts to new format"
```

---

## Task 4: Update Command Parsing for New /load Syntax

**Files:**
- Modify: `src/commands.rs`

**Step 1: Write tests for new syntax**

Add to `src/commands.rs` tests:

```rust
    #[test]
    fn test_load_command_with_prefixes() {
        let parser = CommandParser::new().unwrap();

        // Literal
        if let Some(Command::Load(source, var_name)) = parser.parse("/load \"=hello world\" greeting") {
            assert_eq!(source, "=hello world");
            assert_eq!(var_name, Some("greeting".to_string()));
        } else {
            panic!("Failed to parse literal load command");
        }

        // File
        if let Some(Command::Load(source, var_name)) = parser.parse("/load \"@src/main.rs\" code") {
            assert_eq!(source, "@src/main.rs");
            assert_eq!(var_name, Some("code".to_string()));
        } else {
            panic!("Failed to parse file load command");
        }

        // Command
        if let Some(Command::Load(source, var_name)) = parser.parse("/load \"!git diff\" changes") {
            assert_eq!(source, "!git diff");
            assert_eq!(var_name, Some("changes".to_string()));
        } else {
            panic!("Failed to parse command load command");
        }
    }

    #[test]
    fn test_load_command_with_timeout() {
        let parser = CommandParser::new().unwrap();

        if let Some(Command::Load(source, var_name)) = parser.parse("/load \"!slow cmd\" x --timeout 60") {
            assert_eq!(source, "!slow cmd");
            assert_eq!(var_name, Some("x".to_string()));
            // Note: timeout is parsed separately in repl.rs
        } else {
            panic!("Failed to parse load command with timeout");
        }
    }

    #[test]
    fn test_freeze_command() {
        let parser = CommandParser::new().unwrap();

        if let Some(Command::VariableFreeze(var_name)) = parser.parse("/freeze myvar") {
            assert_eq!(var_name, "myvar");
        } else {
            panic!("Failed to parse freeze command");
        }
    }
```

**Step 2: Run tests to verify they fail**

Run: `nix develop -c sh -c "cargo test commands::tests::test_load_command_with_prefixes --no-run" 2>&1`
Expected: Tests compile but may fail (existing parser should handle basic case)

**Step 3: Add VariableFreeze command and update parsing**

In `src/commands.rs`, add to the Command enum:

```rust
    VariableFreeze(String), // /freeze VAR
```

Add regex for freeze command in CommandParser struct:

```rust
    freeze_regex: Regex,
```

Initialize in `CommandParser::new()`:

```rust
    freeze_regex: Regex::new(r"^/freeze\s+(\S+)$")?,
```

Add parsing for /freeze in the parse method (near other variable commands):

```rust
    if let Some(caps) = self.freeze_regex.captures(input) {
        return Some(Command::VariableFreeze(caps[1].to_string()));
    }
```

**Step 4: Run tests to verify they pass**

Run: `nix develop -c sh -c "cargo test commands::tests::test_freeze_command"`
Expected: All new command tests pass

**Step 5: Commit**

```bash
git add src/commands.rs
git commit -m "feat(commands): add /freeze command and update /load parsing

- Add VariableFreeze command variant
- /freeze VAR toggles frozen state
- /load now accepts source strings with =, @, ! prefixes"
```

---

## Task 5: Update Repl Variable Storage and /load Handler

**Files:**
- Modify: `src/repl.rs`

**Step 1: Write integration test for /load with new syntax**

This will be tested manually since the Repl requires full async runtime. First, update the code.

**Step 2: Update Repl struct and /load handler**

In `src/repl.rs`:

Replace the variables field:
```rust
    variables: HashMap<String, String>, // For file content variables
```
With:
```rust
    variables: HashMap<String, crate::variable::Variable>,
```

Update the Command::Load handler (around line 2568):

```rust
            Command::Load(source_str, variable_name_opt) => {
                use crate::variable::{Variable, VariableSource};

                // Parse the source string
                let (source, timeout_override) = Self::parse_load_source_and_timeout(&source_str);

                match VariableSource::parse(&source) {
                    Ok(mut var_source) => {
                        // Apply timeout override for commands
                        if let (VariableSource::Command { ref mut timeout_secs, .. }, Some(t)) = (&mut var_source, timeout_override) {
                            *timeout_secs = t;
                        }

                        // Generate or use provided variable name
                        let variable_name = variable_name_opt.unwrap_or_else(|| {
                            self.generate_variable_name_from_source(&var_source)
                        });

                        // Evaluate to show preview and validate source works
                        match var_source.evaluate_sync() {
                            Ok(content) => {
                                let preview = if content.len() > 100 {
                                    format!("{}...", &content[..100].replace('\n', " "))
                                } else {
                                    content.replace('\n', " ")
                                };

                                let var = Variable::new(variable_name.clone(), var_source.clone());
                                self.variables.insert(variable_name.clone(), var.clone());
                                self.session.variables.insert(variable_name.clone(), var);

                                self.ui.print_info(&format!(
                                    "Loaded {} as {{{{{}}}}} ({} chars): {}",
                                    var_source.display_source(),
                                    variable_name,
                                    content.len(),
                                    preview
                                ));

                                let _ = self.update_completion_context();
                            }
                            Err(e) => {
                                self.ui.print_error(&format!("Failed to load: {}", e));
                            }
                        }
                    }
                    Err(e) => {
                        self.ui.print_error(&format!("{}", e));
                    }
                }
            }
```

Add helper function:

```rust
    fn parse_load_source_and_timeout(source_str: &str) -> (String, Option<u64>) {
        // Check for --timeout flag
        if let Some(idx) = source_str.find("--timeout") {
            let source = source_str[..idx].trim().to_string();
            let timeout_part = source_str[idx..].trim();
            if let Some(timeout_str) = timeout_part.strip_prefix("--timeout") {
                if let Ok(timeout) = timeout_str.trim().parse::<u64>() {
                    return (source, Some(timeout));
                }
            }
            (source, None)
        } else {
            (source_str.to_string(), None)
        }
    }

    fn generate_variable_name_from_source(&self, source: &crate::variable::VariableSource) -> String {
        use crate::variable::VariableSource;

        let base_name = match source {
            VariableSource::Literal(s) => {
                // Use first word or "literal"
                s.split_whitespace()
                    .next()
                    .unwrap_or("literal")
                    .chars()
                    .filter(|c| c.is_alphanumeric() || *c == '_')
                    .take(20)
                    .collect::<String>()
                    .to_lowercase()
            }
            VariableSource::File(path) => {
                self.generate_variable_name_from_filename(&path.display().to_string())
            }
            VariableSource::Command { cmd, .. } => {
                // Use first word of command
                cmd.split_whitespace()
                    .next()
                    .unwrap_or("cmd")
                    .chars()
                    .filter(|c| c.is_alphanumeric() || *c == '_')
                    .take(20)
                    .collect::<String>()
                    .to_lowercase()
            }
        };

        // Ensure uniqueness
        let mut name = if base_name.is_empty() { "var".to_string() } else { base_name };
        let mut counter = 1;
        while self.variables.contains_key(&name) {
            name = format!("{}_{}", name.trim_end_matches(char::is_numeric).trim_end_matches('_'), counter);
            counter += 1;
        }
        name
    }
```

**Step 3: Run tests to verify compilation**

Run: `nix develop -c sh -c "cargo build" 2>&1`
Expected: Compiles (may have warnings)

**Step 4: Commit**

```bash
git add src/repl.rs
git commit -m "feat(repl): update /load to use new Variable system

- Parse source strings with =, @, ! prefixes
- Support --timeout flag for command sources
- Store Variable objects instead of raw strings
- Generate variable names from source type"
```

---

## Task 6: Implement /freeze Command Handler

**Files:**
- Modify: `src/repl.rs`

**Step 1: Add /freeze handler**

In `src/repl.rs`, add handler for VariableFreeze (near other variable commands):

```rust
            Command::VariableFreeze(var_name) => {
                if let Some(var) = self.variables.get_mut(&var_name) {
                    if var.is_frozen() {
                        // Unfreeze
                        var.unfreeze();
                        if let Some(session_var) = self.session.variables.get_mut(&var_name) {
                            session_var.unfreeze();
                        }
                        self.ui.print_info(&format!("Variable '{{{{{}}}}}' unfrozen (now live)", var_name));
                    } else {
                        // Freeze with current value
                        match var.source.evaluate_sync() {
                            Ok(value) => {
                                let size = value.len();
                                var.freeze(value.clone());
                                if let Some(session_var) = self.session.variables.get_mut(&var_name) {
                                    session_var.freeze(value);
                                }
                                self.ui.print_info(&format!(
                                    "Variable '{{{{{}}}}}' frozen ({} bytes captured)",
                                    var_name, size
                                ));
                            }
                            Err(e) => {
                                self.ui.print_error(&format!("Failed to freeze '{}': {}", var_name, e));
                            }
                        }
                    }
                } else {
                    self.ui.print_error(&format!("Variable '{}' not found", var_name));
                    if !self.variables.is_empty() {
                        self.ui.print_info("Available variables:");
                        for name in self.variables.keys() {
                            println!("  {}", name);
                        }
                    }
                }
            }
```

**Step 2: Run tests to verify compilation**

Run: `nix develop -c sh -c "cargo build" 2>&1`
Expected: Compiles

**Step 3: Commit**

```bash
git add src/repl.rs
git commit -m "feat(repl): implement /freeze command handler

- /freeze VAR toggles between frozen and live states
- Freezing captures current evaluated value
- Unfreezing returns to live evaluation"
```

---

## Task 7: Update Substitution to Evaluate Live Variables

**Files:**
- Modify: `src/repl.rs`

**Step 1: Rewrite substitute_variables**

Replace the `substitute_variables` function:

```rust
    fn substitute_variables(&self, input: &str) -> Result<String, Vec<(String, String)>> {
        use crate::variable::VariableSource;

        let mut result = input.to_string();
        let mut errors: Vec<(String, String)> = Vec::new();

        // First, substitute named variables {{varname}}
        for (var_name, var) in &self.variables {
            let pattern = format!("{{{{{}}}}}", var_name);
            if result.contains(&pattern) {
                let value = if let Some(frozen) = &var.frozen_value {
                    Ok(frozen.clone())
                } else {
                    var.source.evaluate_sync()
                };

                match value {
                    Ok(content) => {
                        result = result.replace(&pattern, &content);
                    }
                    Err(e) => {
                        errors.push((var_name.clone(), e.to_string()));
                    }
                }
            }
        }

        // Then, substitute inline patterns {{@path}}, {{!cmd}}, {{=literal}}
        let inline_regex = regex::Regex::new(r"\{\{([=@!][^}]+)\}\}").unwrap();
        let mut inline_errors: Vec<(String, String)> = Vec::new();

        // Collect all matches first to avoid borrow issues
        let matches: Vec<(String, String)> = inline_regex
            .captures_iter(&result)
            .map(|cap| (cap[0].to_string(), cap[1].to_string()))
            .collect();

        for (full_match, source_str) in matches {
            match VariableSource::parse(&source_str) {
                Ok(source) => {
                    match source.evaluate_sync() {
                        Ok(content) => {
                            result = result.replace(&full_match, &content);
                        }
                        Err(e) => {
                            inline_errors.push((source_str, e.to_string()));
                        }
                    }
                }
                Err(e) => {
                    inline_errors.push((source_str, e.to_string()));
                }
            }
        }

        errors.extend(inline_errors);

        if errors.is_empty() {
            Ok(result)
        } else {
            Err(errors)
        }
    }
```

**Step 2: Update handle_message to handle substitution errors**

Update the call site in `handle_message`:

```rust
    async fn handle_message(&mut self, message: String) -> Result<()> {
        // Substitute variables in the message before processing
        let processed_message = match self.substitute_variables(&message) {
            Ok(msg) => {
                // Show substitution info if variables were replaced
                if msg != message {
                    self.ui.print_info("Variables substituted");
                }
                msg
            }
            Err(errors) => {
                // Interactive error handling
                for (var_name, error) in &errors {
                    self.ui.print_error(&format!("Variable '{}' failed: {}", var_name, error));
                }

                // Prompt user for action
                println!("  [s]kip (use empty) / [a]bort / [r]etry / [e]dit source?");

                let mut input = String::new();
                std::io::stdin().read_line(&mut input)?;

                match input.trim().to_lowercase().as_str() {
                    "s" | "skip" => {
                        // Replace failed variables with empty string
                        let mut result = message.clone();
                        for (var_name, _) in &errors {
                            let pattern = format!("{{{{{}}}}}", var_name);
                            result = result.replace(&pattern, "");
                        }
                        result
                    }
                    "a" | "abort" => {
                        self.ui.print_info("Request aborted");
                        return Ok(());
                    }
                    "r" | "retry" => {
                        // Recursively retry
                        return Box::pin(self.handle_message(message)).await;
                    }
                    "e" | "edit" => {
                        // Queue for editing
                        self.queued_message = Some(message);
                        self.ui.print_info("Message queued for editing. Use /edit to modify.");
                        return Ok(());
                    }
                    _ => {
                        self.ui.print_info("Invalid choice. Aborting.");
                        return Ok(());
                    }
                }
            }
        };

        // ... rest of the function unchanged
```

**Step 3: Run tests to verify compilation**

Run: `nix develop -c sh -c "cargo build" 2>&1`
Expected: Compiles

**Step 4: Commit**

```bash
git add src/repl.rs
git commit -m "feat(repl): implement live variable evaluation

- substitute_variables now evaluates sources on each call
- Frozen variables use cached value
- Inline syntax ({{@path}}, {{!cmd}}) evaluated inline
- Interactive error handling: skip/abort/retry/edit"
```

---

## Task 8: Update /vars Display

**Files:**
- Modify: `src/repl.rs`

**Step 1: Update Command::Variables handler**

Replace the Variables command handler:

```rust
            Command::Variables => {
                if self.variables.is_empty() {
                    self.ui.print_info("No variables loaded");
                } else {
                    self.ui.print_info(&format!("Variables ({} total):", self.variables.len()));
                    println!();

                    for (name, var) in &self.variables {
                        // Get current size (evaluate or use frozen)
                        let size = if let Some(frozen) = &var.frozen_value {
                            frozen.len()
                        } else {
                            var.source.evaluate_sync().map(|s| s.len()).unwrap_or(0)
                        };

                        println!(
                            "  {:<12} {}{:<30} {:>8} ({} bytes)",
                            format!("{{{{{}}}}}", name),
                            var.source.type_indicator(),
                            var.source.display_source(),
                            var.status(),
                            size
                        );
                    }

                    println!();
                    self.ui.print_info("Use {{VARIABLE_NAME}} in messages to substitute content");
                    self.ui.print_info("Use /freeze VAR to toggle frozen state");
                }
            }
```

**Step 2: Run tests to verify compilation**

Run: `nix develop -c sh -c "cargo build" 2>&1`
Expected: Compiles

**Step 3: Commit**

```bash
git add src/repl.rs
git commit -m "feat(repl): update /vars display with new format

Shows: name, source type indicator, source path/cmd, status, size
Example: {{code}} @src/main.rs [live] (12847 bytes)"
```

---

## Task 9: Update /var reload for Frozen Variables

**Files:**
- Modify: `src/repl.rs`

**Step 1: Update reload_specific_variable**

Replace the function:

```rust
    fn reload_specific_variable(&mut self, var_name: &str) {
        if let Some(var) = self.variables.get_mut(var_name) {
            if !var.is_frozen() {
                self.ui.print_info(&format!(
                    "Variable '{{{{{}}}}}' is live (not frozen). No reload needed.",
                    var_name
                ));
                return;
            }

            // Re-evaluate and update frozen value
            match var.source.evaluate_sync() {
                Ok(content) => {
                    let old_size = var.frozen_value.as_ref().map(|s| s.len()).unwrap_or(0);
                    let new_size = content.len();
                    var.freeze(content.clone());

                    // Update session copy
                    if let Some(session_var) = self.session.variables.get_mut(var_name) {
                        session_var.freeze(content);
                    }

                    self.ui.print_info(&format!(
                        "Reloaded frozen variable '{{{{{}}}}}' ({} -> {} bytes)",
                        var_name, old_size, new_size
                    ));
                }
                Err(e) => {
                    self.ui.print_error(&format!(
                        "Failed to reload '{}': {}",
                        var_name, e
                    ));
                }
            }
        } else {
            self.ui.print_error(&format!("Variable '{}' not found", var_name));
            if !self.variables.is_empty() {
                self.ui.print_info("Available variables:");
                for name in self.variables.keys() {
                    println!("  {}", name);
                }
            }
        }
    }
```

**Step 2: Update reload_all_variables**

Replace the function:

```rust
    fn reload_all_variables(&mut self) {
        if self.variables.is_empty() {
            self.ui.print_info("No variables to reload");
            return;
        }

        let frozen_vars: Vec<String> = self.variables.iter()
            .filter(|(_, v)| v.is_frozen())
            .map(|(n, _)| n.clone())
            .collect();

        if frozen_vars.is_empty() {
            self.ui.print_info("No frozen variables to reload (live variables refresh automatically)");
            return;
        }

        let mut reloaded = 0;
        let mut failed = 0;

        for var_name in frozen_vars {
            if let Some(var) = self.variables.get_mut(&var_name) {
                match var.source.evaluate_sync() {
                    Ok(content) => {
                        var.freeze(content.clone());
                        if let Some(session_var) = self.session.variables.get_mut(&var_name) {
                            session_var.freeze(content);
                        }
                        reloaded += 1;
                    }
                    Err(e) => {
                        self.ui.print_error(&format!("Failed to reload '{}': {}", var_name, e));
                        failed += 1;
                    }
                }
            }
        }

        self.ui.print_info(&format!(
            "Reloaded {} frozen variable(s), {} failed",
            reloaded, failed
        ));
    }
```

**Step 3: Run tests to verify compilation**

Run: `nix develop -c sh -c "cargo build" 2>&1`
Expected: Compiles

**Step 4: Commit**

```bash
git add src/repl.rs
git commit -m "feat(repl): update /var reload for frozen variables only

- /var reload VAR: only works on frozen variables (live = no-op)
- /var reload: only reloads frozen variables
- Clear messaging about live vs frozen behavior"
```

---

## Task 10: Update /history with --expand Flag

**Files:**
- Modify: `src/commands.rs`
- Modify: `src/repl.rs`

**Step 1: Add History with expand option**

In `src/commands.rs`, update Command enum:

```rust
    History(bool), // (expand flag)
```

Add regex for history with flag:

```rust
    history_regex: Regex,
```

Initialize:

```rust
    history_regex: Regex::new(r"^/history(\s+--expand)?$")?,
```

Update parsing:

```rust
    if let Some(caps) = self.history_regex.captures(input) {
        let expand = caps.get(1).is_some();
        return Some(Command::History(expand));
    }
```

**Step 2: Update history display in repl.rs**

Update the Command::History handler to accept the expand flag and optionally expand variables:

```rust
            Command::History(expand) => {
                // ... existing header code ...

                // When displaying user messages, show with or without expansion
                let display_content = if expand {
                    match self.substitute_variables(&current_msg.message.content) {
                        Ok(expanded) => expanded,
                        Err(_) => current_msg.message.content.clone() // Fall back to original
                    }
                } else {
                    // Show original with variable placeholders
                    current_msg.message.content.clone()
                };

                // ... rest of display code using display_content ...
```

**Step 3: Run tests and verify**

Run: `nix develop -c sh -c "cargo build" 2>&1`
Expected: Compiles

**Step 4: Commit**

```bash
git add src/commands.rs src/repl.rs
git commit -m "feat(history): add --expand flag to /history

- /history shows messages with {{var}} placeholders (default)
- /history --expand shows fully substituted content"
```

---

## Task 11: Update restore_session_variables

**Files:**
- Modify: `src/repl.rs`

**Step 1: Update restore function**

Replace `restore_session_variables`:

```rust
    fn restore_session_variables(&mut self, session: &ChatSession) {
        self.variables.clear();

        for (name, var) in &session.variables {
            self.variables.insert(name.clone(), var.clone());

            // Validate source still works (for non-frozen vars)
            if !var.is_frozen() {
                if let Err(e) = var.source.evaluate_sync() {
                    self.ui.print_info(&format!(
                        "Warning: Variable '{{{{{}}}}}' source unavailable: {}",
                        name, e
                    ));
                }
            }
        }

        if !self.variables.is_empty() {
            self.ui.print_info(&format!("Restored {} variable(s)", self.variables.len()));
        }

        let _ = self.update_completion_context();
    }
```

**Step 2: Run tests to verify compilation**

Run: `nix develop -c sh -c "cargo build" 2>&1`
Expected: Compiles

**Step 3: Commit**

```bash
git add src/repl.rs
git commit -m "feat(repl): update session variable restoration

- Restore Variable objects from session
- Validate non-frozen sources on load
- Warn if source unavailable"
```

---

## Task 12: Update Help Text

**Files:**
- Modify: `src/repl.rs`

**Step 1: Update help text**

Find the help text section (around line 917) and update:

```rust
                println!("  /load SOURCE VAR   - Load variable from source");
                println!("    Sources: =literal, @filepath, !command");
                println!("    \x1b[1;32mEx:\x1b[0m /load \"=hello\" greeting");
                println!("    \x1b[1;32mEx:\x1b[0m /load \"@src/main.rs\" code");
                println!("    \x1b[1;32mEx:\x1b[0m /load \"!git diff\" changes");
                println!("    \x1b[1;32mEx:\x1b[0m /load \"!slow-cmd\" x --timeout 60");
                println!("  /freeze VAR        - Toggle frozen state on variable");
                println!("  /vars              - List loaded variables");
                println!("  /var show VAR      - Show variable content");
                println!("  /var reload [VAR]  - Reload frozen variable(s)");
                println!("  /var delete VAR    - Delete a variable");
                println!("  /history [--expand]- Show conversation history");
```

**Step 2: Run tests to verify compilation**

Run: `nix develop -c sh -c "cargo build" 2>&1`
Expected: Compiles

**Step 3: Commit**

```bash
git add src/repl.rs
git commit -m "docs(help): update help text for new variable system

- Document new /load syntax with prefixes
- Add /freeze command
- Note --expand flag for /history"
```

---

## Task 13: Run Full Test Suite

**Step 1: Run all tests**

Run: `nix develop -c sh -c "cargo test"`
Expected: All tests pass

**Step 2: Fix any failures**

Address any test failures from the changes.

**Step 3: Manual testing**

Test the following scenarios manually:
1. `/load "=hello" x` - literal variable
2. `/load "@Cargo.toml" cfg` - file variable
3. `/load "!echo test" out` - command variable
4. `/freeze cfg` - freeze a variable
5. `/vars` - check display
6. `{{cfg}}` in a message - substitution
7. `/history` vs `/history --expand`

**Step 4: Final commit**

```bash
git add -A
git commit -m "test: verify live variables implementation

All tests passing, manual testing complete"
```

---

## Summary

This implementation plan covers:

1. **Task 1-2**: New `Variable` and `VariableSource` types with evaluation
2. **Task 3**: Session migration to new variable format
3. **Task 4-6**: Command parsing and /freeze handler
4. **Task 7**: Live substitution with error handling
5. **Task 8-9**: Updated /vars and /var reload
6. **Task 10**: /history --expand flag
7. **Task 11-12**: Session restore and help text
8. **Task 13**: Testing and verification

Each task is designed to be independently committable and testable.
