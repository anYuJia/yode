# AGENTS.md

This file provides guidance to Codex (Codex.ai/code) when working with code in this repository.

## 项目概述

Yode 是用 Rust 构建的终端原生 AI 编程助手，支持多 LLM 提供商、工具调用、会话管理和 TUI 界面。

## 常用命令

### 构建和运行

```bash
# 构建整个项目
cargo build

# 运行 Yode
cargo run

# 运行桌面应用（Tauri）
cargo run -p yode-desktop
```

### 测试

```bash
# 运行所有测试
cargo test

# 运行特定 crate 的测试
cargo test -p yode-core

# 运行单个测试
cargo test -p yode-core test_version_compare
```

### 代码检查

```bash
# Clippy 检查
cargo clippy

# 格式化代码
cargo fmt
```

## 代码架构

### Workspace 结构

```
yode/
├── Cargo.toml              # Workspace 根目录
├── src/main.rs             # CLI 入口（含 TUI）
├── crates/
│   ├── yode-core/          # 核心引擎（AgentEngine、上下文、权限、会话 DB、信任存储）
│   ├── yode-llm/           # LLM 抽象层（提供商接口、消息类型）
│   ├── yode-tools/         # 工具系统（bash、文件操作、LSP 等）
│   ├── yode-mcp/           # MCP 协议支持
│   ├── yode-agent/         # Agent 编排
│   └── yode-runtime/       # 共享运行时装配（provider bootstrap 等）
└── config/
    └── default.toml        # 默认配置
```

### 核心模块依赖关系

```
yode (main, 含 TUI)
  ├── yode-core (引擎、上下文、权限)
  ├── yode-llm (LLM 抽象)
  ├── yode-tools (工具注册表)
  └── yode-runtime (运行时装配)
```

桌面应用：`apps/yode-desktop/`（Tauri 后端 src-tauri + React 前端）

### yode-core 核心组件

- `engine.rs` - AgentEngine：主循环、工具调用、消息处理
- `context.rs` - AgentContext：会话上下文（model、session_id、effort level）
- `permission.rs` - PermissionManager：权限控制（default/auto/plan/bypass 模式）
- `db.rs` - Database：SQLite 会话和消息存储
- `context_manager.rs` - 上下文压缩管理
- `cost_tracker.rs` - Token 成本追踪
- `hooks.rs` - Hook 系统（pre/post tool hooks）
- `updater.rs` - 自动更新（GitHub Releases、锁机制、版本保留）
- `plugin_trust.rs` / `workspace_trust.rs` - 仓库外信任存储（canonical path + 哈希绑定）

### src/（CLI）架构

- `main.rs` - CLI 入口（clap 解析、更新检查、setup 引导）
- `chat_mode.rs` - 交互/非交互回合循环（引擎装配、预算、Hook 注册）
- `app_bootstrap/` - 启动装配（session_restore 权限分层、tooling 工具注册）

## 开发惯例

- 所有用户可见文本使用中文（简体中文）
- 提交信息使用中文
- 核心逻辑修改后运行 `cargo test -p yode-core`
- 修改后运行完整门禁：`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets --no-deps -- -D warnings`、`cargo test --workspace`

## 参考资料

- Codex 源码参考：`~/code/Codex/Codex-rev`
- 当实现新功能时，优先参考 Codex 的类似实现
