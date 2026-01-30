# `/edit` Command Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add `/edit` command to spawn `$EDITOR` for composing new messages or editing existing messages in chat history.

**Architecture:** Extend command parser with `EditTarget` enum, add `open_in_editor()` helper function, implement handler in `handle_command()`. Uses temp file with `.md` extension for syntax highlighting.

**Tech Stack:** Rust stdlib (`std::process::Command`, `std::env`, `std::fs`), `tempfile` crate

---

### Task 1: Promote `tempfile` to Regular Dependency

**Files:**
- Modify: `Cargo.toml:32-33`

**Step 1: Update Cargo.toml**

Move `tempfile` from `[dev-dependencies]` to `[dependencies]`:

```toml
[dependencies]
# ... existing deps ...
tempfile = "3.20"

[dev-dependencies]
# tempfile line removed
```

**Step 2: Verify compilation**

Run: `nix develop -c sh -c "cargo check"`
Expected: Compiles successfully

**Step 3: Commit**

```bash
git add Cargo.toml
git commit -m "build: promote tempfile to regular dependency

Required for /edit command to create temp files for $EDITOR.

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>"
```

---

### Task 2: Add `EditTarget` Enum and Update Command Parsing

**Files:**
- Modify: `src/commands.rs:6-72` (Command enum)
- Modify: `src/commands.rs:95-140` (CommandParser struct)
- Modify: `src/commands.rs:404-451` (CommandParser::new)
- Modify: `src/commands.rs:453-640` (CommandParser::parse)
- Modify: `src/commands.rs:643-1050` (tests)

**Step 1: Add EditTarget enum after SaveType**

Insert after line 86 (`}`):

```rust
#[derive(Debug, Clone)]
pub enum EditTarget {
    NewMessage(String),  // /edit with optional prefix text
    User(usize),         // /edit N or /edit user N
    Agent(usize),        // /edit agent N
}
```

**Step 2: Update Command enum**

Change line 44 from:
```rust
    Edit(usize),
```
to:
```rust
    Edit(EditTarget),
```

**Step 3: Update CommandParser struct**

Change line 112 from:
```rust
    edit_regex: Regex,
```
to:
```rust
    edit_regex: Regex,
    edit_typed_regex: Regex,
```

**Step 4: Update CommandParser::new**

Change line 422 from:
```rust
            edit_regex: Regex::new(r"^/edit\s+(\d+)$")?,
```
to:
```rust
            edit_regex: Regex::new(r"^/edit(?:\s+(\d+))?$")?,
            edit_typed_regex: Regex::new(r"^/edit\s+(user|agent)\s+(\d+)$")?,
```

**Step 5: Update CommandParser::parse - add /edit handling**

Find lines 554-555:
```rust
                } else if let Some(caps) = self.edit_regex.captures(input) {
                    Some(Command::Edit(caps[1].parse().unwrap_or(1)))
```

Replace with:
```rust
                } else if let Some(caps) = self.edit_typed_regex.captures(input) {
                    let edit_type = match caps[1].as_ref() {
                        "agent" => EditTarget::Agent(caps[2].parse().unwrap_or(1)),
                        _ => EditTarget::User(caps[2].parse().unwrap_or(1)),
                    };
                    Some(Command::Edit(edit_type))
                } else if let Some(caps) = self.edit_regex.captures(input) {
                    match caps.get(1) {
                        Some(m) => Some(Command::Edit(EditTarget::User(m.as_str().parse().unwrap_or(1)))),
                        None => Some(Command::Edit(EditTarget::NewMessage(String::new()))),
                    }
```

**Step 6: Add test cases for edit command parsing**

Add after line 1048 (before the final `}`):

```rust
    #[test]
    fn test_edit_commands() {
        let parser = create_parser();

        // Test /edit (new message)
        if let Some(Command::Edit(target)) = parser.parse("/edit") {
            assert!(matches!(target, EditTarget::NewMessage(_)));
        } else {
            panic!("Expected Edit command");
        }

        // Test /edit N (user shorthand)
        if let Some(Command::Edit(target)) = parser.parse("/edit 3") {
            if let EditTarget::User(n) = target {
                assert_eq!(n, 3);
            } else {
                panic!("Expected User target");
            }
        } else {
            panic!("Expected Edit command");
        }

        // Test /edit user N
        if let Some(Command::Edit(target)) = parser.parse("/edit user 5") {
            if let EditTarget::User(n) = target {
                assert_eq!(n, 5);
            } else {
                panic!("Expected User target");
            }
        } else {
            panic!("Expected Edit command");
        }

        // Test /edit agent N
        if let Some(Command::Edit(target)) = parser.parse("/edit agent 2") {
            if let EditTarget::Agent(n) = target {
                assert_eq!(n, 2);
            } else {
                panic!("Expected Agent target");
            }
        } else {
            panic!("Expected Edit command");
        }
    }
```

**Step 7: Run tests**

Run: `nix develop -c sh -c "cargo test --lib commands::tests"`
Expected: All tests pass

**Step 8: Commit**

```bash
git add src/commands.rs
git commit -m "feat(commands): add EditTarget enum and update /edit parsing

Supports:
- /edit - new message composition
- /edit N - edit user message N (shorthand)
- /edit user N - edit user message N (explicit)
- /edit agent N - edit agent message N

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>"
```

---

### Task 3: Add `open_in_editor()` Helper Function

**Files:**
- Modify: `src/repl.rs:1-10` (imports)
- Modify: `src/repl.rs:660-680` (add helper function)

**Step 1: Add imports at top of repl.rs**

Find existing imports and add (if not already present):
```rust
use std::io::Write;
use tempfile::NamedTempFile;
```

**Step 2: Add open_in_editor function**

Insert after `get_session_display()` function (around line 678):

```rust
    fn open_in_editor(&self, initial_content: &str) -> Result<Option<String>> {
        // Get editor from environment, fallback to vi
        let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());

        // Create temp file with .md extension for syntax highlighting
        let mut temp_file = NamedTempFile::with_suffix(".md")?;

        // Write initial content
        temp_file.write_all(initial_content.as_bytes())?;
        temp_file.flush()?;

        let temp_path = temp_file.path().to_path_buf();

        // Spawn editor and wait for it to exit
        let status = std::process::Command::new(&editor)
            .arg(&temp_path)
            .status();

        match status {
            Ok(exit_status) => {
                if !exit_status.success() {
                    // Editor exited with error, but still try to read the file
                    // (user may have saved before the error)
                }

                // Read the edited content
                let content = std::fs::read_to_string(&temp_path)?;

                // Trim whitespace and check for empty (cancel)
                let trimmed = content.trim();
                if trimmed.is_empty() {
                    Ok(None)
                } else {
                    Ok(Some(trimmed.to_string()))
                }
            }
            Err(e) => {
                if e.kind() == std::io::ErrorKind::NotFound {
                    Err(anyhow::anyhow!(
                        "Editor '{}' not found. Set $EDITOR environment variable to your preferred editor.",
                        editor
                    ))
                } else {
                    Err(anyhow::anyhow!("Failed to launch editor '{}': {}", editor, e))
                }
            }
        }
        // temp_file is automatically deleted when dropped
    }
```

**Step 3: Verify compilation**

Run: `nix develop -c sh -c "cargo check"`
Expected: Compiles successfully

**Step 4: Commit**

```bash
git add src/repl.rs
git commit -m "feat(repl): add open_in_editor helper function

Spawns \$EDITOR (or vi) with temp file, returns edited content.
Empty content after trim = cancel (returns None).

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>"
```

---

### Task 4: Add Helper Functions to Get Message Index by Number

**Files:**
- Modify: `src/repl.rs` (add helper functions near get_user_message_by_number)

**Step 1: Add get_agent_message_index_by_number function**

Insert after `get_user_message_by_number` function (around line 697):

```rust
    fn get_agent_message_index_by_number(&self, agent_number: usize) -> Option<usize> {
        let mut agent_count = 0;
        for (i, msg) in self.session.messages.iter().enumerate() {
            if msg.message.role == "assistant" {
                agent_count += 1;
                if agent_count == agent_number {
                    return Some(i);
                }
            }
        }
        None
    }

    fn get_user_message_index_by_number(&self, user_number: usize) -> Option<usize> {
        let mut user_count = 0;
        for (i, msg) in self.session.messages.iter().enumerate() {
            if msg.message.role == "user" {
                user_count += 1;
                if user_count == user_number {
                    return Some(i);
                }
            }
        }
        None
    }
```

**Step 2: Verify compilation**

Run: `nix develop -c sh -c "cargo check"`
Expected: Compiles successfully

**Step 3: Commit**

```bash
git add src/repl.rs
git commit -m "feat(repl): add message index lookup helpers

Add get_agent_message_index_by_number and get_user_message_index_by_number
for finding message indices by their display number.

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>"
```

---

### Task 5: Implement `/edit` Command Handler

**Files:**
- Modify: `src/repl.rs:2573-2575` (replace catch-all with Edit handler)

**Step 1: Add EditTarget import**

Find the imports at top of repl.rs and add `EditTarget` to the commands import:
```rust
use crate::commands::{Command, CommandParser, CopyType, SaveType, SessionReference, EditTarget};
```

**Step 2: Replace catch-all handler with Edit implementation**

Find lines 2573-2575:
```rust
            _ => {
                self.ui.print_info(&format!("Command not yet implemented: {:?}", command));
            }
```

Replace with:
```rust
            Command::Edit(target) => {
                match target {
                    EditTarget::NewMessage(prefix) => {
                        match self.open_in_editor(&prefix) {
                            Ok(Some(content)) => {
                                // Queue the edited content for the user to review and send
                                self.queued_message = Some(content);
                                self.ui.print_info("Content ready - press Enter to review and send");
                            }
                            Ok(None) => {
                                self.ui.print_info("Edit cancelled (empty content)");
                            }
                            Err(e) => {
                                self.ui.print_error(&e.to_string());
                            }
                        }
                    }
                    EditTarget::User(user_number) => {
                        if let Some(idx) = self.get_user_message_index_by_number(user_number) {
                            let content = self.session.messages[idx].message.content.clone();
                            match self.open_in_editor(&content) {
                                Ok(Some(new_content)) => {
                                    self.session.messages[idx].message.content = new_content;
                                    self.ui.print_info(&format!("User {} updated", user_number));
                                }
                                Ok(None) => {
                                    self.ui.print_info("Edit cancelled (empty content)");
                                }
                                Err(e) => {
                                    self.ui.print_error(&e.to_string());
                                }
                            }
                        } else {
                            self.ui.print_error(&format!("User {} not found", user_number));
                        }
                    }
                    EditTarget::Agent(agent_number) => {
                        if let Some(idx) = self.get_agent_message_index_by_number(agent_number) {
                            let content = self.session.messages[idx].message.content.clone();
                            match self.open_in_editor(&content) {
                                Ok(Some(new_content)) => {
                                    self.session.messages[idx].message.content = new_content;
                                    self.ui.print_info(&format!("Agent {} updated", agent_number));
                                }
                                Ok(None) => {
                                    self.ui.print_info("Edit cancelled (empty content)");
                                }
                                Err(e) => {
                                    self.ui.print_error(&e.to_string());
                                }
                            }
                        } else {
                            self.ui.print_error(&format!("Agent {} not found", agent_number));
                        }
                    }
                }
            }
```

**Step 3: Verify compilation**

Run: `nix develop -c sh -c "cargo check"`
Expected: Compiles successfully

**Step 4: Run all tests**

Run: `nix develop -c sh -c "cargo test"`
Expected: All tests pass

**Step 5: Commit**

```bash
git add src/repl.rs
git commit -m "feat(repl): implement /edit command handler

- /edit opens editor for new message, queues result for review
- /edit N or /edit user N edits user message in place
- /edit agent N edits agent message in place
- Empty result (after trim) = cancel, no changes made

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>"
```

---

### Task 6: Handle Prefix Text for `/edit` Command

**Files:**
- Modify: `src/repl.rs` (in run loop, detect `/edit` and strip prefix)

**Step 1: Find the command parsing section in run()**

Around line 469, find where input is checked for commands. We need to detect `/edit` with prefix text.

The current flow is:
1. User types "some text /edit"
2. Parser sees "/edit" at end but doesn't extract prefix

We need special handling before the parser is called.

**Step 2: Add prefix detection before command parsing**

Find (around line 469):
```rust
                if input.starts_with('/') {
```

Replace with:
```rust
                // Special handling for /edit with prefix text
                // e.g., "Hello world /edit" -> opens editor with "Hello world"
                if let Some(prefix) = input.strip_suffix("/edit").map(|s| s.trim_end()) {
                    if !prefix.is_empty() || input == "/edit" {
                        let prefix_text = if prefix.is_empty() { String::new() } else { prefix.to_string() };
                        self.handle_command(Command::Edit(EditTarget::NewMessage(prefix_text))).await?;
                        continue;
                    }
                }

                if input.starts_with('/') {
```

**Step 3: Verify compilation**

Run: `nix develop -c sh -c "cargo check"`
Expected: Compiles successfully

**Step 4: Run all tests**

Run: `nix develop -c sh -c "cargo test"`
Expected: All tests pass

**Step 5: Commit**

```bash
git add src/repl.rs
git commit -m "feat(repl): support prefix text with /edit command

'Hello world /edit' opens editor pre-seeded with 'Hello world'.
'/edit' alone opens editor with empty content.

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>"
```

---

### Task 7: Update Help Text

**Files:**
- Modify: `src/ui.rs` (in print_help function)

**Step 1: Find /edit in help text**

Search for existing `/edit` help text and update it.

In `print_help()` function, find lines mentioning edit and update to:
```rust
        println!("  /edit - Open $EDITOR to compose a new message");
        println!("  /edit N - Edit user message N in $EDITOR (modifies history)");
        println!("  /edit user N - Edit user message N in $EDITOR (explicit)");
        println!("  /edit agent N - Edit agent message N in $EDITOR");
```

**Step 2: Verify help displays correctly**

Run: `nix develop -c sh -c "cargo run -- --help"` (or check the help function compiles)
Expected: Compiles, help text is updated

**Step 3: Commit**

```bash
git add src/ui.rs
git commit -m "docs(help): update /edit command help text

Document all /edit variants and their behavior.

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>"
```

---

### Task 8: Final Integration Test

**Step 1: Build release binary**

Run: `nix develop -c sh -c "cargo build --release"`
Expected: Builds successfully

**Step 2: Run all tests one final time**

Run: `nix develop -c sh -c "cargo test"`
Expected: All tests pass

**Step 3: Manual smoke test (optional)**

Launch the REPL and test:
1. `/edit` - should open editor, on save content appears at prompt
2. Type "Hello /edit" - should open editor with "Hello" pre-filled
3. After sending a message, `/edit 1` - should open editor with user message 1
4. `/edit agent 1` - should open editor with agent message 1
5. Save empty file in editor - should cancel the operation

**Step 4: Final commit (if any cleanup needed)**

If all tests pass, the feature is complete.
