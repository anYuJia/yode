<div align="center">

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="assets/logo-dark.svg">
  <source media="(prefers-color-scheme: light)" srcset="assets/logo-light.svg">
  <img alt="Yode" src="assets/logo-dark.svg" width="220">
</picture>

### A local-first desktop AI coding agent

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/Rust-1.75+-orange.svg)](https://www.rust-lang.org/)
[![Release](https://img.shields.io/github/v/release/anYuJia/yode?color=green)](https://github.com/anYuJia/yode/releases)
[![Stars](https://img.shields.io/github/stars/anYuJia/yode?style=social)](https://github.com/anYuJia/yode)

**English** | [中文](README.zh-CN.md)

</div>

---

**Yode** is an open-source, desktop-first coding agent built with Rust, Tauri, and React. It is designed for long-running repository work where planning, execution, verification, recovery, and observability need to stay inside one inspectable local application.

Yode is no longer developed as a CLI/TUI product. The supported product surface is `apps/yode-desktop/`; reusable Agent capabilities live in Rust crates and are exposed to the GUI through the Tauri runtime.

## Product principles

- **Act in the repository.** Read, edit, search, run commands, use LSP, review diffs, coordinate agents, and operate worktrees from one desktop workspace.
- **Verify before delivery.** Code-changing runs are expected to produce verification evidence before they can be considered complete.
- **Keep the runtime visible.** Runs, permissions, tool traces, background tasks, recovery state, costs, browser actions, artifacts, and evaluation results are inspectable by the GUI.
- **Stay local-first.** Project state, artifacts, indexes, evaluation data, and recovery information remain auditable on disk.

## Desktop capabilities

| Area | Current direction |
| --- | --- |
| Agent runtime | RunController lifecycle, hard budgets, cancellation, recovery and persistent run state |
| Code intelligence | Repository map, LSP and persistent repository intelligence index |
| Verification | Acceptance criteria, evidence tracking, test/review verification gates |
| Multi-agent | Planning, DAG orchestration, true concurrent ready batches and background agents |
| Browser | Real Chromium CDP runtime for navigation, interaction, JavaScript and screenshots |
| Evaluation | YodeBench task/outcome/metrics model with workspace artifacts |
| Safety | Workspace trust, permission governance, fail-closed tool/runtime behavior |
| Extensibility | MCP, skills, hooks, tools and provider abstractions |

## Install

Download the desktop application for macOS, Windows, or Linux from GitHub Releases.

Yode does not provide or maintain a root CLI binary, Cargo-install workflow, shell completion package, or TUI installer.

## Run from source

### Requirements

- Rust toolchain
- Node.js
- pnpm
- platform dependencies required by Tauri

### Development

```bash
git clone https://github.com/anYuJia/yode.git
cd yode/apps/yode-desktop
pnpm install --frozen-lockfile
pnpm tauri:dev
```

### Validation

```bash
# from repository root
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --no-deps -- -D warnings
cargo check --workspace --all-targets
cargo test --workspace

# Desktop frontend
cd apps/yode-desktop
pnpm test
pnpm build
```

Provider integration tests live with the provider crate:

```bash
cargo test -p yode-llm --test anthropic_integration
```

## Architecture

```text
Desktop React UI
      ↓ Tauri commands / events
apps/yode-desktop/src-tauri
      ↓
┌──────────────────────────────────────────────────────────────┐
│ yode-core      AgentEngine, RunController, context, safety  │
│ yode-agent     planning, orchestration, scheduler           │
│ yode-tools     repository/tool/browser/runtime capabilities │
│ yode-runtime   shared runtime bootstrap                     │
├──────────────────────────────────────────────────────────────┤
│ yode-llm       provider/model abstraction                   │
│ yode-mcp       MCP integration                              │
│ yode-index     repository intelligence                      │
│ yode-evals     YodeBench evaluation model                   │
└──────────────────────────────────────────────────────────────┘
```

The repository root is a **virtual Cargo workspace**. There is intentionally no root `src/main.rs` or root `yode` executable.

## Agent tools

Yode includes a broad runtime tool surface, including file operations, search, shell execution, LSP, review, worktrees, MCP, repository intelligence, verification agents, background tasks, multi-agent coordination, browser control, and Git operations.

The project does not aim to win by adding more tools indefinitely. Current development prioritizes a stronger closed loop:

```text
Intent
  → Analyze / Acceptance Criteria
  → Plan
  → Execute
  → Verify
      ├─ pass → Deliver
      └─ fail → Replan → Execute
  → Evaluate / Postmortem / Learn
```

## Runtime artifacts

Project-local `.yode/` data is used for inspectable runtime state such as:

- run/session artifacts
- verification and review evidence
- repository index data
- browser screenshots/cache
- YodeBench tasks and outcomes
- transcripts, plans, checkpoints and recovery information

## Project instructions

Yode can load compatible project instruction files including:

- `YODE.md`
- `docs/YODE.md`
- `.yode/instructions.md`
- `CLAUDE.md`
- `AGENTS.md`
- `.claude/CLAUDE.md`

For contributors and coding agents working on Yode itself, `AGENTS.md` is the canonical product-direction guide. In particular, new user-facing capabilities must target the Desktop GUI rather than recreating CLI/TUI surfaces.

## Release

Tagged releases are built through the Tauri desktop release workflow for macOS, Windows, and Linux. Release quality gates run Rust formatting, clippy, workspace checks/tests, and dependency audit before packaging.

## Contributing

Contributions are welcome. Keep changes focused, preserve fail-closed behavior, add verification for behavior changes, and run the relevant Rust and Desktop checks before submitting.

## License

[MIT](LICENSE)
