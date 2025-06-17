# Njord - Interactive LLM REPL

*Named after the Norse god of the sea and sailors, Njord guides you through the vast ocean of AI conversations.*

## Project Vision and Goals

This is an **Interactive LLM REPL** - a terminal-based chat interface for multiple AI providers with advanced session management and developer-friendly features.

### Core Vision
- **Terminal-native**: Designed for command-line power users who prefer keyboard-driven workflows
- **Multi-provider**: Seamless switching between Anthropic, OpenAI, Gemini, and other LLM providers
- **Session persistence**: Automatic save/restore of chat history with granular navigation
- **Developer-focused**: Code block extraction, clipboard integration, and numbered references

## Key Functionality

### Provider Management
- Auto-detect API keys from environment variables (`ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, etc.)
- CLI arguments for explicit key specification
- `/model MODEL` to switch providers/models mid-conversation
- `/models` to list available models

### Session Management
- Numbered interactions (User 1, Agent 1, User 2, Agent 2...)
- `/chat new` - Start fresh session, archive current to `.llm-history`
- `/chat save NAME` - Save current session with custom name
- `/chat load NAME` - Load named session
- `/chat list` - Show available saved sessions

### Navigation & History
- `/undo` - Remove last agent response from context
- `/undo N` - Remove last N responses
- `/goto N` - Jump back to response N, truncate history there
- `/history` - Show numbered conversation overview
- `/search TERM` - Search through chat history

### Code & Content Management
- Numbered code blocks (Block 1, Block 2...)
- `/block N` - Copy code block N to clipboard (Xorg/OSC-52)
- `/copy N` - Copy entire response N to clipboard
- `/save N FILE` - Save response N to file
- `/exec N` - Execute code block N (with confirmation)

### Additional Features
- `/system PROMPT` - Set system prompt for session
- `/temp 0.7` - Adjust temperature/creativity
- `/tokens` - Show token usage stats
- `/export FORMAT` - Export chat (markdown, json, html)
- `/help` or `/commands` - Show all available commands
- `/clear` - Clear terminal display (keep history)
- `/stats` - Show session statistics
- `/retry` - Regenerate last response
- `/edit N` - Edit and resend message N

### Terminal Experience
- Real-time streaming responses via WebSockets/SSE
- ANSI color coding (user input, agent responses, commands, code blocks)
- Syntax highlighting for code blocks
- Progress indicators during API calls
- Graceful handling of Ctrl+C (save state)
- Tab completion for commands and model names

### Persistence
- `.llm-history` in current directory (JSON format)
- Automatic backup rotation
- Resume last session on startup with context display
- Cross-session search capabilities

## Desired Behavior

Njord creates a powerful, persistent, and highly navigable AI chat environment optimized for technical users who want full control over their AI interactions. The tool should feel like a native terminal application with the responsiveness and feature set that systems engineers and developers expect from their command-line tools.

Like its namesake Norse god who provided safe passage across treacherous waters, Njord helps developers navigate the complex landscape of AI providers and conversations with confidence and control.
