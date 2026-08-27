# AGENTS.md

This file provides guidance to coding agents working in this repository.

## 产品方向（必须遵守）

Yode 已经完成从 CLI/TUI 产品向 **Desktop GUI-first AI coding agent** 的迁移。

- `apps/yode-desktop/` 是唯一正式用户产品入口（Tauri + React）。
- 不要重新创建根目录 CLI binary、TUI、`src/main.rs`、`clap` 命令树、shell completion 或 `cargo install --path .` 产品路径。
- 新能力必须优先通过 `yode-core` / `yode-agent` / `yode-tools` / `yode-runtime` 等共享 runtime crate 实现，再通过 Tauri command / event 暴露给 Desktop GUI。
- 仅用于底层执行的 shell/bash/powershell 工具属于 Agent runtime 能力，不等于恢复 CLI 产品。
- 如果旧文档、脚本或测试仍引用 CLI/TUI，应迁移到 Desktop/runtime 语义，而不是为兼容旧 CLI 新增代码。
- 除非有明确迁移需求，不为已删除 CLI 保留兼容层。

## 项目概述

Yode 是用 Rust + Tauri + React 构建的本地优先 AI 编程 Agent。核心目标是可检查、可恢复、可验证的长任务执行，并提供多模型、工具调用、代码理解、多 Agent 编排、浏览器与 Desktop 控制面。

## 常用命令

### Rust workspace

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --no-deps -- -D warnings
cargo check --workspace --all-targets
cargo test --workspace
```

### Desktop

```bash
cd apps/yode-desktop
pnpm install --frozen-lockfile
pnpm test
pnpm build
pnpm tauri:dev
```

### Provider integration

```bash
cargo test -p yode-llm --test anthropic_integration
```

## Workspace 结构

```text
yode/
├── Cargo.toml                      # virtual workspace；无根 CLI package
├── apps/yode-desktop/              # 唯一正式产品入口
│   ├── src/                        # React GUI
│   └── src-tauri/                  # Tauri/Desktop runtime bridge
├── crates/
│   ├── yode-core/                  # AgentEngine、上下文、权限、会话、RunController
│   ├── yode-llm/                   # provider/model abstraction
│   ├── yode-tools/                 # Agent tools
│   ├── yode-mcp/                   # MCP
│   ├── yode-agent/                 # planning/orchestration/scheduler
│   ├── yode-runtime/               # shared runtime bootstrap
│   ├── yode-evals/                 # YodeBench/evaluation model
│   └── yode-index/                 # repository intelligence/index
└── config/
    └── default.toml
```

## 架构边界

```text
Desktop React UI
      ↓ Tauri commands/events
apps/yode-desktop/src-tauri
      ↓
yode-core / yode-agent / yode-tools / yode-runtime
      ↓
yode-llm / yode-mcp / yode-index / yode-evals
```

不要把核心 Agent 行为直接写进 React 组件，也不要为了调用核心能力新增 CLI command。Desktop 后端负责 adapter/orchestration，核心能力应尽量留在可测试的 Rust crate。

## 当前重点

优先提高 Agent 闭环能力，而不是继续堆工具数量：

1. YodeBench / eval-driven development
2. RunController 与状态机
3. Acceptance Criteria + Verification Gate
4. Repository Intelligence
5. Parallel DAG Scheduler
6. Browser / Computer Use
7. Model Capability Registry + ModelRouter
8. Sandbox / worktree isolation / GitHub delivery loop
9. Postmortem + Learning

任何“完成”状态都应尽量有验证证据；修改代码后避免仅凭模型文本宣称成功。

## 开发惯例

- 用户可见 GUI 文案优先简体中文。
- 每个改动保持小而可 review，避免把多个独立能力揉成一个提交。
- 核心 Rust 改动后至少运行对应 crate 测试；合入 main 前目标门禁为 fmt、clippy、workspace tests、Desktop tests/build。
- 新状态优先结构化并可序列化，便于 RunInspector、恢复、评测和 telemetry 使用。
- 新 runtime 能力需要考虑 cancellation、timeout、artifact、recovery 与 fail-closed 行为。
