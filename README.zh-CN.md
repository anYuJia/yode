<div align="center">

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="assets/logo-dark.svg">
  <source media="(prefers-color-scheme: light)" srcset="assets/logo-light.svg">
  <img alt="Yode" src="assets/logo-dark.svg" width="220">
</picture>

### 本地优先的桌面 AI 编程 Agent

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/Rust-1.75+-orange.svg)](https://www.rust-lang.org/)
[![Release](https://img.shields.io/github/v/release/anYuJia/yode?color=green)](https://github.com/anYuJia/yode/releases)
[![Stars](https://img.shields.io/github/stars/anYuJia/yode?style=social)](https://github.com/anYuJia/yode)

[English](README.md) | **中文**

</div>

---

**Yode** 是一个使用 Rust、Tauri 和 React 构建的开源 Desktop-first AI 编程 Agent，目标是把长任务中的规划、执行、验证、恢复和可观测性统一放在一个可检查的本地桌面应用里。

Yode 已经停止把 CLI/TUI 作为产品方向。正式产品入口只有 `apps/yode-desktop/`；可复用 Agent 能力放在 Rust crates 中，再通过 Tauri runtime 暴露给 GUI。

## 产品原则

- **直接在仓库里行动。** 读写文件、搜索、运行命令、使用 LSP、审查 diff、协调 Agents、管理 worktree，都在同一个桌面工作区完成。
- **验证后再交付。** 修改代码的 run 应产生验证证据，不能只凭模型文本宣称“完成”。
- **让 runtime 可见。** Run、权限、工具 trace、后台任务、恢复状态、成本、浏览器动作、artifact、评测结果都应能被 GUI 检查。
- **保持本地优先。** 项目状态、artifact、索引、评测数据和恢复信息都保留在可审计的本地文件中。

## Desktop 能力

| 范围 | 当前方向 |
| --- | --- |
| Agent Runtime | RunController 生命周期、硬预算、取消、恢复、持久化运行状态 |
| 代码理解 | Repository Map、LSP、持久 Repository Intelligence 索引 |
| 验证 | Acceptance Criteria、Evidence、测试/Review Verification Gate |
| Multi-Agent | Planning、DAG 编排、真正并发 ready batch、后台 Agent |
| Browser | 真实 Chromium CDP runtime，支持导航、交互、JS 与截图 |
| Evaluation | YodeBench task/outcome/metrics 与 workspace artifact |
| Safety | Workspace Trust、Permission Governance、fail-closed runtime |
| Extensibility | MCP、Skills、Hooks、Tools、Provider abstraction |

## 安装

请直接从 GitHub Releases 下载 macOS、Windows 或 Linux 的桌面应用。

Yode 不再提供或维护根 CLI binary、Cargo install 产品路径、shell completion 包或 TUI installer。

## 从源码运行

### 环境要求

- Rust toolchain
- Node.js
- pnpm
- Tauri 对应平台依赖

### 开发

```bash
git clone https://github.com/anYuJia/yode.git
cd yode/apps/yode-desktop
pnpm install --frozen-lockfile
pnpm tauri:dev
```

### 验证

```bash
# 仓库根目录
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --no-deps -- -D warnings
cargo check --workspace --all-targets
cargo test --workspace

# Desktop 前端
cd apps/yode-desktop
pnpm test
pnpm build
```

Provider 集成测试已经归属到 provider crate：

```bash
cargo test -p yode-llm --test anthropic_integration
```

## 架构

```text
Desktop React UI
      ↓ Tauri commands / events
apps/yode-desktop/src-tauri
      ↓
┌──────────────────────────────────────────────────────────────┐
│ yode-core      AgentEngine、RunController、Context、Safety  │
│ yode-agent     Planning、Orchestration、Scheduler           │
│ yode-tools     Code/Browser/Runtime tools                   │
│ yode-runtime   Shared runtime bootstrap                     │
├──────────────────────────────────────────────────────────────┤
│ yode-llm       Provider / Model abstraction                 │
│ yode-mcp       MCP integration                              │
│ yode-index     Repository Intelligence                      │
│ yode-evals     YodeBench                                    │
└──────────────────────────────────────────────────────────────┘
```

仓库根目录现在是 **virtual Cargo workspace**，有意不再存在根 `src/main.rs` 或根 `yode` 可执行程序。

## Agent 工具

Yode runtime 已经拥有较完整的工具面，包括文件操作、搜索、shell、LSP、review、worktree、MCP、repository intelligence、verification agent、后台任务、多 Agent 协调、浏览器控制和 Git 操作。

项目下一阶段不会继续靠“堆更多工具”提升能力，而是优先强化闭环：

```text
Intent
  → Analyze / Acceptance Criteria
  → Plan
  → Execute
  → Verify
      ├─ 通过 → Deliver
      └─ 失败 → Replan → Execute
  → Evaluate / Postmortem / Learn
```

## Runtime Artifacts

项目内 `.yode/` 用于保存可检查的 runtime 数据，例如：

- run / session artifact
- verification / review evidence
- repository index
- browser screenshot/cache
- YodeBench task/outcome
- transcript、plan、checkpoint、recovery 信息

## 项目指令

Yode 可以加载兼容的项目指令文件，包括：

- `YODE.md`
- `docs/YODE.md`
- `.yode/instructions.md`
- `CLAUDE.md`
- `AGENTS.md`
- `.claude/CLAUDE.md`

对开发 Yode 本身的 Coding Agent 来说，`AGENTS.md` 是产品方向的唯一权威说明。特别是：新增用户能力必须面向 Desktop GUI，不要重新创建 CLI/TUI 产品面。

## 发布

Tag release 通过 Tauri Desktop release workflow 构建 macOS、Windows 和 Linux 应用。发布前会执行 Rust fmt、clippy、workspace check/test 和依赖审计。

## 贡献

欢迎贡献。请保持提交聚焦，保留 fail-closed 行为，对行为修改增加验证，并在提交前运行相关 Rust 与 Desktop 检查。

## 许可证

[MIT](LICENSE)
