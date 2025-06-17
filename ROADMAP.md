# Njord Development Roadmap

## Overview
Njord is an interactive LLM REPL that aims to provide a powerful terminal-based interface for conversing with multiple AI providers. This roadmap outlines the development phases from the current basic structure to a fully-featured AI conversation tool.

## Current Status ✅
- [x] Basic CLI argument parsing and configuration
- [x] Provider architecture with OpenAI, Anthropic, and Gemini stubs
- [x] Chat session management with numbered messages and timestamps
- [x] History persistence to JSON file
- [x] Command parsing infrastructure with regex-based parser
- [x] Basic REPL loop structure
- [x] TUI framework setup with ratatui and crossterm
- [x] Project structure with proper module organization

## Phase 1: Core Functionality 🚧

### LLM Provider Integration
- [ ] **OpenAI API Integration**
  - [ ] Implement chat completions API calls
  - [ ] Handle streaming responses
  - [ ] Error handling and rate limiting
  - [ ] Support for different models (GPT-3.5, GPT-4, etc.)

- [ ] **Anthropic API Integration**
  - [ ] Implement Claude API calls
  - [ ] Handle streaming responses
  - [ ] Support for Claude 3 model variants

- [ ] **Google Gemini API Integration**
  - [ ] Implement Gemini API calls
  - [ ] Handle streaming responses
  - [ ] Support for Gemini Pro models

### Input/Output System
- [x] **Simple REPL Interface** (replacing broken TUI)
  - [x] Standard input/output with prompt
  - [x] Text coloring for responses
  - [ ] Multi-line input support
  - [ ] Input history with arrow keys
  - [ ] Tab completion for commands

- [ ] **Response Display**
  - [x] Real-time streaming response display
  - [ ] Markdown rendering in terminal
  - [ ] Code syntax highlighting
  - [x] Message numbering and timestamps

### Core Commands Implementation
- [ ] **Chat Management**
  - [x] `/chat new` - Start new session (basic implementation)
  - [ ] `/chat save NAME` - Save current session
  - [ ] `/chat load NAME` - Load saved session
  - [ ] `/chat list` - List saved sessions
  - [ ] `/chat delete NAME` - Delete saved session

- [ ] **Model Management**
  - [x] `/models` - List available models (basic implementation)
  - [ ] `/model MODEL` - Switch to different model
  - [ ] Model-specific configuration

- [ ] **Message Navigation**
  - [x] `/undo [N]` - Remove last N messages (basic implementation)
  - [ ] `/goto N` - Jump to message N
  - [ ] `/history` - Show conversation history
  - [ ] `/search TERM` - Search conversation history

## Phase 2: Advanced Features 🔮

### Code Block Management
- [ ] **Code Extraction and Management**
  - [ ] Extract code blocks from markdown responses
  - [ ] Number and catalog code blocks
  - [ ] `/block N` - Display specific code block
  - [ ] `/copy N` - Copy code block to clipboard
  - [ ] `/save N FILENAME` - Save code block to file
  - [ ] `/exec N` - Execute code block (with safety prompts)

### Enhanced UI/UX
- [ ] **Enhanced REPL Interface**
  - [ ] Better text coloring and formatting
  - [ ] Progress indicators for streaming
  - [ ] Status information display
  - [ ] Optional pager for long responses

- [ ] **Customization**
  - [ ] Theme support
  - [ ] Configurable key bindings
  - [ ] Custom prompt templates
  - [ ] `/system PROMPT` - Set system prompt

### Session Management
- [ ] **Advanced Session Features**
  - [ ] Session branching (fork conversations)
  - [ ] Session merging
  - [ ] Session templates
  - [ ] Auto-save functionality
  - [ ] Session metadata (tags, descriptions)

## Phase 3: Power User Features 🚀

### Multi-Provider Support
- [ ] **Provider Comparison**
  - [ ] Send same prompt to multiple providers
  - [ ] Side-by-side response comparison
  - [ ] Provider performance metrics

- [ ] **Provider Switching**
  - [ ] Hot-swap providers mid-conversation
  - [ ] Provider-specific optimizations
  - [ ] Cost tracking per provider

### Advanced Commands
- [ ] **Analysis and Export**
  - [ ] `/stats` - Show conversation statistics
  - [ ] `/tokens` - Show token usage and costs
  - [ ] `/export FORMAT` - Export conversation (markdown, JSON, PDF)
  - [ ] `/retry` - Retry last request with same or different provider

- [ ] **Editing and Refinement**
  - [ ] `/edit N` - Edit previous message
  - [ ] `/temperature VALUE` - Adjust response creativity
  - [ ] Message templates and snippets

### Integration Features
- [ ] **External Tool Integration**
  - [ ] Plugin system for custom commands
  - [ ] Integration with external editors
  - [ ] File upload and processing
  - [ ] Web search integration

## Phase 4: Enterprise Features 🏢

### Configuration and Profiles
- [ ] **Configuration Management**
  - [ ] Configuration file support (TOML/YAML)
  - [ ] Environment-specific configs
  - [ ] User profiles and workspaces

### Security and Privacy
- [ ] **Security Features**
  - [ ] API key encryption at rest
  - [ ] Local conversation encryption
  - [ ] Audit logging
  - [ ] Data retention policies

### Performance and Reliability
- [ ] **Optimization**
  - [ ] Response caching
  - [ ] Offline mode with cached responses
  - [ ] Background processing
  - [ ] Memory usage optimization

## Technical Debt and Improvements

### Known Issues
- [x] **TUI Input Handling Broken** - Replaced with simple REPL
- [x] **Overcomplicated UI** - Simplified to standard terminal interface
- [ ] **Streaming Response Parsing** - OpenAI SSE parsing needs improvement
- [ ] **Error Handling** - Need better user-friendly error messages

### Code Quality
- [ ] **Testing**
  - [ ] Unit tests for all modules
  - [ ] Integration tests for API providers
  - [ ] End-to-end TUI testing
  - [ ] Performance benchmarks

- [ ] **Documentation**
  - [ ] API documentation
  - [ ] User manual
  - [ ] Developer guide
  - [ ] Command reference

### Architecture Improvements
- [ ] **Error Handling**
  - [ ] Comprehensive error types
  - [ ] Graceful degradation
  - [ ] User-friendly error messages
  - [ ] Recovery mechanisms

- [ ] **Performance**
  - [ ] Async/await optimization
  - [ ] Memory leak prevention
  - [ ] Startup time optimization
  - [ ] Large conversation handling

## Environment Variables and Configuration

### Required Environment Variables
```bash
# At least one API key is required
export OPENAI_API_KEY="your-openai-key"
export ANTHROPIC_API_KEY="your-anthropic-key"
export GEMINI_API_KEY="your-gemini-key"
```

### Optional Configuration
```bash
# Default model preference
export NJORD_DEFAULT_MODEL="gpt-4"
export NJORD_DEFAULT_TEMPERATURE="0.7"

# UI preferences
export NJORD_THEME="dark"
export NJORD_EDITOR="vim"
```

## Success Metrics

### Phase 1 Success Criteria
- [ ] Can successfully chat with at least one LLM provider
- [ ] Streaming responses work smoothly
- [ ] Basic session save/load functionality
- [ ] Core commands are implemented and functional

### Phase 2 Success Criteria
- [ ] Code block extraction and management works
- [ ] Advanced TUI provides good user experience
- [ ] Session management is robust and reliable

### Phase 3 Success Criteria
- [ ] Multi-provider workflows are seamless
- [ ] Power user features enhance productivity
- [ ] Export and analysis features provide value

### Phase 4 Success Criteria
- [ ] Enterprise-ready security and configuration
- [ ] Performance scales to large conversations
- [ ] Comprehensive testing and documentation

## Contributing

This roadmap is a living document. As development progresses, priorities may shift based on user feedback and technical discoveries. Each phase builds upon the previous one, ensuring a stable foundation for advanced features.

Key areas where contributions are welcome:
- LLM provider API implementations
- TUI/UX improvements
- Testing and documentation
- Performance optimizations
- New feature ideas and feedback
