<div align="center">

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="assets/logo-dark.svg">
  <source media="(prefers-color-scheme: light)" srcset="assets/logo-light.svg">
  <img alt="Yode" src="assets/logo-dark.svg" width="200">
</picture>

### Terminal-native AI coding agent runtime built with Rust

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/Rust-1.75+-orange.svg)](https://www.rust-lang.org/)
[![Release](https://img.shields.io/github/v/release/anYuJia/yode?color=green)](https://github.com/anYuJia/yode/releases)
[![Stars](https://img.shields.io/github/stars/anYuJia/yode?style=social)](https://github.com/anYuJia/yode)

**English** | [中文](README.zh-CN.md)

</div>

---

**Yode** is an open-source coding agent for people who want a serious local terminal workflow:

- built-in tools for reading, editing, searching, shell execution, web fetch, LSP, workflows, review, and MCP
- operator surfaces for `/status`, `/brief`, `/diagnostics`, `/inspect`, `/tasks`, `/remote-control`, `/checkpoint`
- inspectable runtime artifacts for permissions, hooks, team runs, remote sessions, startup settings, and task history
- a tool/runtime model that has been pushed toward Claude Code-style parity, while staying local-first and Rust-native

```text
╭─── Yode ───────────────────────────────────────────────╮
│  claude-sonnet · ~/my-project · Default · 3 jobs       │
╰─────────────────────────────────────────────────────────╯

❯ review the current workspace changes and propose a safe fix

⏺ review_pipeline(...)
⏺ coordinate_agents(...)
⏺ remote_queue_dispatch(...)

/status
/inspect artifact latest-runtime-timeline
/tasks monitor
```

## Install

### One-line install (macOS / Linux)

```bash
curl -fsSL https://raw.githubusercontent.com/anYuJia/yode/main/install.sh | bash
```

### Cargo

```bash
cargo install --git https://github.com/anYuJia/yode.git --tag v0.0.13
```

### From source

```bash
git clone https://github.com/anYuJia/yode.git
cd yode
cargo install --path .
```

### Windows

Download `yode-x86_64-pc-windows-msvc.zip` from [Releases](https://github.com/anYuJia/yode/releases).

## Quick Start

```bash
# Set one provider API key
export ANTHROPIC_API_KEY="..."
# or OPENAI_API_KEY / GEMINI_API_KEY

# Launch the TUI
yode

# Non-interactive one-shot
yode --chat "Summarize the repository structure"

# Pick a provider/model explicitly
yode --provider anthropic --model <model-name>

# Resume a previous session
yode --resume <session-id>

# Run environment checks
yode doctor
```

If you have not configured a provider yet, run:

```bash
yode provider add
```

## Why Yode

### 1. Tool Runtime, Not Just Chat

Yode is not only a shell around an LLM. It ships a real tool/runtime plane with:

- code tools: `read_file`, `write_file`, `edit_file`, `glob`, `grep`, `bash`, `lsp`
- orchestration tools: `agent`, `team_create`, `send_message`, `team_monitor`, `coordinate_agents`
- workflow/review tools: `workflow_run`, `workflow_run_with_writes`, `review_changes`, `review_pipeline`, `review_then_commit`
- remote runtime tools: `remote_queue_dispatch`, `remote_queue_result`, `remote_transport_control`
- runtime helpers: `task_output`, `tool_search`, plan-mode tools, worktree tools, cron tools, MCP resource tools

### 2. Inspectable Operator Surface

The runtime is visible and debuggable from inside the product:

- `/status`, `/brief`, `/diagnostics` for session and runtime summaries
- `/inspect artifact ...` for startup, runtime, hook, permission, team, and remote artifacts
- `/tasks monitor` and `/tasks follow latest` for background work
- `/remote-control monitor`, `/remote-control queue`, and `/remote-control follow latest` for remote/live-session flows
- `/checkpoint` for checkpoint, branch, rewind, restore, and rollback-oriented session control

### 3. Governance, Hooks, and Safety

Yode now includes a much richer control plane than the early versions:

- permission modes: `default`, `plan`, `auto`, `accept-edits`, `bypass`
- inspectable permission governance and precedence chain
- hook lifecycle coverage for tool, task, sub-agent, and worktree flows
- hook `defer` support with resumable artifacts/state
- dangerous shell detection and runtime confirmation rules

### 4. MCP and Managed Settings Visibility

Yode surfaces more than "which tools are loaded":

- provider inventory artifacts
- settings scope artifacts
- managed MCP inventory artifacts
- tool-search activation artifacts
- operator-facing MCP diagnostics and remediation paths

## Commands Worth Learning First

| Command | What it is for |
| --- | --- |
| `/status` | Full session/runtime snapshot |
| `/brief` | Compact current-state summary |
| `/diagnostics` | Runtime health and observability overview |
| `/inspect artifact summary` | Entry point into artifact families |
| `/tools diag` | Tool pool, hidden/deferred tool, and failure diagnostics |
| `/permissions mode` | Permission mode guide and switching |
| `/tasks monitor` | Background runtime task workspace |
| `/remote-control monitor` | Remote live-session monitor surface |
| `/mcp` | MCP and settings control-plane summary |
| `/workflows` | Workflow inspection and run prompts |
| `/checkpoint` | Save/restore/branch/rewind/rollback session state |

## CLI Entry Points

| Command | Description |
| --- | --- |
| `yode` | Start the TUI |
| `yode --chat "<msg>"` | Non-interactive single-turn mode |
| `yode --serve-mcp` | Run Yode as an MCP server over stdio |
| `yode provider list` | List configured providers |
| `yode provider add` | Interactive provider setup |
| `yode update check` | Check and apply updates |
| `yode completions zsh` | Generate shell completions |
| `yode doctor` | Environment health checks |

## Configuration Layers

Yode merges configuration and permission/governance state from multiple layers:

- `~/.yode/managed-config.toml`
- `~/.yode/config.toml`
- `.yode/config.toml`
- `.yode/config.local.toml`
- session and CLI overrides

Example:

```toml
[llm]
default_provider = "anthropic"
default_model = "your-model-name"

[permissions]
default_mode = "auto"

[[permissions.always_allow]]
category = "read"
description = "allow read-only tools"

[[hooks.hooks]]
command = "scripts/pre_tool_use.sh"
events = ["pre_tool_use", "permission_request"]
tool_filter = ["bash", "write_file"]
timeout_secs = 10
can_block = true

[mcp.servers.github]
command = "npx"
args = ["-y", "@modelcontextprotocol/server-github"]
```

## Project Instructions

Yode loads project instructions from multiple compatible filenames, including:

- `YODE.md`
- `docs/YODE.md`
- `.yode/instructions.md`
- `CLAUDE.md`
- `AGENTS.md`
- `.claude/CLAUDE.md`

Example:

```markdown
# Project Guidelines

- Run `cargo test -p yode-core --lib` after engine changes
- Prefer small reviewable patches
- Keep migration notes in `docs/optimization/`
```

## Architecture

```text
crates/
├── yode-core     # engine, context, permissions, hooks, session/runtime state
├── yode-llm      # provider abstraction
├── yode-tools    # built-in tools and runtime tool surface
├── yode-tui      # terminal UI and operator commands
├── yode-mcp      # MCP integration
└── yode-agent    # agent/runtime helpers
```

## What 0.0.13 Adds

The `0.0.13` release finishes a broad Claude Code-style output pass across the TUI:

- tool reads/searches now collapse into compact summary-first blocks instead of dumping raw output by default
- status, diagnostics, brief/context, doctor, inspect, and export artifacts now share one runtime/context/tool summary model
- system messages now have semantic grouping and lightweight status batches for compact/memory/export/task/update events
- retry banners now surface root-cause transport details and retry provider `403` API failures instead of stopping after one attempt

Release: [v0.0.13](https://github.com/anYuJia/yode/releases/tag/v0.0.13)

## Contributing

Contributions are welcome.

1. Fork the repository.
2. Create a branch.
3. Make a focused change.
4. Run the relevant checks.
5. Open a pull request.

## License

[MIT](LICENSE)
