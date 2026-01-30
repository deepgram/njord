# Live Variables Design

## Overview

Redesign the variable system to support live evaluation from multiple source types, template-based message storage, and frozen snapshots.

## Goals

1. **Template storage** — Store `{{var}}` placeholders in history, not expanded content
2. **Live evaluation** — Variables re-evaluate from their source on each LLM request
3. **Multiple source types** — Support literals, files, and shell commands
4. **Freeze control** — Allow snapshotting a variable's value when needed

## Source Types

All sources require a prefix:

| Prefix | Type | Description |
|--------|------|-------------|
| `=` | Literal | Static string value |
| `@` | File | Read contents from filesystem |
| `!` | Command | Execute shell command, use stdout |

### Examples

```
/load "=Analyze this code for bugs" prompt
/load "@src/main.rs" code
/load "!git diff --cached" changes
```

Unprefixed `/load` is an error with a helpful message showing valid syntax.

## Binding vs Inline Syntax

### Named Bindings

Created via `/load`, appear in `/vars`, can be frozen:

```
/load "@src/main.rs" code
/load "!git diff" changes --timeout 60
```

Referenced as `{{varname}}`:

```
Review this: {{code}}
```

### Inline Syntax

No binding created, always live, cannot be frozen:

```
Review this: {{@src/main.rs}}
Changes: {{!git diff --cached}}
Prompt: {{=Analyze for security issues}}
```

Inline is for quick one-offs. Use named bindings when you need freeze control.

## Evaluation Semantics

### Default: Live

File and command sources re-evaluate on each LLM request. This ensures the LLM always sees current data.

### Freeze Escape Hatch

`/freeze VAR` toggles frozen state:

- **Freezing**: Captures current value, stops re-evaluation
- **Unfreezing**: Returns to live evaluation

Frozen state persists across session save/load.

### Literals

Always static. Freezing a literal is a no-op.

## Command Execution

- **Shell**: Executed via `$SHELL -c "command"`
- **Working directory**: Where `njord` was launched
- **Timeout**: 30 seconds default, configurable via `--timeout N`
- **Exit codes**: Non-zero is a warning, not a failure. Stdout is still used.

### Warnings

```
Warning: Command 'git diff' exited with code 1
```

## Error Handling

Actual failures trigger an interactive prompt:

- File not found / permission denied
- Command not found
- Timeout exceeded
- Spawn failure

```
Variable 'code' failed: file not found: src/deleted.rs
  [s]kip (use empty) / [a]bort / [r]etry / [e]dit source?
```

| Option | Behavior |
|--------|----------|
| Skip | Substitute empty string, continue |
| Abort | Cancel the LLM request |
| Retry | Re-attempt evaluation |
| Edit | Modify the source definition |

## Commands

### New/Modified

| Command | Behavior |
|---------|----------|
| `/load "source" name` | Create binding (prefix required: `=`, `@`, `!`) |
| `/load "!cmd" name --timeout N` | Set command timeout in seconds |
| `/freeze VAR` | Toggle frozen state |
| `/vars` | List variables with source, status, size |
| `/var delete VAR` | Remove binding |
| `/var reload VAR` | Re-evaluate frozen variable (no-op if unfrozen) |
| `/history` | Show messages with `{{var}}` placeholders |
| `/history --expand` | Show fully expanded content |

### Variable List Display

```
Variables:
  code     @src/main.rs      [live]   (12,847 bytes)
  changes  !git diff         [frozen] (342 bytes)
  prompt   =Analyze this...  [static] (28 bytes)
```

## Storage

### Messages

Store the original template with `{{...}}` placeholders. Substitution happens at LLM send time only.

### Variables

Per-session, serialize as:

```json
{
  "name": "code",
  "source": "@src/main.rs",
  "frozen": false,
  "frozen_value": null
}
```

Frozen variables include their snapshot:

```json
{
  "name": "changes",
  "source": "!git diff",
  "frozen": true,
  "frozen_value": "diff --git a/..."
}
```

## Edit Behavior

`/edit N` shows the original template with `{{var}}` placeholders. User edits the template; variables evaluate fresh on re-send.

## Migration

### Existing Sessions

- Old sessions load as-is (expanded content stays expanded)
- Old `variable_bindings` (`{filename: varname}`) convert to `{name, source: "@filename", frozen: false}`
- No attempt to reverse-engineer placeholders from expanded content

### Backward Compatibility

Old `/load filename varname` syntax (without prefix) produces an error with helpful guidance:

```
Error: Missing source prefix. Use:
  /load "=text" var    - literal value
  /load "@path" var    - file contents
  /load "!cmd" var     - command output
```

## Summary

The new variable system stores templates instead of expanded content, evaluates sources live by default, and supports files, commands, and literals as first-class source types. Users can freeze variables when they need a stable snapshot. This enables iterative development workflows where source files change during a session.
