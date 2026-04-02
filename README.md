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

[Install](#install) · [Usage](#usage) · [Features](#features) · [Configuration](#configuration) · [Contributing](#contributing)

**English** | [中文](README.zh-CN.md)

</div>

---

> Yode is a terminal-native AI coding agent built in Rust.
> It reads, edits, searches, and runs commands — all from a single conversation.

<!-- TODO: Add demo GIF -->
<!-- <p align="center"><img src="assets/demo.gif" width="720" alt="Yode demo"></p> -->

```
╭─── Yode ──────────────────────────────────────╮
│  claude-sonnet-4-20250514 · ~/my-project       │
╰────────────────────────────────────────────────╯

> Fix the authentication bug in login.rs

⏺ Read(src/login.rs)
  ⎿  (248 lines)

⏺ Edit(src/login.rs)
   - if token.is_expired() { return None; }
   + if token.is_expired() { return Err(AuthError::Expired); }

⏺ Done. Expired tokens now return a proper error.
```

## Install

```bash
# macOS / Linux — one-line install
curl -fsSL https://raw.githubusercontent.com/anYuJia/yode/main/install.sh | bash

# Or with Cargo
cargo install --git https://github.com/anYuJia/yode.git
```

> **Windows**: Download `yode-x86_64-pc-windows-msvc.zip` from [Releases](https://github.com/anYuJia/yode/releases).

## Usage

```bash
export ANTHROPIC_API_KEY="sk-ant-..."   # or OPENAI_API_KEY
yode                                     # launch
yode --model claude-sonnet-4-20250514    # specify model
yode --resume <session-id>               # resume session
```

## Features

**Provider-agnostic** — Anthropic, OpenAI, or any OpenAI-compatible endpoint

**Rich terminal UI** — Markdown rendering, streaming responses, keyboard-driven workflow

**Built-in tools** — File I/O, shell execution, search, LSP, web fetch, sub-agents, memory

**MCP support** — Extend with any Model Context Protocol server

**Safe by design** — Permission system with dangerous command detection

**Native performance** — Pure Rust, ~3 MB binary, instant startup

**Session persistence** — SQLite-backed, resume any previous conversation

## Built-in Tools

| Tool | Description |
|------|-------------|
| `bash` | Run shell commands with safety checks |
| `read_file` / `write_file` / `edit_file` | Precise file operations |
| `glob` / `grep` | Fast codebase search |
| `web_fetch` / `web_search` | Fetch web content |
| `lsp` | Go-to-definition, references, hover |
| `agent` | Spawn sub-agents for parallel tasks |
| `memory` | Persistent memory across sessions |

## Configuration

```bash
~/.config/yode/config.toml
```

```toml
[provider]
default = "anthropic"
model = "claude-sonnet-4-20250514"
```

Drop a `YODE.md` in your project root for project-specific instructions:

```markdown
This is a Rust project using Actix-web.
Always run `cargo clippy` after making changes.
```

## Architecture

```
crates/
├── yode-core     # Engine, context, permissions, database
├── yode-llm      # LLM provider abstraction
├── yode-tools    # Tool registry & built-in tools
├── yode-tui      # Terminal UI (ratatui-based)
├── yode-mcp      # Model Context Protocol support
└── yode-agent    # Agent orchestration
```

## Contributing

Contributions welcome! Fork → branch → PR.

## License

[MIT](LICENSE)
