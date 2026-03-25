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

## Highlights

- **Provider-agnostic** — works with Anthropic, OpenAI, or any OpenAI-compatible API
- **Built-in tools** — read, edit, search, and run commands directly from the conversation
- **Rich TUI** — markdown rendering, streaming responses, keyboard-driven workflow
- **Native performance** — pure Rust, ~3 MB binary, instant startup

## Quick Start

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

### Run

```bash
# Set your API key
export ANTHROPIC_API_KEY="sk-ant-..."

# Launch
yode

# Or specify a model
yode --model claude-sonnet-4-20250514

# Resume a previous session
yode --resume <session-id>
```

## Demo

<!-- TODO: Add a terminal recording GIF here -->
<!-- <img src="assets/demo.gif" width="720" alt="Yode demo"> -->

```
╭─── Yode ──────────────────────────────────────╮
│  claude-sonnet-4-20250514 · ~/my-project       │
│                                                │
│  ? /help for commands · /keys for shortcuts    │
╰────────────────────────────────────────────────╯

> Fix the authentication bug in login.rs

⏺ I'll investigate the login module and fix the issue.

⏺ Read(src/login.rs)
  ⎿  (248 lines)

⏺ Edit(src/login.rs)
   - if token.is_expired() { return None; }
   + if token.is_expired() { return Err(AuthError::Expired); }

⏺ The bug was that expired tokens returned `None` instead of
  a proper error...
```

## Features

### LLM Integration
- **Multi-provider** — OpenAI, Anthropic, or any OpenAI-compatible endpoint
- **Streaming responses** — real-time token streaming with cancel support
- **Context management** — automatic summarization when approaching context limits

### Built-in Tools
- `bash` — run shell commands with safety checks
- `read_file` / `write_file` / `edit_file` — precise file operations
- `glob` / `grep` — fast codebase search
- `web_fetch` / `web_search` — fetch web content and search the web
- `lsp` — language server integration (go-to-definition, references, hover)
- `agent` — spawn sub-agents for parallel tasks
- `memory` — persistent memory across sessions
- MCP server support — extend with any Model Context Protocol server

### Terminal UI
- Markdown rendering — tables, code blocks (syntax-highlighted), blockquotes, task lists
- Braille spinner and streaming indicators
- Scrollbar, input history search (`Ctrl+R`)
- Permission mode cycling (`Shift+Tab`)
- Tool confirmation — `[y]` approve, `[n]` deny, `[a]` always-allow
- `@file` references and `!command` shell shortcuts
- Bracketed paste support

### Safety & Control
- Permission system — Normal, Auto-Accept, and Plan modes
- Dangerous command detection (destructive git operations, `rm -rf`, etc.)
- Session persistence — SQLite-backed with `--resume` support

## Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| `Enter` | Send message |
| `Ctrl+Enter` | Insert newline |
| `Ctrl+C` | Stop generation (×2 to quit) |
| `Esc` | Stop generation |
| `↑` / `↓` | Scroll chat |
| `Ctrl+P` / `Ctrl+N` | Browse input history |
| `Ctrl+R` | Reverse search history |
| `Ctrl+L` | Clear screen |
| `Ctrl+K` | Delete to end of line |
| `Ctrl+W` | Delete previous word |
| `PageUp` / `PageDown` | Scroll chat (10 lines) |
| `Shift+Tab` | Cycle permission mode |
| `Tab` | Autocomplete commands |

## Commands

| Command | Description |
|---------|-------------|
| `/help` | Show all commands |
| `/keys` | Keyboard shortcut reference |
| `/clear` | Clear chat display |
| `/model` | Show current model |
| `/tools` | List available tools |
| `/cost` | Show token usage and estimated cost |
| `/diff` | Show `git diff --stat` |
| `/status` | Session status summary |
| `/context` | Context window usage |
| `/compact` | Compress chat history |
| `/copy` | Copy last response to clipboard |
| `!command` | Execute shell command |
| `@file` | Attach file as context |

## Configuration

Config file: `~/.config/yode/config.toml`

```toml
[provider]
default = "anthropic"
model = "claude-sonnet-4-20250514"

[permissions]
# Tools that are always allowed without confirmation
allow = ["read_file", "glob", "grep"]
# Tools that always require confirmation
confirm = ["bash", "write_file", "edit_file"]
```

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

## Project-level Config

Create a `YODE.md` file in your project root to give the agent project-specific instructions:

```markdown
# Project Instructions

This is a Rust project using Actix-web.
Always run `cargo clippy` after making changes.
Prefer `anyhow::Result` over custom error types.
```

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

1. Fork the repository
2. Create your feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

## License

MIT — see [LICENSE](LICENSE) for details.
