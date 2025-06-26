# Njord Development Roadmap

## Overview
Njord is an interactive LLM REPL that aims to provide a powerful terminal-based interface for conversing with multiple AI providers. This roadmap outlines the development phases from the current basic structure to a fully-featured AI conversation tool.

## Current Status ✅
- [x] Basic CLI argument parsing and configuration
- [x] Provider architecture with OpenAI, Anthropic, and Gemini implementations
- [x] Chat session management with numbered messages and timestamps
- [x] History persistence to JSON file
- [x] Command parsing infrastructure with regex-based parser
- [x] Basic REPL loop structure with professional UX
- [x] ~~TUI framework setup with ratatui and crossterm~~ (Replaced with simple REPL)
- [x] Project structure with proper module organization
- [x] Comprehensive tab completion for commands and parameters

### **Multi-Provider LLM Integration** - COMPLETE! ✅
- [x] **OpenAI API Integration** - COMPLETE!
  - [x] Implement chat completions API calls and Responses API
  - [x] Handle streaming responses with proper SSE parsing
  - [x] Error handling and retry logic with exponential backoff
  - [x] Support for latest models (o3-pro, o3, o4-mini, gpt-4.1, etc.)
  - [x] Reasoning model support (o1, o3 series)
  - [x] Temperature control for supported models

- [x] **Anthropic API Integration** - COMPLETE!
  - [x] Implement Claude API calls with Messages API
  - [x] Handle streaming responses with SSE parsing
  - [x] Support for Claude 4 and 3.x model variants (Sonnet, Opus, Haiku)
  - [x] Message format conversion (system messages handled properly)
  - [x] Thinking mode support for supported models
  - [x] Dynamic temperature handling (1.0 when thinking enabled)

- [x] **Google Gemini API Integration** - COMPLETE!
  - [x] Implement Gemini API calls with streaming
  - [x] Support for Gemini 2.5 Pro, Flash, and Flash Lite models
  - [x] Proper message format conversion
  - [x] SSE streaming response handling

### **Advanced Session Management** - COMPLETE! ✅
- [x] **Rich Session Management System**
  - [x] Always start with fresh sessions
  - [x] Auto-saving sessions with LLM interactions
  - [x] `/chat load NAME` - Load safe copy of session (non-destructive)
  - [x] `/chat continue [NAME]` - Resume/modify saved session
  - [x] `/chat save NAME` - Save current session with name
  - [x] `/chat fork NAME` - Save current session and start fresh
  - [x] `/chat merge NAME` - Merge another session into current
  - [x] `/chat auto-rename [NAME]` - Auto-generate session titles using LLM
  - [x] `/chat auto-rename-all` - Bulk auto-rename all anonymous sessions
  - [x] Automatic session naming with timestamps
  - [x] Prevention of saving empty or command-only sessions
  - [x] Session name source tracking (user-provided vs auto-generated)

## Next Priority 🎯

**🎉 Phase 1, 2, & Code Management Features are COMPLETE! 🎉**

Based on current progress, the next most valuable features to implement are:

1. **Enhanced Response Display** - Better formatting and UX
   - Markdown rendering in terminal (headers, lists, blockquotes, etc.)
   - Code syntax highlighting with language-specific colors
   - Better text formatting (bold, italic, strikethrough)
   - Progress indicators for long responses

2. **Export and Analysis Features**
   - `/export FORMAT` - Export conversations (markdown, JSON, PDF)
   - `/stats` - Show conversation statistics
   - `/tokens` - Show token usage and costs
   - Cost tracking per provider

3. **Advanced Features**
   - Session tagging and metadata
   - `/edit N` - Edit previous messages
   - `/retry` - Retry last request with same or different provider
   - Multi-provider comparison features

## Phase 1: Core Functionality ✅ COMPLETE!

### LLM Provider Integration ✅ COMPLETE!
- [x] **OpenAI API Integration** - COMPLETE!
  - [x] Chat Completions API and Responses API support
  - [x] Streaming with proper SSE parsing
  - [x] Support for o3-pro, o3, o4-mini, gpt-4.1 series
  - [x] Reasoning model handling (o1, o3 series)

- [x] **Anthropic API Integration** - COMPLETE!
  - [x] Messages API with streaming
  - [x] Claude 4 and 3.x model support
  - [x] Thinking mode for supported models
  - [x] Dynamic temperature handling

- [x] **Google Gemini API Integration** - COMPLETE!
  - [x] Gemini API calls with streaming
  - [x] Support for Gemini 2.5 Pro, Flash, Flash Lite
  - [x] Proper message format conversion

### Input/Output System ✅ COMPLETE!
- [x] **Professional REPL Interface** - COMPLETE!
  - [x] Standard input/output with colored prompts
  - [x] Real-time streaming response display
  - [x] Message numbering and timestamps
  - [x] Multi-line input support with triple-backtick blocks
  - [x] Input history with arrow keys and line editing
  - [x] Robust Ctrl-C handling for request interruption
  - [x] Message retry and interruption queuing with UX feedback
  - [x] Thinking mode display (dimmed/italic for thinking content)
  - [x] Tab completion for commands - COMPLETE!
  - [ ] Markdown rendering in terminal
  - [ ] Code syntax highlighting

### Core Commands Implementation ✅ COMPLETE!
- [x] **Advanced Chat Management** - COMPLETE!
  - [x] `/chat new` - Start new session
  - [x] `/chat save NAME` - Save current session with name
  - [x] `/chat load NAME` - Load safe copy of session (non-destructive)
  - [x] `/chat continue [NAME]` - Resume/modify saved session
  - [x] `/chat fork NAME` - Save current session and start fresh
  - [x] `/chat list` - List saved sessions with metadata
  - [x] `/chat delete NAME` - Delete saved session
  - [x] `/chat recent` - Show recent sessions
  - [x] `/chat merge NAME` - Merge another session into current

- [x] **Model Management** - COMPLETE!
  - [x] `/models` - List available models for current provider
  - [x] `/model MODEL` - Switch to different model with validation
  - [x] `/provider PROVIDER` - Switch between providers
  - [x] `/status` - Show current provider, model, and settings

- [x] **Message Navigation** - COMPLETE!
  - [x] `/undo [N]` - Remove last N messages
  - [x] `/goto N` - Jump to message N
  - [x] `/history` - Show conversation history with metadata
  - [ ] `/search TERM` - Search conversation history

- [x] **Configuration Management** - COMPLETE!
  - [x] `/system [PROMPT]` - Set/view/clear system prompt
  - [x] `/temp TEMPERATURE` - Set temperature with model validation
  - [x] `/max-tokens TOKENS` - Set maximum output tokens
  - [x] `/thinking-budget TOKENS` - Set thinking token budget
  - [x] `/thinking on|off` - Enable/disable thinking for supported models

## Phase 2: Advanced Features 🚧

**Current Focus: Code Management and Enhanced UX**

### Code Block Management ✅ COMPLETE!
- [x] **Code Extraction and Management** - COMPLETE!
  - [x] Extract code blocks from markdown responses
  - [x] Number and catalog code blocks per message
  - [x] `/blocks` - List all code blocks in session
  - [x] `/block N` - Display specific code block
  - [x] `/copy N` - Copy code block to clipboard with system + OSC52 support
  - [x] `/save N FILENAME` - Save code block to file
  - [x] `/exec N` - Execute code block (with safety prompts)
  - [x] Support for bash, python, javascript execution

### Enhanced UI/UX 🎯 NEXT PRIORITY
- [x] **Enhanced REPL Interface** - PARTIALLY COMPLETE!
  - [x] Professional REPL with colored prompts and real-time streaming
  - [x] Code block styling with cyan coloring
  - [x] Tab completion for commands with single/multiple completion hints
  - [x] Multi-line input support with triple-backtick blocks
  - [x] Robust Ctrl-C handling and request interruption
  - [x] Clipboard integration (system + OSC52) for universal compatibility
  - [ ] Markdown rendering in terminal (headers, lists, blockquotes, links)
  - [ ] Code syntax highlighting with language-specific colors
  - [ ] Progress indicators for long responses
  - [ ] Optional pager for long responses

- [ ] **Customization**
  - [ ] Theme support
  - [ ] Configurable key bindings
  - [ ] Custom prompt templates

### Advanced Session Features ✅ COMPLETE!
- [x] **Session Management** - COMPLETE!
  - [x] Session forking (save and start fresh)
  - [x] Session merging
  - [x] Auto-save functionality with LLM interaction detection
  - [x] Session metadata (timestamps, provider/model tracking)
  - [x] Safe session loading (non-destructive copies)
  - [x] Session summarization with LLM-generated summaries
  - [x] Auto-renaming with LLM-generated titles (single and bulk)
  - [x] Session name source tracking and duplicate handling
  - [ ] Session templates
  - [ ] Session tagging and descriptions

### Search and Navigation ✅ COMPLETE!
- [x] **Advanced Search** - COMPLETE!
  - [x] `/search TERM` - Search conversation history with highlighted excerpts
  - [x] Search across all saved sessions with session grouping
  - [x] Smart excerpt generation with context preservation
  - [x] Color-coded results by role (user/assistant)

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

### Advanced Commands 🎯 NEXT PRIORITY
- [ ] **Analysis and Export**
  - [ ] `/stats` - Show conversation statistics
  - [ ] `/tokens` - Show token usage and costs
  - [ ] `/export FORMAT` - Export conversation (markdown, JSON, PDF)
  - [x] `/retry` - Retry functionality built into interruption system
  - [x] `/summarize [NAME]` - Generate session summaries (COMPLETE!)

- [ ] **Editing and Refinement**
  - [ ] `/edit N` - Edit previous message
  - [x] `/temp VALUE` - Adjust response creativity (COMPLETE!)
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
- [x] **Streaming Response Parsing** - OpenAI SSE parsing fixed and working perfectly
- [x] **Ctrl-C Handling** - Fixed with proper signal handling and cancellation tokens
- [x] **Session Persistence** - Provider and model selection now persisted across restarts
- [x] **Message Metadata** - Provider and model info now tracked per message
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

### Phase 1 Success Criteria ✅ ALL COMPLETE!
- [x] Can successfully chat with multiple LLM providers (OpenAI, Anthropic, Gemini!)
- [x] Streaming responses work smoothly with proper SSE parsing
- [x] Advanced session save/load functionality with rich management
- [x] All core commands implemented and functional
- [x] Multi-provider support working (OpenAI + Anthropic + Gemini complete!)
- [x] Robust error handling and retry logic with exponential backoff
- [x] Professional UX with input history, interruption, and retry queuing
- [x] Thinking mode support for Anthropic models
- [x] Auto-saving with LLM interaction detection

**🎉 Phase 1 is COMPLETE! 🎉**
**🎉 Phase 2 Session Management is COMPLETE! 🎉**

**🚀 Phase 2 Code Management is COMPLETE! 🚀**
**🎯 Phase 3 Enhanced UX is now IN PROGRESS! 🎯**

**Current Status**: All three major providers (OpenAI, Anthropic, and Gemini) are fully implemented with streaming support. The session management system is sophisticated with auto-saving, safe loading, forking, merging, and continuation features. The REPL has professional UX with robust error handling, retry logic, thinking mode support, and comprehensive tab completion. Code block extraction, management, copying (system + OSC52), saving, and execution are all complete. Advanced search across all sessions with highlighted excerpts is implemented. Session summarization using LLM-generated summaries is now available. Next focus is enhanced markdown rendering and export features.

### Phase 2 Success Criteria ✅ ALL COMPLETE!
- [x] Code block extraction and management works (COMPLETE!)
- [x] Session management is robust and reliable (COMPLETE!)
- [x] Multi-provider workflows are seamless (COMPLETE!)
- [x] Tab completion for commands and parameters (COMPLETE!)
- [x] Advanced search functionality across sessions (COMPLETE!)
- [x] Clipboard integration with universal compatibility (COMPLETE!)

### Phase 3 Success Criteria 🎯 IN PROGRESS
- [ ] Enhanced response display with markdown rendering
- [ ] Export and analysis features provide value
- [ ] Code syntax highlighting improves readability
- [ ] Professional terminal UI experience

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
