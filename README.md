# Njord

**Interactive LLM REPL - Navigate the vast ocean of AI conversations**

Named after the Norse god of the sea and sailors, Njord guides you through the vast ocean of AI conversations with a powerful terminal-based interface for multiple AI providers.

## Features

- **Multi-Provider Support**: Chat with OpenAI GPT models, Anthropic Claude, and Google Gemini
- **Interactive REPL**: Rich terminal interface with input history, multi-line support, and real-time streaming
- **Session Management**: Save, load, and manage conversation sessions
- **Message Navigation**: Undo messages, jump to specific points, and view conversation history
- **Flexible Configuration**: Support for multiple API keys and model switching
- **Zero Dependencies**: Builds as a single, statically-linked executable

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

## Basic Commands

### Chat Commands
- `/help` - Show all available commands
- `/models` - List available models for current provider
- `/model MODEL` - Switch to a different model
- `/provider PROVIDER` - Switch provider (openai, anthropic, gemini)
- `/status` - Show current provider and model

### Session Management
- `/chat new` - Start a new chat session
- `/chat save NAME` - Save current session with given name
- `/chat load NAME` - Load a previously saved session
- `/chat list` - List all saved sessions
- `/chat delete NAME` - Delete a saved session

### Message Navigation
- `/undo [N]` - Remove last N responses (default 1)
- `/goto N` - Jump back to message N
- `/history` - Show conversation history
- `/system [PROMPT]` - Set system prompt (empty to view, 'clear' to remove)

### Input Tips
- Start with ``` for multi-line input (end with ``` on its own line)
- Use Ctrl-C to interrupt ongoing requests
- Use arrow keys to navigate input history
- Use `/quit` to exit

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
- `gpt-3.5-turbo`
- `gpt-4`
- `gpt-4-turbo`

### Anthropic
- `claude-3-haiku-20240307`
- `claude-3-sonnet-20240229`
- `claude-3-opus-20240229`
- `claude-3-5-sonnet-20241022`

### Google Gemini
- `gemini-pro`
- `gemini-pro-vision`

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

## Roadmap

See [ROADMAP.md](ROADMAP.md) for planned features and development phases.
