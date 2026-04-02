<div align="center">

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="assets/logo-dark.svg">
  <source media="(prefers-color-scheme: light)" srcset="assets/logo-light.svg">
  <img alt="Yode" src="assets/logo-dark.svg" width="200">
</picture>

### 终端原生的开源 AI 编程助手

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/Rust-1.75+-orange.svg)](https://www.rust-lang.org/)
[![Release](https://img.shields.io/github/v/release/anYuJia/yode?color=green)](https://github.com/anYuJia/yode/releases)
[![Stars](https://img.shields.io/github/stars/anYuJia/yode?style=social)](https://github.com/anYuJia/yode)

[English](README.md) | **中文**

</div>

---

> **Yode** 是一款用 Rust 构建的终端原生 AI 编程助手。
> 它在一个统一的会话中读取、编辑、搜索文件并执行命令。

```
╭─── Yode ──────────────────────────────────────╮
│  claude-sonnet-4-20250514 · ~/my-project       │
╰────────────────────────────────────────────────╯

❯ 修复 login.rs 中的认证 bug

⏺ Read(src/login.rs)
  ⎿  (248 行)

⏺ Edit(src/login.rs)
   - if token.is_expired() { return None; }
   + if token.is_expired() { return Err(AuthError::Expired); }

⏺ 完成。过期的 token 现在会返回正确的错误。
```

## 安装

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

> **Windows**: 从 [Releases](https://github.com/anYuJia/yode/releases) 下载 `yode-x86_64-pc-windows-msvc.zip`。

## 快速开始

```bash
# 设置 API 密钥
export ANTHROPIC_API_KEY="sk-ant-..."   # 或 OPENAI_API_KEY

# 启动 Yode
yode

# 指定模型
yode --model claude-sonnet-4-20250514

# 恢复历史会话
yode --resume <session-id>
```

## 功能特性

### LLM 集成
- **多提供商支持** — OpenAI、Anthropic 或任何 OpenAI 兼容端点
- **流式响应** — 实时 token 流传输，支持取消
- **上下文管理** — 接近上下文限制时自动摘要压缩

### 内置工具
| 工具 | 说明 |
|------|------|
| `bash` | Shell 命令执行，带危险命令检测 |
| `read_file` / `write_file` / `edit_file` | 精确的文件操作 |
| `glob` / `grep` | 快速代码库搜索 |
| `web_fetch` / `web_search` | 网页抓取和网络搜索 |
| `lsp` | 语言服务器集成（跳转定义、查找引用、悬停信息） |
| `agent` | 启动子代理执行并行任务 |
| `memory` | 跨会话持久化记忆 |
| MCP 支持 | 通过 Model Context Protocol 服务器扩展功能 |

### 终端界面
- **Markdown 渲染** — 表格、代码块（语法高亮）、引用块、任务列表
- **Braille 加载动画** 和流式传输指示器
- **滚动导航** 支持输入历史搜索（`Ctrl+R`）
- **权限模式切换**（`Shift+Tab`）
- **工具确认** — `[y]` 允许、`[n]` 拒绝、`[a]` 始终允许
- **文件附件** 支持 `@file` 和 Shell 快捷方式 `!command`
- 括号粘贴模式支持

### 安全与控制
- **权限系统** — 普通、自动接受、计划三种模式
- **危险命令检测** — 阻止破坏性 `git` 操作、`rm -rf` 等
- **会话持久化** — SQLite 存储，支持 `--resume` 恢复

## 快捷键

| 快捷键 | 功能 |
|--------|------|
| `Enter` | 发送消息 |
| `Ctrl+Enter` / `Shift+Enter` | 插入换行 |
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

## 斜杠命令

| 命令 | 说明 |
|------|------|
| `/help` | 显示所有命令 |
| `/keys` | 快捷键参考 |
| `/clear` | 清除聊天显示 |
| `/model` | 显示当前模型 |
| `/provider` | 切换 LLM 提供商 |
| `/providers` | 列出可用提供商 |
| `/tools` | 列出可用工具 |
| `/cost` | 显示 token 用量和预估费用 |
| `/diff` | 显示 `git diff --stat` |
| `/status` | 会话状态摘要 |
| `/context` | 上下文窗口使用情况 |
| `/compact` | 压缩聊天历史 |
| `/copy` | 复制最后一条回复到剪贴板 |
| `/sessions` | 列出历史会话 |
| `/bug` | 生成 bug 报告 |
| `/doctor` | 环境健康检查 |
| `/config` | 显示当前配置 |
| `/version` | 显示版本信息 |

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

在项目根目录创建 `YODE.md` 文件，为 AI 提供项目特定的上下文和指令：

```markdown
# 项目说明

这是一个使用 Actix-web 的 Rust 项目。
- 代码修改后始终运行 `cargo clippy`
- 优先使用 `anyhow::Result` 而非自定义错误类型
- 所有 I/O 操作使用 async/await
```

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

## 贡献

欢迎贡献！以下是帮助方式：

1. Fork 本仓库
2. 创建功能分支 (`git checkout -b feature/amazing-feature`)
3. 提交更改 (`git commit -m 'Add amazing feature'`)
4. 推送分支 (`git push origin feature/amazing-feature`)
5. 创建 Pull Request

## 许可证

[MIT](LICENSE) — 欢迎使用、修改和分发。
