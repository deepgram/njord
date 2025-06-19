# Njord

**Interactive LLM REPL - Navigate the vast ocean of AI conversations**

Named after the Norse god of the sea and sailors, Njord guides you through the vast ocean of AI conversations with a powerful terminal-based interface for multiple AI providers.

## Features

### 🤖 Multi-Provider AI Support
- **OpenAI**: Latest models including o3-pro, o3, o4-mini, gpt-4.1 series with reasoning model support
- **Anthropic**: Claude 4 and 3.x models (Sonnet, Opus, Haiku) with thinking mode support
- **Google Gemini**: Gemini 2.5 Pro, Flash, and Flash Lite models
- **Smart Model Detection**: Automatic provider switching based on model selection

### 💬 Advanced Chat Experience
- **Real-time Streaming**: Live response streaming with proper interruption handling
- **Thinking Mode**: See AI reasoning process for supported Anthropic models
- **Multi-line Input**: Triple-backtick code blocks for complex prompts
- **Smart Interruption**: Ctrl-C handling with message queuing and retry logic
- **Tab Completion**: Intelligent command and parameter completion with hints

### 📁 Powerful Session Management
- **Auto-saving**: Sessions automatically saved when they contain AI interactions
- **Session Operations**: Save, load, fork, merge, and continue sessions
- **Safe Loading**: Load copies of sessions without modifying originals
- **Recent Sessions**: Quick access to recently used conversations
- **Session Search**: Full-text search across all saved sessions with highlighted excerpts

### 🔧 Code Block Management
- **Automatic Extraction**: Code blocks automatically detected and numbered
- **Universal Clipboard**: Copy to system clipboard + OSC52 for SSH/terminal compatibility
- **File Operations**: Save code blocks directly to files
- **Safe Execution**: Execute bash, Python, and JavaScript with confirmation prompts
- **Language Support**: Syntax detection for multiple programming languages

### 🎨 Professional Terminal UI
- **Colored Output**: Syntax highlighting for code blocks and role-based message coloring
- **Message History**: Navigate conversation history with timestamps and metadata
- **Command System**: Comprehensive slash commands for all operations
- **Input History**: Arrow key navigation through previous inputs
- **Status Display**: Current model, provider, and configuration at startup

## Quick Start

### Prerequisites

You'll need at least one API key from the supported providers:

- **OpenAI**: Get your API key from [OpenAI Platform](https://platform.openai.com/api-keys)
- **Anthropic**: Get your API key from [Anthropic Console](https://console.anthropic.com/)
- **Google Gemini**: Get your API key from [Google AI Studio](https://aistudio.google.com/app/apikey)

### Installation

#### Option 1: Download Pre-built Binary

Download the latest release from the [releases page](https://github.com/yourusername/njord/releases).

#### Option 2: Build from Source

```bash
# Clone the repository
git clone https://github.com/yourusername/njord.git
cd njord

# Build the project
cargo build --release

# The binary will be at target/release/njord
```

### Setup

Set your API keys as environment variables:

```bash
# Set at least one API key
export OPENAI_API_KEY="your-openai-key-here"
export ANTHROPIC_API_KEY="your-anthropic-key-here"
export GEMINI_API_KEY="your-gemini-key-here"
```

### Usage

Start Njord:

```bash
./njord
```

Or with command-line options:

```bash
# Start with a specific model
./njord --model gpt-4

# Start with custom temperature
./njord --temperature 0.9

# Load a saved session
./njord --load-session "my-session"

# Start fresh session
./njord --new-session
```

## Command Reference

### 🤖 Model & Provider Management
- `/models` - List all available models across providers
- `/model MODEL` - Switch to any model (auto-detects provider)
- `/status` - Show current provider, model, and configuration

### 💬 Session Management
- `/chat new` - Start fresh session
- `/chat save NAME` - Save current session
- `/chat load NAME` - Load safe copy of session
- `/chat continue [NAME]` - Resume most recent or named session
- `/chat fork NAME` - Save current session and start fresh
- `/chat merge NAME` - Merge another session into current
- `/chat list` - List all saved sessions with metadata
- `/chat recent` - Show recently used sessions
- `/chat delete NAME` - Delete saved session

### 📝 Message & History
- `/history` - Show full conversation with timestamps
- `/undo [N]` - Remove last N messages (default 1)
- `/goto N` - Jump to message N, removing later messages
- `/search TERM` - Search across all sessions with highlighted results

### 🔧 Code Block Operations
- `/blocks` - List all code blocks in current session
- `/block N` - Display specific code block with syntax highlighting
- `/copy N` - Copy code block to clipboard (system + OSC52)
- `/save N FILENAME` - Save code block to file
- `/exec N` - Execute code block with safety confirmation

### ⚙️ Configuration
- `/system [PROMPT]` - Set/view/clear system prompt
- `/temp VALUE` - Set temperature (0.0-2.0, model-dependent)
- `/max-tokens N` - Set maximum response tokens
- `/thinking on|off` - Enable/disable thinking mode (Anthropic models)
- `/thinking-budget N` - Set thinking token budget

### 🔍 Utilities
- `/help` - Show all commands
- `/clear` - Clear terminal screen
- `/quit` - Exit Njord

### 💡 Pro Tips
- **Multi-line Input**: Start with ``` and end with ``` on its own line
- **Smart Interruption**: Use Ctrl-C to cancel requests - messages are queued for retry
- **Tab Completion**: Press Tab for command completion with helpful hints
- **Universal Clipboard**: `/copy` works in SSH sessions and all terminal types
- **Session Safety**: `/chat load` creates copies, originals remain unchanged

## Development

### Prerequisites

- Rust 1.70+ (install from [rustup.rs](https://rustup.rs/))
- Git

### Building

```bash
# Clone the repository
git clone https://github.com/yourusername/njord.git
cd njord

# Build in development mode
cargo build

# Build optimized release
cargo build --release

# Run directly with cargo
cargo run -- --help
```

### Building Static Binary

For deployment or distribution, you can build a statically-linked binary with zero runtime dependencies:

#### On Debian/Ubuntu:

```bash
# Install musl tools
sudo apt update
sudo apt install musl-tools musl-dev

# Add musl target
rustup target add x86_64-unknown-linux-musl

# Build static binary
cargo build --release --target x86_64-unknown-linux-musl

# Binary will be at target/x86_64-unknown-linux-musl/release/njord
```

#### On other platforms:

```bash
# macOS (already statically links by default)
cargo build --release

# Windows
cargo build --release --target x86_64-pc-windows-msvc
```

### Running Tests

```bash
# Run all tests
cargo test

# Run tests with output
cargo test -- --nocapture

# Run specific test
cargo test test_name
```

### Code Structure

```
src/
├── main.rs           # Application entry point
├── cli.rs            # Command-line argument parsing
├── config.rs         # Configuration management
├── repl.rs           # Main REPL loop and logic
├── ui.rs             # User interface and terminal handling
├── commands.rs       # Command parsing and execution
├── session.rs        # Chat session management
├── history.rs        # Session persistence
└── providers/        # LLM provider implementations
    ├── mod.rs        # Provider trait and factory
    ├── openai.rs     # OpenAI API integration
    ├── anthropic.rs  # Anthropic API integration
    └── gemini.rs     # Google Gemini API integration
```

## Configuration

### Environment Variables

```bash
# API Keys (at least one required)
export OPENAI_API_KEY="your-openai-key"
export ANTHROPIC_API_KEY="your-anthropic-key"
export GEMINI_API_KEY="your-gemini-key"

# Optional defaults
export NJORD_DEFAULT_MODEL="gpt-4"
export NJORD_DEFAULT_TEMPERATURE="0.7"
```

### Command Line Options

```bash
njord --help
```

## Supported Models

### OpenAI
- `o3-pro` (latest reasoning model)
- `o3` (reasoning model)
- `o4-mini` (fast reasoning model)
- `o3-mini` (compact reasoning model)
- `o1-pro` (reasoning model)
- `o1` (reasoning model)
- `gpt-4.1` (latest chat model)
- `gpt-4o`
- `gpt-4.1-mini`
- `gpt-4o-mini`
- `gpt-4.1-nano`

### Anthropic
- `claude-sonnet-4-20250514` (latest, supports thinking)
- `claude-opus-4-20250514` (supports thinking)
- `claude-3-7-sonnet-20250219` (supports thinking)
- `claude-3-5-sonnet-20241022`
- `claude-3-5-haiku-20241022`
- `claude-3-5-sonnet-20240620`

### Google Gemini
- `gemini-2.5-pro`
- `gemini-2.5-flash`
- `gemini-2.5-flash-lite`

## Contributing

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Make your changes
4. Add tests if applicable
5. Commit your changes (`git commit -m 'Add amazing feature'`)
6. Push to the branch (`git push origin feature/amazing-feature`)
7. Open a Pull Request

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## Acknowledgments

- Named after Njörðr, the Norse god associated with the sea, seafaring, wind, fishing, wealth, and crop fertility
- Built with [Rust](https://www.rust-lang.org/) for performance and safety
- Uses [rustls](https://github.com/rustls/rustls) for pure Rust TLS implementation
- Terminal interface powered by [rustyline](https://github.com/kkawakam/rustyline)
- Universal clipboard support via [arboard](https://github.com/1Password/arboard) and OSC52 escape sequences
- Developed with [Aider](https://aider.chat/) - the entire project was collaboratively built using Aider and Claude-3.5-Sonnet

## What's New in v0.2.0

- **Complete Code Management System**: Extract, view, copy, save, and execute code blocks
- **Universal Clipboard Integration**: Works in SSH sessions and all terminal environments
- **Advanced Session Operations**: Fork, merge, continue, and safe loading of sessions
- **Full-Text Search**: Search across all sessions with intelligent excerpt highlighting
- **Enhanced Tab Completion**: Smart command completion with helpful hints
- **Thinking Mode Support**: See AI reasoning process for supported Anthropic models
- **Robust Interruption Handling**: Ctrl-C with message queuing and retry logic
- **Professional Terminal UI**: Syntax highlighting, colored output, and status displays

## Roadmap

See [ROADMAP.md](ROADMAP.md) for planned features and development phases.
