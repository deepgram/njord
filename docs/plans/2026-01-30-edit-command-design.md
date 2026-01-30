# `/edit` Command Design

## Overview

Add an `/edit` command to spawn `$EDITOR` for composing new messages or editing existing messages in the chat history.

## Command Syntax

| Command | Description |
|---------|-------------|
| `/edit` | Open `$EDITOR` for new message composition |
| `/edit N` | Edit user message N in history (shorthand) |
| `/edit user N` | Edit user message N in history (explicit) |
| `/edit agent N` | Edit agent message N in history |

## Behavior

### `/edit` (New Message Composition)

1. Strip `/edit` from end of current input line; remaining text becomes `initial_content`
2. Open `$EDITOR` with temp file pre-seeded with `initial_content`
3. On editor close:
   - If temp file empty (after whitespace trim): cancel, return to prompt with original content
   - Otherwise: pre-fill readline buffer with edited content for user to review and send

The edited content does NOT auto-send. User must review and press Enter.

### `/edit [user|agent] N` (Edit Existing Message)

1. Look up message N in session history
2. If not found: print "Message N not found", return
3. Open `$EDITOR` with temp file pre-seeded with message content
4. On editor close:
   - If temp file empty (after whitespace trim): print "Edit cancelled", leave history unchanged
   - Otherwise: update message content in-place in history, print confirmation

Key behaviors:
- No message sent to LLM
- Current readline prompt unchanged
- History mutated in-place (affects `/save`, display, etc.)
- Editing a message does not affect subsequent messages in history

## Implementation

### Data Structures (`commands.rs`)

```rust
pub enum EditTarget {
    NewMessage,           // /edit (no args)
    User(usize),          // /edit N or /edit user N
    Agent(usize),         // /edit agent N
}

pub enum Command {
    // ...
    Edit(EditTarget),
    // ...
}
```

### Core Editor Function (`repl.rs`)

```rust
fn open_in_editor(initial_content: &str) -> Result<Option<String>>
```

1. Get editor: `$EDITOR` env var, fallback to `vi`
2. Create temp file with `.md` extension (for syntax highlighting)
3. Write `initial_content` to temp file
4. Spawn editor with temp file path, wait for exit
5. Read temp file contents
6. Delete temp file
7. Trim whitespace; if empty, return `Ok(None)` (cancel)
8. Otherwise return `Ok(Some(content))`

### Error Handling

- `$EDITOR` not set and `vi` not found: Print helpful error suggesting to set `$EDITOR`
- Temp file creation fails: Propagate error with context
- Editor exits with non-zero status: Still read file (user may have saved)
- Message N not found: Print "Message N not found"

### Cancel Detection

Empty temp file (after whitespace trim) = cancel/abort:
- For `/edit`: return to prompt unchanged
- For `/edit N`: leave history unmodified

## Files to Modify

1. `src/commands.rs` - Add `EditTarget` enum, update `Command::Edit`, update parser regex
2. `src/repl.rs` - Add `open_in_editor()` helper, implement handler for `Edit(EditTarget)`
3. `Cargo.toml` - Promote `tempfile` from dev-dependency to regular dependency (if needed)

## Testing

- Command parsing tests for all `/edit` variants
- Integration testing with `$EDITOR=cat` or simple test scripts
- Whitespace-trim and empty-detection logic tested separately
