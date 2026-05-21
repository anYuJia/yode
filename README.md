<div align="center">

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="assets/logo-dark.svg">
  <source media="(prefers-color-scheme: light)" srcset="assets/logo-light.svg">
  <img alt="Yode" src="assets/logo-dark.svg" width="220">
</picture>

### A local-first AI coding agent runtime for the terminal

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/Rust-1.75+-orange.svg)](https://www.rust-lang.org/)
[![Release](https://img.shields.io/github/v/release/anYuJia/yode?color=green)](https://github.com/anYuJia/yode/releases)
[![Stars](https://img.shields.io/github/stars/anYuJia/yode?style=social)](https://github.com/anYuJia/yode)

**English** | [中文](README.zh-CN.md)

</div>

---

**Yode** is an open-source coding agent for developers who want Claude Code-style agent ergonomics inside a Rust-native, inspectable, terminal workflow.

It is built around three ideas:

- **Act in the repo.** Read, edit, search, run shell commands, use LSP, review diffs, run workflows, and coordinate agents from one terminal session.
- **Keep the runtime visible.** Inspect permissions, hooks, MCP servers, startup settings, background tasks, remote sessions, and compact/restore state.
- **Stay local-first.** Configuration, artifacts, task history, checkpoints, and operator commands live where you can audit and recover them.

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
cargo install --git https://github.com/anYuJia/yode.git --tag v0.0.19
```

### Windows

Download `yode-x86_64-pc-windows-msvc.zip` from [Releases](https://github.com/anYuJia/yode/releases), or install with PowerShell:

```powershell
iwr -useb https://raw.githubusercontent.com/anYuJia/yode/main/install.ps1 | iex
```

### From source

```bash
git clone https://github.com/anYuJia/yode.git
cd yode
cargo install --path .
```

## Quick Start

```bash
# Set one provider API key
export ANTHROPIC_API_KEY="..."
# or OPENAI_API_KEY / GEMINI_API_KEY

# Configure a provider if this is your first run
yode provider add

# Launch the TUI
yode

# Run a non-interactive one-shot
yode --chat "Summarize the repository structure"

# Pick a provider/model explicitly
yode --provider anthropic --model <model-name>

# Resume a previous session
yode --resume <session-id>

# Check the local environment
yode doctor
```

## Core Experience

### Agent Tools

Yode ships a broad built-in tool surface:

| Area | Tools |
| --- | --- |
| Code | `read_file`, `write_file`, `edit_file`, `glob`, `grep`, `bash`, `lsp` |
| Review | `review_changes`, `review_pipeline`, `review_then_commit` |
| Workflow | `workflow_run`, `workflow_run_with_writes`, worktree tools, plan-mode tools |
| Agents | `agent`, `team_create`, `send_message`, `team_monitor`, `coordinate_agents` |
| Remote | `remote_queue_dispatch`, `remote_queue_result`, `remote_transport_control` |
| Runtime | `task_output`, `tool_search`, cron tools, MCP resource tools |

### Operator Commands

The runtime is visible from inside the product:

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
| `/checkpoint` | Save, restore, branch, rewind, and rollback session state |

### Governance and Safety

Yode includes a control plane for serious local work:

- permission modes: `default`, `plan`, `auto`, `accept-edits`, `bypass`
- inspectable permission governance and precedence chains
- hook lifecycle coverage for tool, task, sub-agent, and worktree flows
- hook `defer` support with resumable artifacts/state
- dangerous shell detection and runtime confirmation rules

### MCP and Managed Settings

MCP and settings are part of the inspectable runtime, not hidden setup:

- provider inventory artifacts
- settings scope artifacts
- managed MCP inventory artifacts
- tool-search activation artifacts
- operator-facing MCP diagnostics and remediation paths

## CLI Reference

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

## Configuration

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

## Regression Snapshots

Use the snapshot scripts when changing output surfaces:

- `./scripts/output-regression-snapshot.sh`
  Writes a multi-surface output snapshot to `.yode/benchmarks/output-regression-snapshot.md`.
- `./scripts/split-output-regression-snapshot.sh`
  Splits the combined output snapshot into per-section markdown files.
- `./scripts/diff-output-regression-snapshot.sh`
  Regenerates the snapshot in a temp dir and diffs it against the saved baseline.
- `./scripts/benchmark-snapshot.sh`
  Writes the long-session benchmark snapshot to `.yode/benchmarks/long-session-benchmark.md`.

## Release Candidate Validation

Before tagging a release candidate, run `bash scripts/release-checklist.sh` alongside the
GitHub Actions matrix. The local checklist covers parity docs, snapshots, replay, visual
artifacts, and benchmark evidence; the remote matrix still provides the Linux, macOS, and
Windows confidence needed for the final tag.

Current release-candidate evidence:

- `docs/optimization/304-four-month-release-note-draft.md`
- `docs/optimization/305-release-benchmark-evidence.md`
- `docs/optimization/306-release-validation-matrix.md`

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

## Latest Release

`0.0.19` focuses on terminal command polish, diagnostics readability, and release packaging refinements:

- `/diagnostics` rows now stay width-aware across narrow and wide terminals
- `/context` runtime details truncate long compact, memory, prompt-cache, and task summaries cleanly
- README and release packaging include the refreshed Yode icon asset
- parity and release validation docs remain linked for final tag checks

Release: [v0.0.19](https://github.com/anYuJia/yode/releases/tag/v0.0.19)

## Contributing

Contributions are welcome.

1. Fork the repository.
2. Create a branch.
3. Make a focused change.
4. Run the relevant checks.
5. Open a pull request.

## License

[MIT](LICENSE)
