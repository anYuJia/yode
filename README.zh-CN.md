<div align="center">

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="assets/logo-dark.svg">
  <source media="(prefers-color-scheme: light)" srcset="assets/logo-light.svg">
  <img alt="Yode" src="assets/logo-dark.svg" width="220">
</picture>

### 面向终端的本地优先 AI 编程代理 runtime

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/Rust-1.75+-orange.svg)](https://www.rust-lang.org/)
[![Release](https://img.shields.io/github/v/release/anYuJia/yode?color=green)](https://github.com/anYuJia/yode/releases)
[![Stars](https://img.shields.io/github/stars/anYuJia/yode?style=social)](https://github.com/anYuJia/yode)

[English](README.md) | **中文**

</div>

---

**Yode** 是一个开源编程代理，目标是在 Rust 原生、可检查、终端优先的工作流里，提供接近 Claude Code 的核心使用体验。

它围绕三件事设计：

- **直接在仓库里行动。** 读写文件、搜索、运行 shell、使用 LSP、审查 diff、执行 workflow、协调 agents，都在一个终端会话内完成。
- **让 runtime 可见。** permissions、hooks、MCP servers、startup settings、后台任务、remote sessions、compact/restore 状态都能被检查和复盘。
- **保持本地优先。** 配置、artifact、任务历史、checkpoint 和 operator commands 都保留在你能审计、恢复、接力的位置。

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

## 安装

### 一键安装（macOS / Linux）

```bash
curl -fsSL https://raw.githubusercontent.com/anYuJia/yode/main/install.sh | bash
```

### Cargo

```bash
cargo install --git https://github.com/anYuJia/yode.git --tag v0.0.18
```

### Windows

从 [Releases](https://github.com/anYuJia/yode/releases) 下载 `yode-x86_64-pc-windows-msvc.zip`，或使用 PowerShell 安装：

```powershell
iwr -useb https://raw.githubusercontent.com/anYuJia/yode/main/install.ps1 | iex
```

### 从源码安装

```bash
git clone https://github.com/anYuJia/yode.git
cd yode
cargo install --path .
```

## 快速开始

```bash
# 设置一个 provider API key
export ANTHROPIC_API_KEY="..."
# 或 OPENAI_API_KEY / GEMINI_API_KEY

# 第一次使用时配置 provider
yode provider add

# 启动 TUI
yode

# 非交互单轮模式
yode --chat "Summarize the repository structure"

# 显式指定 provider / model
yode --provider anthropic --model <model-name>

# 恢复历史会话
yode --resume <session-id>

# 检查本地环境
yode doctor
```

## 核心体验

### Agent 工具

Yode 内置一套覆盖实际编程工作的工具面：

| 范围 | 工具 |
| --- | --- |
| 代码 | `read_file`、`write_file`、`edit_file`、`glob`、`grep`、`bash`、`lsp` |
| Review | `review_changes`、`review_pipeline`、`review_then_commit` |
| Workflow | `workflow_run`、`workflow_run_with_writes`、worktree tools、plan-mode tools |
| Agents | `agent`、`team_create`、`send_message`、`team_monitor`、`coordinate_agents` |
| Remote | `remote_queue_dispatch`、`remote_queue_result`、`remote_transport_control` |
| Runtime | `task_output`、`tool_search`、cron tools、MCP resource tools |

### Operator Commands

runtime 不是黑盒，可以在产品内部直接检查：

| 命令 | 用途 |
| --- | --- |
| `/status` | 会话与 runtime 全量快照 |
| `/brief` | 当前状态的紧凑摘要 |
| `/diagnostics` | runtime health / observability 总览 |
| `/inspect artifact summary` | artifact family 总入口 |
| `/tools diag` | tool pool、hidden/deferred tool、失败诊断 |
| `/permissions mode` | permission mode 指南与切换 |
| `/tasks monitor` | 后台 runtime task 工作区 |
| `/remote-control monitor` | remote live-session monitor 面 |
| `/mcp` | MCP 与 settings control-plane 摘要 |
| `/workflows` | workflow 检查与运行入口 |
| `/checkpoint` | 保存、恢复、branch、rewind、rollback 会话状态 |

### Governance 与 Safety

Yode 面向严肃本地开发提供控制平面：

- permission modes：`default`、`plan`、`auto`、`accept-edits`、`bypass`
- 可检查的 permission governance 与 precedence chain
- 覆盖 tool / task / sub-agent / worktree 的 hook lifecycle
- hook `defer` 支持和可恢复 artifact/state
- 危险 shell 行为检测与 runtime confirmation 规则

### MCP 与 Managed Settings

MCP 和 settings 也是可检查 runtime 的一部分：

- provider inventory artifact
- settings scope artifact
- managed MCP inventory artifact
- tool-search activation artifact
- 面向 operator 的 MCP diagnostics 与 remediation 跳转

## CLI 入口

| 命令 | 说明 |
| --- | --- |
| `yode` | 启动 TUI |
| `yode --chat "<msg>"` | 非交互单轮模式 |
| `yode --serve-mcp` | 以 stdio MCP server 方式运行 |
| `yode provider list` | 列出已配置 provider |
| `yode provider add` | 交互式 provider 配置 |
| `yode update check` | 检查并应用更新 |
| `yode completions zsh` | 生成 shell 补全 |
| `yode doctor` | 环境健康检查 |

## 配置

Yode 会从多层来源合并配置与 governance 状态：

- `~/.yode/managed-config.toml`
- `~/.yode/config.toml`
- `.yode/config.toml`
- `.yode/config.local.toml`
- session 与 CLI override

示例：

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

## 项目指令文件

Yode 会加载多种兼容命名的项目说明文件，包括：

- `YODE.md`
- `docs/YODE.md`
- `.yode/instructions.md`
- `CLAUDE.md`
- `AGENTS.md`
- `.claude/CLAUDE.md`

示例：

```markdown
# Project Guidelines

- Run `cargo test -p yode-core --lib` after engine changes
- Prefer small reviewable patches
- Keep migration notes in `docs/optimization/`
```

## 回归快照

修改命令输出或 operator surface 时，优先使用这些脚本留存快照：

- `./scripts/output-regression-snapshot.sh`
  写入多 surface 输出快照到 `.yode/benchmarks/output-regression-snapshot.md`。
- `./scripts/split-output-regression-snapshot.sh`
  将组合快照拆成按 section 分组的 markdown 文件。
- `./scripts/diff-output-regression-snapshot.sh`
  在临时目录重新生成快照，并和保存的 baseline 对比。
- `./scripts/benchmark-snapshot.sh`
  写入长会话 benchmark 快照到 `.yode/benchmarks/long-session-benchmark.md`。

## 架构

```text
crates/
├── yode-core     # engine、context、permissions、hooks、session/runtime state
├── yode-llm      # provider abstraction
├── yode-tools    # built-in tools 与 runtime tool surface
├── yode-tui      # terminal UI 与 operator commands
├── yode-mcp      # MCP integration
└── yode-agent    # agent/runtime helpers
```

## 最新版本

`0.0.18` 重点改进长会话 remote continuity、skill persistence 与 remote replay/state diagnostics：

- compact boundary 会作为一等 session event 记录
- restore blocks 会在 compact 后保留最近使用的 skills
- remote transport events 会写入 durable JSONL logs，并支持 replay diagnostics
- remote control summaries 会展示重建后的 transport 与 queue state

Release: [v0.0.18](https://github.com/anYuJia/yode/releases/tag/v0.0.18)

## 贡献

欢迎贡献。

1. Fork 仓库
2. 新建分支
3. 做聚焦、可 review 的改动
4. 运行相关检查
5. 提交 PR

## 许可证

[MIT](LICENSE)
