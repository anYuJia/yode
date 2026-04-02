<div align="center">

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="assets/logo-dark.svg">
  <source media="(prefers-color-scheme: light)" srcset="assets/logo-light.svg">
  <img alt="Yode" src="assets/logo-dark.svg" width="200">
</picture>

### Open-source AI coding agent for your terminal

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/Rust-1.75+-orange.svg)](https://www.rust-lang.org/)
[![Release](https://img.shields.io/github/v/release/anYuJia/yode?color=green)](https://github.com/anYuJia/yode/releases)
[![Stars](https://img.shields.io/github/stars/anYuJia/yode?style=social)](https://github.com/anYuJia/yode)

**English** | [中文](README.zh-CN.md)

</div>

---

> **Yode** is a terminal-native AI coding agent built in Rust.
> It reads, edits, searches, and runs commands — all from a single conversation.

```
╭─── Yode ──────────────────────────────────────╮
│  claude-sonnet-4-20250514 · ~/my-project       │
╰────────────────────────────────────────────────╯

❯ Fix the authentication bug in login.rs

⏺ Read(src/login.rs)
  ⎿  (248 lines)

⏺ Edit(src/login.rs)
   - if token.is_expired() { return None; }
   + if token.is_expired() { return Err(AuthError::Expired); }

⏺ Done. Expired tokens now return a proper error.
```

## Install

### One-line install (macOS / Linux)

```bash
curl -fsSL https://raw.githubusercontent.com/anYuJia/yode/main/install.sh | bash
```

### Cargo

```bash
cargo install --git https://github.com/anYuJia/yode.git
```

### From source

```bash
git clone https://github.com/anYuJia/yode.git
cd yode
cargo install --path .
```

> **Windows**: Download `yode-x86_64-pc-windows-msvc.zip` from [Releases](https://github.com/anYuJia/yode/releases).

## Quick Start

```bash
# Set API key
export ANTHROPIC_API_KEY="sk-ant-..."   # or OPENAI_API_KEY

# Launch Yode
yode

# Specify a model
yode --model claude-sonnet-4-20250514

# Resume a previous session
yode --resume <session-id>
```

## Features

### LLM Integration
- **Multi-provider** — OpenAI, Anthropic, or any OpenAI-compatible endpoint
- **Streaming responses** — Real-time token streaming with cancellation support
- **Context management** — Automatic summarization when approaching context limits

### Built-in Tools
| Tool | Description |
|------|-------------|
| `bash` | Shell execution with dangerous command detection |
| `read_file` / `write_file` / `edit_file` | Precise file operations |
| `glob` / `grep` | Fast codebase search |
| `web_fetch` / `web_search` | Web scraping and search |
| `lsp` | Language server integration (go-to-definition, references, hover) |
| `agent` | Spawn sub-agents for parallel task execution |
| `memory` | Persistent memory across sessions |
| MCP support | Extend via Model Context Protocol servers |

### Terminal UI
- **Markdown rendering** — Tables, code blocks with syntax highlighting, blockquotes, task lists
- **Braille loading animation** and streaming indicators
- **Scrollback navigation** with input history search (`Ctrl+R`)
- **Permission mode switching** (`Shift+Tab`)
- **Tool confirmation** — `[y]` Allow, `[n]` Deny, `[a]` Always allow
- **File attachments** via `@file` and shell shortcuts via `!command`
- Bracketed paste mode support

### Safety & Control
- **Permission system** — Normal, Auto-Accept, and Plan modes
- **Dangerous command detection** — Blocks destructive `git` operations, `rm -rf`, etc.
- **Session persistence** — SQLite-backed with `--resume` support

## Keybindings

| Key | Action |
|-----|--------|
| `Enter` | Send message |
| `Ctrl+Enter` / `Shift+Enter` | Insert newline |
| `Ctrl+C` | Stop generation (press twice to quit) |
| `Esc` | Stop generation |
| `↑` / `↓` | Scroll chat |
| `Ctrl+P` / `Ctrl+N` | Browse input history |
| `Ctrl+R` | Reverse search history |
| `Ctrl+L` | Clear screen |
| `Ctrl+K` | Delete to end of line |
| `Ctrl+W` | Delete previous word |
| `PageUp` / `PageDown` | Scroll chat (10 lines) |
| `Shift+Tab` | Cycle permission mode |
| `Tab` | Auto-complete command |

## Slash Commands

| Command | Description |
|---------|-------------|
| `/help` | Show all commands |
| `/keys` | Keyboard shortcuts reference |
| `/clear` | Clear chat display |
| `/model` | Show current model |
| `/provider` | Switch LLM provider |
| `/providers` | List available providers |
| `/tools` | List registered tools |
| `/cost` | Show token usage & estimated cost |
| `/diff` | Show `git diff --stat` |
| `/status` | Session status summary |
| `/context` | Context window usage |
| `/compact` | Compact chat history |
| `/copy` | Copy last response to clipboard |
| `/sessions` | List recent sessions |
| `/bug` | Generate bug report |
| `/doctor` | Environment health check |
| `/config` | Show current configuration |
| `/version` | Version info |

## Architecture

```
crates/
├── yode-core     # Engine, context, permissions, database
├── yode-llm      # LLM provider abstraction (OpenAI, Anthropic)
├── yode-tools    # Tool registry & built-in tools
├── yode-tui      # Terminal UI (ratatui-based)
├── yode-mcp      # Model Context Protocol support
└── yode-agent    # Agent orchestration
```

## Project-specific Instructions

Drop a `YODE.md` file in your project root to provide context-aware instructions:

```markdown
# Project Guidelines

This is a Rust project using Actix-web.
- Always run `cargo clippy` after code changes
- Prefer `anyhow::Result` over custom error types
- Use async/await for all I/O operations
```

## Configuration

Config file location: `~/.config/yode/config.toml`

```toml
[provider]
default = "anthropic"
model = "claude-sonnet-4-20250514"

[permissions]
# Tools that never require confirmation
allow = ["read_file", "glob", "grep"]
# Tools that always require confirmation
confirm = ["bash", "write_file", "edit_file"]
```

## Contributing

Contributions are welcome! Here's how you can help:

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

## License

[MIT](LICENSE) — feel free to use, modify, and distribute.
