<div align="center">

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="assets/logo-dark.svg">
  <source media="(prefers-color-scheme: light)" srcset="assets/logo-light.svg">
  <img alt="Yode" src="assets/logo-dark.svg" width="200">
</picture>

### 开源终端 AI 编程助手

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/Rust-1.75+-orange.svg)](https://www.rust-lang.org/)
[![Release](https://img.shields.io/github/v/release/anYuJia/yode?color=green)](https://github.com/anYuJia/yode/releases)
[![Stars](https://img.shields.io/github/stars/anYuJia/yode?style=social)](https://github.com/anYuJia/yode)

[English](README.md) | **中文**

</div>

---

## 亮点

- **多模型支持** — 兼容 Anthropic、OpenAI 及任何 OpenAI 兼容 API
- **内置工具** — 在对话中直接读取、编辑、搜索文件并执行命令
- **丰富的终端界面** — Markdown 渲染、流式响应、键盘驱动的工作流
- **原生性能** — 纯 Rust 实现，约 3 MB 二进制，瞬间启动

## 快速开始

### 一键安装（macOS / Linux）

```bash
curl -fsSL https://raw.githubusercontent.com/anYuJia/yode/main/install.sh | bash
```

### Cargo 安装

```bash
cargo install --git https://github.com/anYuJia/yode.git
```

### 从源码编译

```bash
git clone https://github.com/anYuJia/yode.git
cd yode
cargo install --path .
```

### 运行

```bash
# 设置 API 密钥
export ANTHROPIC_API_KEY="sk-ant-..."

# 启动
yode

# 或指定模型
yode --model claude-sonnet-4-20250514

# 恢复历史会话
yode --resume <session-id>
```

## 演示

<!-- TODO: 添加终端录屏 GIF -->
<!-- <img src="assets/demo.gif" width="720" alt="Yode 演示"> -->

```
╭─── Yode ──────────────────────────────────────╮
│  claude-sonnet-4-20250514 · ~/my-project       │
│                                                │
│  ? /help 查看命令 · /keys 查看快捷键           │
╰────────────────────────────────────────────────╯

> 修复 login.rs 中的认证 bug

⏺ 我来检查登录模块并修复问题。

⏺ Read(src/login.rs)
  ⎿  (248 行)

⏺ Edit(src/login.rs)
   - if token.is_expired() { return None; }
   + if token.is_expired() { return Err(AuthError::Expired); }

⏺ 问题在于过期 token 返回了 `None` 而非正确的错误类型...
```

## 功能特性

### LLM 集成
- **多模型提供商** — OpenAI、Anthropic 或任何 OpenAI 兼容端点
- **流式响应** — 实时 token 流传输，支持取消
- **上下文管理** — 接近上下文限制时自动摘要压缩

### 内置工具
- `bash` — 带安全检测的 Shell 命令执行
- `read_file` / `write_file` / `edit_file` — 精确的文件操作
- `glob` / `grep` — 快速代码库搜索
- `web_fetch` / `web_search` — 网页抓取和网络搜索
- `lsp` — 语言服务器集成（跳转定义、查找引用、悬停信息）
- `agent` — 启动子代理执行并行任务
- `memory` — 跨会话持久化记忆
- MCP 服务器支持 — 通过 Model Context Protocol 扩展功能

### 终端界面
- Markdown 渲染 — 表格、代码块（语法高亮）、引用块、任务列表
- Braille 加载动画和流式传输指示
- 滚动条、输入历史搜索（`Ctrl+R`）
- 权限模式切换（`Shift+Tab`）
- 工具确认 — `[y]` 允许、`[n]` 拒绝、`[a]` 始终允许
- `@file` 文件引用和 `!command` Shell 快捷方式
- 括号粘贴模式支持

### 安全与控制
- 权限系统 — 普通、自动接受、计划三种模式
- 危险命令检测（破坏性 git 操作、`rm -rf` 等）
- 会话持久化 — 基于 SQLite，支持 `--resume` 恢复

## 快捷键

| 快捷键 | 功能 |
|--------|------|
| `Enter` | 发送消息 |
| `Ctrl+Enter` | 插入换行 |
| `Ctrl+C` | 停止生成（连按两次退出） |
| `Esc` | 停止生成 |
| `↑` / `↓` | 滚动聊天 |
| `Ctrl+P` / `Ctrl+N` | 浏览输入历史 |
| `Ctrl+R` | 反向搜索历史 |
| `Ctrl+L` | 清屏 |
| `Ctrl+K` | 删除到行尾 |
| `Ctrl+W` | 删除前一个单词 |
| `PageUp` / `PageDown` | 滚动聊天（10 行） |
| `Shift+Tab` | 切换权限模式 |
| `Tab` | 自动补全命令 |

## 命令

| 命令 | 说明 |
|------|------|
| `/help` | 显示所有命令 |
| `/keys` | 快捷键参考 |
| `/clear` | 清除聊天显示 |
| `/model` | 显示当前模型 |
| `/tools` | 列出可用工具 |
| `/cost` | 显示 token 用量和预估费用 |
| `/diff` | 显示 `git diff --stat` |
| `/status` | 会话状态摘要 |
| `/context` | 上下文窗口使用情况 |
| `/compact` | 压缩聊天历史 |
| `/copy` | 复制最后一条回复到剪贴板 |
| `!command` | 直接执行 Shell 命令 |
| `@file` | 附加文件作为上下文 |

## 配置

配置文件位置：`~/.config/yode/config.toml`

```toml
[provider]
default = "anthropic"
model = "claude-sonnet-4-20250514"

[permissions]
# 始终允许、无需确认的工具
allow = ["read_file", "glob", "grep"]
# 始终需要确认的工具
confirm = ["bash", "write_file", "edit_file"]
```

## 架构

```
crates/
├── yode-core     # 引擎、上下文、权限、数据库
├── yode-llm      # LLM 提供商抽象层（OpenAI、Anthropic）
├── yode-tools    # 工具注册表和内置工具
├── yode-tui      # 终端界面（基于 ratatui）
├── yode-mcp      # Model Context Protocol 支持
└── yode-agent    # Agent 编排
```

## 项目级配置

在项目根目录创建 `YODE.md` 文件，为代理提供项目特定的指令：

```markdown
# 项目说明

这是一个使用 Actix-web 的 Rust 项目。
修改代码后始终运行 `cargo clippy`。
优先使用 `anyhow::Result` 而非自定义错误类型。
```

## 贡献

欢迎贡献！请随时提交 Pull Request。

1. Fork 本仓库
2. 创建功能分支 (`git checkout -b feature/amazing-feature`)
3. 提交更改 (`git commit -m 'Add amazing feature'`)
4. 推送分支 (`git push origin feature/amazing-feature`)
5. 创建 Pull Request

## 许可证

MIT — 详见 [LICENSE](LICENSE)。
