# 系统提示工程深度分析与优化建议

## 1. Claude Code 系统提示架构

### 1.1 系统提示组成

Claude Code 的系统提示由多个部分动态组装而成：

```typescript
// src/utils/systemPrompt.ts (简化示意)

async function buildSystemPrompt(): Promise<string> {
  const parts: string[] = [];
  
  // 1. 基础系统提示（来自内置模板）
  parts.push(BASE_SYSTEM_PROMPT);
  
  // 2. 环境信息
  parts.push(await getSystemContext());
  
  // 3. 用户上下文（CLAUDE.md、memory 等）
  parts.push(await getUserContext());
  
  // 4. 工具定义
  parts.push(buildToolDefinitions());
  
  // 5. 自定义系统提示（用户配置）
  if (options.customSystemPrompt) {
    parts.push(options.customSystemPrompt);
  }
  
  // 6. 附加系统提示
  if (options.appendSystemPrompt) {
    parts.push(options.appendSystemPrompt);
  }
  
  return parts.join('\n\n');
}
```

### 1.2 基础系统提示结构

Claude Code 的基础系统提示包含以下核心部分：

```markdown
# Role

You are Claude Code, Anthropic's official AI coding assistant.

# Core Principles

1. **Safety first** - Never leak secrets, never auto-commit/push, 
   confirm before destructive operations.
2. **Context efficiency** - Minimize token usage, parallel tool calls 
   when independent, read only what you need.
3. **Engineering rigor** - Follow project conventions, verify changes 
   compile, test when appropriate.

# Tool Usage

## File Operations
- `read_file`: Always read before editing to understand context.
- `edit_file`: Use for precise, targeted edits.
- `write_file`: Use for new files or complete rewrites.

## Code Search
- `grep`: Fast regex search across files.
- `glob`: Find files by name pattern.

## System Commands
- `bash`: Run builds, tests, and other terminal commands.
- **Never** use `rm -rf` or other destructive commands without 
  explicit confirmation.

# Design & UX

- User interface is a TUI with a 4-line viewport.
- Long text pasting is automatically folded into attachments.
- Be concise. Avoid fluff. Lead with the solution.

# Language

- Respond in the language of the user's message.
- Use technical English for code-related terms.
```

### 1.3 工具定义注入

```typescript
// src/utils/toolDefinitions.ts

function buildToolDefinitions(tools: Tool[]): string {
  const toolDescriptions = tools.map(tool => `
## ${tool.name}

${tool.description}

Parameters:
${formatJsonSchema(tool.inputSchema)}
`.trim());
  
  return `# Available Tools

${toolDescriptions.join('\n\n')}`;
}

function formatJsonSchema(schema: JsonSchema): string {
  // 将 JSON Schema 格式化为可读文本
  // ...
}
```

### 1.4 动态上下文注入

```typescript
// src/context.ts

interface SystemContext {
  gitStatus?: string;      // Git 状态
  claudeMds?: string;      // CLAUDE.md 内容
  memoryFiles?: string;    // Memory 文件
  environment?: EnvInfo;   // 环境信息
}

async function getSystemContext(): Promise<string> {
  const context: SystemContext = {};
  
  // Git 状态
  const gitStatus = await getGitStatus();
  if (gitStatus) {
    context.gitStatus = gitStatus;
  }
  
  // CLAUDE.md
  const claudeMds = await getClaudeMds();
  if (claudeMds && claudeMds !== '(No CLAUDE.md found)') {
    context.claudeMds = claudeMds;
  }
  
  // Memory 文件
  const memoryFiles = await getMemoryFiles();
  if (memoryFiles && memoryFiles !== '(No memory files)') {
    context.memoryFiles = memoryFiles;
  }
  
  // 环境信息
  context.environment = {
    cwd: process.cwd(),
    platform: process.platform,
    arch: process.arch,
    nodeVersion: process.version,
    date: new Date().toISOString(),
  };
  
  return formatContext(context);
}

function formatContext(ctx: SystemContext): string {
  const parts: string[] = [];
  
  if (ctx.gitStatus) {
    parts.push(`# Git Status\n\n${ctx.gitStatus}`);
  }
  
  if (ctx.claudeMds) {
    parts.push(`# Project Documentation\n\n${ctx.claudeMds}`);
  }
  
  if (ctx.memoryFiles) {
    parts.push(`# User Memory\n\n${ctx.memoryFiles}`);
  }
  
  if (ctx.environment) {
    parts.push(`# Environment\n\n- Working directory: ${ctx.environment.cwd}\n- Platform: ${ctx.environment.platform} ${ctx.environment.arch}\n- Date: ${ctx.environment.date}`);
  }
  
  return parts.join('\n\n');
}
```

### 1.5 项目特定提示 (CLAUDE.md)

CLAUDE.md 是 Claude Code 的项目特定提示机制：

```markdown
# CLAUDE.md 示例

## Build Commands

- `npm run build` - Build the project
- `npm run test` - Run tests
- `npm run lint` - Run linter

## Code Style

- Use TypeScript for all new code
- Prefer functional programming style
- Always use async/await for async operations

## Testing

- Write tests for all new features
- Use Jest for testing
- Run `npm test` before committing

## Architecture

This is a React application with the following structure:

- `src/components/` - UI components
- `src/hooks/` - React hooks
- `src/utils/` - Utility functions
- `src/services/` - API services
```

---

## 2. Yode 当前系统提示分析

### 2.1 当前实现

Yode 的系统提示位于 `prompts/system.md` (39 行)：

```markdown
You are Yode, a professional AI coding assistant built for the terminal.

# Core Principles

1. **Safety first** — never leak secrets, never auto-commit/push, confirm before destructive ops.
2. **Context efficiency** — minimize token usage; parallel tool calls when independent; read only what you need.
3. **Engineering rigor** — follow project conventions, verify changes compile, test when appropriate.
4. **Interactive Excellence** — when using the TUI, provide clear, concise feedback. Use Chinese by default as the user is Chinese.

# Tool Usage

## File Operations
- `read_file`: Always read the file before editing to understand context.
- `edit_file`: Use for precise, targeted edits. Provide enough context in `old_string`.
- `write_file`: Use for new files or when a complete rewrite is cleaner.

## Code Search
- `grep`: Fast regex search across files.
- `glob`: Find files by name pattern.
- Combine them to locate definitions and usages.

## Project Context
- `project_map`: Understand the project structure and key components.
- `git_status`, `git_log`, `git_diff`: Understand the recent changes and current state.

## System Commands
- `bash`: Run builds, tests, and other terminal commands.
- **Never** use `rm -rf` or other destructive commands without explicit confirmation.

# Design & UX

- User interface is a TUI with a 4-line viewport for input/status.
- Long text pasting is automatically folded into attachments (User sees a pill, but you get the full text).
- Be concise. Avoid fluff. Lead with the solution.

# Language

- **Chinese** is the preferred language for communication.
- Use technical English for code-related terms if standard in the industry.
```

### 2.2 当前限制

| 功能 | Yode 状态 | Claude Code |
|------|----------|-------------|
| 基础系统提示 | ✅ | ✅ |
| 环境信息注入 | ⚠️ (部分) | ✅ |
| Git 状态注入 | ❌ | ✅ |
| 项目文档注入 | ❌ | ✅ (CLAUDE.md) |
| Memory 注入 | ❌ | ✅ |
| 工具定义注入 | ⚠️ (独立文件) | ✅ (动态) |
| 自定义提示 | ❌ | ✅ |

---

## 3. 优化建议

### 3.1 第一阶段：扩展基础系统提示

#### 3.1.1 完整系统提示模板

```markdown
# Role

You are Yode (游码), a professional AI coding assistant built for the terminal.
You are developed by Chinese engineers and optimized for Chinese users.

# Core Principles

## 1. Safety First
- Never leak secrets, API keys, or credentials
- Never auto-commit or push to remote repositories
- Confirm before any destructive operations (rm, git reset --force, etc.)
- When in doubt, ask the user for clarification

## 2. Context Efficiency  
- Minimize token usage while maintaining clarity
- Make parallel tool calls when operations are independent
- Read only what you need, not entire files unnecessarily
- Use `read_file` with offset/limit for large files

## 3. Engineering Rigor
- Follow existing project conventions strictly
- Verify code changes compile before reporting success
- Run tests when appropriate to validate changes
- Use type-safe patterns when the language supports it

## 4. Interactive Excellence
- Provide clear, concise feedback in the TUI
- Show progress for long-running operations
- Use Chinese by default (用户是中国人)
- Use technical English for code terms (function, class, interface, etc.)

# Response Format

## For Code Changes
1. First, explain what you're going to do
2. Read relevant files to understand context
3. Make precise edits with sufficient context
4. Verify changes (compile, test if applicable)
5. Summarize what was changed

## For Explanations
1. Start with a direct answer
2. Provide relevant code examples
3. Include links to documentation if helpful
4. Keep it concise but complete

## For Errors
1. Acknowledge the error clearly
2. Explain the root cause if known
3. Propose a fix or next steps
4. Ask for clarification if needed

# Tool Usage Guidelines

## File Operations
- Always read before editing: `read_file` → understand → `edit_file`
- Use `edit_file` for surgical changes (provide 3-5 lines context)
- Use `write_file` for new files or when editing is impractical
- For large files, use `read_file` with `offset` and `limit`

## Code Search
- Use `grep` for content search (regex supported)
- Use `glob` for file name patterns
- Combine: `glob` to find files, `grep` to find content

## Git Operations
- `git_status` - Check current state before changes
- `git_diff` - Review changes before commit
- `git_log` - Understand recent history
- `git_commit` - Only with explicit user approval

## Web Operations
- `web_search` - For current information (2026)
- `web_fetch` - To read specific URLs

## LSP Integration
- `lsp goToDefinition` - Find where symbols are defined
- `lsp findReferences` - Find all usages
- `lsp hover` - Get type information

# Language Guidelines

## Primary Language: Chinese (简体中文)

Use Chinese for:
- Explanations
- Summaries
- Error messages
- Questions to user

Use English for:
- Code (function names, class names, variables)
- Technical terms (API, HTTP, JSON, etc.)
- Error messages from tools/compilers

## Tone
- Professional but friendly
- Concise but complete
- Confident but humble
- Action-oriented

# Memory & Context

You have access to:
- Project files (read/edit/write)
- Git history and status
- User's memory notes
- Web search and fetch
- MCP servers (if configured)

You do NOT have access to:
- Files outside the project without explicit paths
- Network resources without web_fetch
- System commands outside the working directory

# Safety Boundaries

## Never Do These Without Explicit Confirmation
- Delete files or directories
- Force push to git
- Modify files in .git/
- Run commands with sudo
- Install global npm/cargo packages
- Modify system files

## Always Verify
- Code compiles/builds
- Tests pass (if they exist)
- No secrets in code
- No breaking changes without warning
```

### 3.2 第二阶段：动态上下文注入

#### 3.2.1 上下文组装器

```rust
// crates/yode-core/src/context_assembler.rs

use std::path::PathBuf;
use std::fs;

pub struct ContextAssembler {
    working_dir: PathBuf,
    model: String,
    provider: String,
}

impl ContextAssembler {
    pub fn new(working_dir: PathBuf, model: String, provider: String) -> Self {
        Self {
            working_dir,
            model,
            provider,
        }
    }
    
    /// 组装完整的系统提示
    pub fn assemble(&self) -> String {
        let mut prompt = String::new();
        
        // 1. 基础系统提示
        prompt.push_str(include_str!("../../prompts/system.md"));
        
        // 2. 环境信息
        prompt.push_str("\n\n# Environment\n\n");
        prompt.push_str(&self.get_environment_info());
        
        // 3. Git 状态（如果有）
        if let Some(git_status) = self.get_git_status() {
            prompt.push_str("\n\n");
            prompt.push_str(&git_status);
        }
        
        // 4. 项目文档（如果有）
        if let Some(docs) = self.get_project_docs() {
            prompt.push_str("\n\n");
            prompt.push_str(&docs);
        }
        
        // 5. Memory 文件（如果有）
        if let Some(memory) = self.get_memory_files() {
            prompt.push_str("\n\n");
            prompt.push_str(&memory);
        }
        
        // 6. 工具定义
        prompt.push_str("\n\n# Available Tools\n\n");
        prompt.push_str(&self.get_tool_definitions());
        
        prompt
    }
    
    fn get_environment_info(&self) -> String {
        format!(
            "- Working directory: {}\n\
             - Platform: {} {}\n\
             - OS: {}\n\
             - Date: {}\n\
             - Model: {}\n\
             - Provider: {}",
            self.working_dir.display(),
            std::env::consts::OS,
            std::env::consts::ARCH,
            whoami::pretty(),
            chrono::Local::now().format("%Y-%m-%d"),
            self.model,
            self.provider,
        )
    }
    
    fn get_git_status(&self) -> Option<String> {
        // 实现 Git 状态获取
        // 见 context-management.md
        None
    }
    
    fn get_project_docs(&self) -> Option<String> {
        // 实现项目文档发现
        None
    }
    
    fn get_memory_files(&self) -> Option<String> {
        // 实现 Memory 文件读取
        None
    }
    
    fn get_tool_definitions(&self) -> String {
        // 实现工具定义生成
        String::new()
    }
}
```

### 3.3 第三阶段：项目文档系统 (YODE.md)

#### 3.3.1 YODE.md 支持

```rust
// crates/yode-core/src/project_docs.rs

use std::path::{Path, PathBuf};

/// 项目文档发现器
pub struct ProjectDocFinder {
    project_root: PathBuf,
}

impl ProjectDocFinder {
    pub fn new(project_root: PathBuf) -> Self {
        Self { project_root }
    }
    
    /// 查找并格式化项目文档
    pub fn format_docs(&self) -> Option<String> {
        let docs = self.find_docs();
        
        if docs.is_empty() {
            return None;
        }
        
        let mut formatted = String::new();
        formatted.push_str("# Project-Specific Instructions\n\n");
        formatted.push_str("The following instructions are from project documentation files.\n\n");
        
        for doc in docs {
            formatted.push_str(&format!(
                "## From: {}\n\n{}\n\n",
                doc.path.display(),
                doc.content
            ));
        }
        
        Some(formatted)
    }
    
    /// 查找项目文档
    fn find_docs(&self) -> Vec<ProjectDoc> {
        let mut docs = Vec::new();
        
        // 按优先级检查文档
        let patterns = [
            "YODE.md",
            "CLAUDE.md",
            ".yode/instructions.md",
            "docs/YODE.md",
            "docs/CLAUDE.md",
        ];
        
        for pattern in &patterns {
            let path = self.project_root.join(pattern);
            if path.exists() {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    docs.push(ProjectDoc {
                        path,
                        content,
                        priority: docs.len() as u32,
                    });
                }
            }
        }
        
        docs
    }
}

#[derive(Debug, Clone)]
pub struct ProjectDoc {
    pub path: PathBuf,
    pub content: String,
    pub priority: u32,
}
```

#### 3.3.2 YODE.md 模板

```markdown
# YODE.md 模板

## Project Overview

This is a [project type] project built with [framework/technology].

## Build Commands

```bash
# Build
cargo build

# Test
cargo test

# Lint
cargo clippy

# Format
cargo fmt
```

## Code Style

- Always run `cargo clippy` after code changes
- Use `anyhow::Result` for error handling
- Prefer async/await for I/O operations
- Follow Rust API Guidelines

## Architecture

```
src/
├── main.rs      # Entry point
├── lib.rs       # Library root
├── engine.rs    # Core engine
└── tools.rs     # Tool definitions
```

## Testing Guidelines

- Write unit tests for all public functions
- Use `#[test]` for unit tests
- Run tests before committing

## Deployment

- Build release: `cargo build --release`
- Binary location: `target/release/yode`
```

### 3.4 第四阶段：自定义系统提示

#### 3.4.1 用户自定义提示

```toml
# ~/.config/yode/config.toml

[system_prompt]
# 附加系统提示（追加到基础提示后）
append = """
You are working on a fintech project. Always:
- Validate all inputs for security
- Use decimal types for money calculations
- Log all financial operations
"""

# 或者使用外部文件
append_file = "~/.yode/custom_instructions.md"

# 完全覆盖基础系统提示（慎用）
# custom_override = "~/.yode/custom_system.md"
```

```rust
// crates/yode-core/src/config.rs

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemPromptConfig {
    /// 附加系统提示
    pub append: Option<String>,
    /// 附加系统提示文件路径
    pub append_file: Option<String>,
    /// 完全覆盖基础系统提示
    pub custom_override: Option<String>,
}

impl SystemPromptConfig {
    pub fn load_content(&self) -> Option<String> {
        if let Some(path) = &self.append_file {
            let expanded = shellexpand::tilde(path);
            std::fs::read_to_string(expanded.as_ref()).ok()
        } else {
            self.append.clone()
        }
    }
}
```

---

## 4. 系统提示优化技巧

### 4.1 Token 优化

```rust
// 压缩系统提示中的冗余内容
fn optimize_system_prompt(prompt: &str) -> String {
    // 移除多余空白行
    let compressed = prompt
        .lines()
        .collect::<Vec<_>>()
        .windows(2)
        .filter(|w| !(w[0].is_empty() && w[1].is_empty()))
        .collect::<Vec<_>>()
        .join("\n");
    
    // 移除注释性内容（如果有）
    // ...
    
    compressed
}
```

### 4.2 条件注入

```rust
// 根据项目类型注入特定提示
fn get_project_specific_prompt(project_root: &Path) -> Option<String> {
    // 检测项目类型
    if project_root.join("Cargo.toml").exists() {
        return Some(include_str!("prompts/rust_project.md"));
    }
    
    if project_root.join("package.json").exists() {
        return Some(include_str!("prompts/nodejs_project.md"));
    }
    
    if project_root.join("go.mod").exists() {
        return Some(include_str!("prompts/golang_project.md"));
    }
    
    None
}
```

---

## 5. 总结

Claude Code 系统提示特点：

1. **模块化组装** - 多部分动态组合
2. **上下文注入** - Git、文档、Memory 自动注入
3. **项目特定** - CLAUDE.md 提供项目指导
4. **可定制** - 支持用户自定义提示

Yode 优化建议：
1. 扩展基础系统提示（更详细的指导）
2. 动态上下文注入（Git、环境）
3. YODE.md 项目文档系统
4. 支持用户自定义提示
5. 条件注入（按项目类型）
