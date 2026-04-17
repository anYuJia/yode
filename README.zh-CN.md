<div align="center">

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="assets/logo-dark.svg">
  <source media="(prefers-color-scheme: light)" srcset="assets/logo-light.svg">
  <img alt="Yode" src="assets/logo-dark.svg" width="200">
</picture>

### 用 Rust 构建的终端原生 AI 编程代理 runtime

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/Rust-1.75+-orange.svg)](https://www.rust-lang.org/)
[![Release](https://img.shields.io/github/v/release/anYuJia/yode?color=green)](https://github.com/anYuJia/yode/releases)
[![Stars](https://img.shields.io/github/stars/anYuJia/yode?style=social)](https://github.com/anYuJia/yode)

[English](README.md) | **中文**

</div>

---

**Yode** 面向希望在本地终端里获得“真正可操作的 AI 编程 runtime”体验的用户：

- 有内置工具：读写文件、搜索、shell、web、LSP、workflow、review、MCP
- 有 operator surface：`/status`、`/brief`、`/diagnostics`、`/inspect`、`/tasks`、`/remote-control`、`/checkpoint`
- 有可回看的 runtime artifact：permissions、hooks、team、remote session、startup settings、task history
- 有一整套向 Claude Code 风格靠近的 tool/runtime 平台，而不是只有聊天框

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
cargo install --git https://github.com/anYuJia/yode.git --tag v0.0.12
```

### 从源码安装

```bash
git clone https://github.com/anYuJia/yode.git
cd yode
cargo install --path .
```

### Windows

从 [Releases](https://github.com/anYuJia/yode/releases) 下载 `yode-x86_64-pc-windows-msvc.zip`。

## 快速开始

```bash
# 先设置一个 provider 的 API key
export ANTHROPIC_API_KEY="..."
# 或 OPENAI_API_KEY / GEMINI_API_KEY

# 启动 TUI
yode

# 非交互单轮模式
yode --chat "Summarize the repository structure"

# 显式指定 provider / model
yode --provider anthropic --model <model-name>

# 恢复历史会话
yode --resume <session-id>

# 环境检查
yode doctor
```

如果还没有 provider 配置，先运行：

```bash
yode provider add
```

## 为什么是 Yode

### 1. 它是 Tool Runtime，不只是聊天 UI

Yode 不是简单把 LLM 包在 shell 外面，而是自带一套 runtime plane：

- 基础代码工具：`read_file`、`write_file`、`edit_file`、`glob`、`grep`、`bash`、`lsp`
- 编排工具：`agent`、`team_create`、`send_message`、`team_monitor`、`coordinate_agents`
- workflow / review 工具：`workflow_run`、`workflow_run_with_writes`、`review_changes`、`review_pipeline`、`review_then_commit`
- remote runtime 工具：`remote_queue_dispatch`、`remote_queue_result`、`remote_transport_control`
- 运行时辅助：`task_output`、`tool_search`、plan mode、worktree、cron、MCP resource

### 2. 它有 Inspectable Operator Surface

runtime 不是黑盒，可以在产品内部被复盘和诊断：

- `/status`、`/brief`、`/diagnostics` 看整体状态
- `/inspect artifact ...` 看 startup、runtime、hook、permission、team、remote 产物
- `/tasks monitor`、`/tasks follow latest` 看后台任务
- `/remote-control monitor`、`/remote-control queue`、`/remote-control follow latest` 看远端/持续运行面
- `/checkpoint` 做 session checkpoint、branch、rewind、restore、rollback 类操作

### 3. 它有 Governance、Hooks 和 Safety

相比早期版本，Yode 现在有更完整的控制平面：

- permission modes：`default`、`plan`、`auto`、`accept-edits`、`bypass`
- 可检查的 permission governance 与 precedence chain
- 覆盖 tool / task / sub-agent / worktree 的 hook lifecycle
- hook `defer` 语义和可恢复 artifact/state
- 对危险 shell 行为的检测与 runtime confirmation 规则

### 4. 它把 MCP 和 Managed Settings 也做成了可见平面

不再只是“当前加载了哪些工具”：

- provider inventory artifact
- settings scope artifact
- managed MCP inventory artifact
- tool-search activation artifact
- 面向 operator 的 MCP diagnostics 与 remediation 跳转

## 建议先掌握的命令

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
| `/checkpoint` | 保存/恢复/branch/rewind/rollback |

## 常用 CLI 入口

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

## 配置分层

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

## 0.0.12 版本重点

`0.0.12` 重点补齐了 Claude Code 风格的渲染结构与重试恢复：

- fenced code block 已拆成独立的 `HighlightedCode` 和 `StructuredDiff` 渲染路径，不再走单一路径的 markdown code renderer
- diff 渲染现在支持行号 gutter、增删背景、基于文件路径的语言探测，以及相邻增删行的 word-level 强调
- 当网络恢复、流式输出重新开始后，顶部状态会从 `Retrying` 正确恢复到 `Working`，不会再卡住旧的 429 重试提示

Release: [v0.0.12](https://github.com/anYuJia/yode/releases/tag/v0.0.12)

## 贡献

欢迎贡献。

1. Fork 仓库
2. 新建分支
3. 做聚焦、可 review 的改动
4. 运行相关检查
5. 提交 PR

## 许可证

[MIT](LICENSE)
