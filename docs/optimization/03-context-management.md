# 上下文管理系统深度分析与优化建议

## 1. Claude Code 上下文管理架构

### 1.1 系统上下文生成

Claude Code 的系统上下文位于 `src/context.ts` (221 行)，核心功能：

```typescript
// src/context.ts

// 系统上下文是缓存的，在会话期间不变
export const getSystemContext = memoize(async (): Promise<{
  [k: string]: string
}> => {
  // 获取 git 状态（如果不是 git 仓库则跳过）
  const gitStatus = await getGitStatus();
  
  // 包含系统提示注入（用于缓存破坏，仅限 ant）
  const injection = getSystemPromptInjection();
  
  return {
    ...(gitStatus && { gitStatus }),
    ...(injection ? { cacheBreaker: `[CACHE_BREAKER: ${injection}]` } : {}),
  };
});
```

### 1.2 Git 状态集成

```typescript
// src/context.ts - Git 状态详情

export const getGitStatus = memoize(async (): Promise<string | null> => {
  if (!await getIsGit()) return null;
  
  const [branch, mainBranch, status, log, userName] = await Promise.all([
    getBranch(),                                    // 当前分支
    getDefaultBranch(),                             // 主分支
    execFileNoThrow(gitExe(), ['status', '--short']),
    execFileNoThrow(gitExe(), ['log', '--oneline', '-n', '5']),
    execFileNoThrow(gitExe(), ['config', 'user.name']),
  ]);
  
  // 状态截断（最大 2000 字符）
  const truncatedStatus = status.length > MAX_STATUS_CHARS
    ? status.substring(0, MAX_STATUS_CHARS) + 
      '\n... (truncated, use "git status" for full details)'
    : status;
  
  return [
    `Current branch: ${branch}`,
    `Main branch: ${mainBranch}`,
    `Git user: ${userName}`,
    `Status:\n${truncatedStatus || '(clean)'}`,
    `Recent commits:\n${log}`,
  ].join('\n\n');
});
```

### 1.3 用户上下文生成

```typescript
// src/context.ts

export const getUserContext = memoize(async (): Promise<{
  [k: string]: string
}> => {
  const startTime = Date.now();
  
  // 获取 CLAUDE.md 文件（项目文档）
  const claudeMds = await getClaudeMds();
  
  // 获取 memory files（用户笔记）
  const memoryFiles = await getMemoryFiles();
  
  // 获取额外的 CLAUDE.md 目录
  const additionalDirs = getAdditionalDirectoriesForClaudeMd();
  
  return {
    claudeMds,
    memoryFiles,
    additionalDirs,
  };
});
```

### 1.4 CLAUDE.md 系统

```typescript
// src/utils/claudemd.ts

/**
 * 获取项目中的 CLAUDE.md 文件
 * 这些文件提供项目特定的上下文和指导
 */
export async function getClaudeMds(): Promise<string> {
  const projectRoot = getProjectRoot();
  
  // 搜索 CLAUDE.md 文件（支持多级目录）
  const claudeMdPaths = await glob([
    'CLAUDE.md',
    'docs/CLAUDE.md',
    '.github/CLAUDE.md',
  ], { cwd: projectRoot });
  
  const contents: string[] = [];
  for (const path of claudeMdPaths) {
    const content = await readFile(path, 'utf-8');
    contents.push(`## ${path}\n\n${content}`);
  }
  
  return contents.join('\n\n') || '(No CLAUDE.md found)';
}

/**
 * 获取 memory files（用户笔记）
 * 位于 ~/.claude/projects/{projectHash}/memory/
 */
export async function getMemoryFiles(): Promise<string> {
  const memoryDir = getMemoryDirectoryForProject(getProjectRoot());
  
  const files = await glob('**/*.md', { cwd: memoryDir });
  const contents: string[] = [];
  
  for (const file of files) {
    const content = await readFile(join(memoryDir, file), 'utf-8');
    contents.push(`## ${file}\n\n${content}`);
  }
  
  return contents.join('\n\n') || '(No memory files)';
}
```

### 1.5 上下文压缩策略

Claude Code 的上下文压缩在 `src/utils/contextWindow.ts` 中实现：

```typescript
// 压缩策略配置
const CONTEXT_COMPRESSION = {
  // 触发压缩的阈值（占 context window 的比例）
  compressionThreshold: 0.85,
  
  // 压缩后的目标使用量
  targetUtilization: 0.65,
  
  // 始终保留的最近消息数
  preserveRecentCount: 6,
  
  // 工具结果最大字符数（压缩后）
  maxToolResultChars: 500,
};

/**
 * 上下文压缩策略
 */
export function compressContext(messages: Message[]): Message[] {
  // 1. 始终保留 system message 和最近 N 条消息
  const systemMessage = messages[0];
  const recentMessages = messages.slice(-PRESERVE_RECENT_COUNT);
  
  // 2. 中间部分可以压缩
  const middleMessages = messages.slice(1, -PRESERVE_RECENT_COUNT);
  
  // 3. 截断工具结果
  const truncatedMiddle = middleMessages.map(msg => {
    if (msg.role === 'tool' && msg.content?.length > MAX_TOOL_RESULT_CHARS) {
      return {
        ...msg,
        content: msg.content.substring(0, MAX_TOOL_RESULT_CHARS) + '... [compressed]',
      };
    }
    return msg;
  });
  
  return [systemMessage, ...truncatedMiddle, ...recentMessages];
}
```

---

## 2. Yode 当前上下文管理分析

### 2.1 当前实现

Yode 的上下文管理位于 `crates/yode-core/src/context_manager.rs` (468 行)，已经实现了：

**已实现功能：**
- ✅ 模型上下文窗口限制查询
- ✅ 自动压缩触发（阈值 75%）
- ✅ 字符到 token 的估算（带缓存）
- ✅ 工具结果截断（500 字符）
- ✅ 消息优先级删除
- ✅ 保持最近 6 条消息
- ✅ 系统消息保护

**代码质量评估：**
- 代码结构清晰
- 有完善的测试覆盖（9 个测试用例）
- 使用原子操作估算 token（高效）
- 优先级删除策略合理

### 2.2 当前限制

| 功能 | Yode 状态 | Claude Code |
|------|----------|-------------|
| Git 状态注入 | ❌ | ✅ |
| CLAUDE.md 支持 | ⚠️ (独立技能) | ✅ (内置) |
| Memory 文件 | ⚠️ (独立技能) | ✅ (内置) |
| 动态上下文 | ❌ | ✅ |
| 缓存破坏 | ❌ | ✅ |

---

## 3. 优化建议

### 3.1 第一阶段：Git 状态集成

#### 3.1.1 添加 Git 状态到系统提示

```rust
// crates/yode-core/src/context.rs

use std::process::Command;

/// Git 状态信息
#[derive(Debug, Clone, Default)]
pub struct GitStatus {
    pub current_branch: Option<String>,
    pub main_branch: Option<String>,
    pub git_user: Option<String>,
    pub status: Option<String>,
    pub recent_commits: Option<String>,
    pub is_git_repo: bool,
}

impl GitStatus {
    /// 获取当前目录的 git 状态
    pub fn from_working_dir(working_dir: &std::path::Path) -> Self {
        // 检查是否是 git 仓库
        if !working_dir.join(".git").exists() {
            return Self { is_git_repo: false, ..Default::default() };
        }
        
        let git_cmd = if cfg!(windows) { "git.exe" } else { "git" };
        
        // 并行获取所有 git 信息
        let (branch, main_branch, status, log, user) = std::thread::scope(|s| {
            let branch_handle = s.spawn(|| {
                Command::new(git_cmd)
                    .args(["branch", "--show-current"])
                    .current_dir(working_dir)
                    .output()
                    .ok()
                    .and_then(|o| String::from_utf8(o.stdout).ok())
                    .map(|s| s.trim().to_string())
            });
            
            let main_handle = s.spawn(|| {
                Command::new(git_cmd)
                    .args(["symbolic-ref", "refs/remotes/origin/HEAD"])
                    .current_dir(working_dir)
                    .output()
                    .ok()
                    .and_then(|o| String::from_utf8(o.stdout).ok())
                    .and_then(|s| s.split('/').last().map(|s| s.trim().to_string()))
            });
            
            let status_handle = s.spawn(|| {
                Command::new(git_cmd)
                    .args(["--no-optional-locks", "status", "--short"])
                    .current_dir(working_dir)
                    .output()
                    .ok()
                    .and_then(|o| String::from_utf8(o.stdout).ok())
                    .map(|s| s.trim().to_string())
            });
            
            let log_handle = s.spawn(|| {
                Command::new(git_cmd)
                    .args(["--no-optional-locks", "log", "--oneline", "-n", "5"])
                    .current_dir(working_dir)
                    .output()
                    .ok()
                    .and_then(|o| String::from_utf8(o.stdout).ok())
                    .map(|s| s.trim().to_string())
            });
            
            let user_handle = s.spawn(|| {
                Command::new(git_cmd)
                    .args(["config", "user.name"])
                    .current_dir(working_dir)
                    .output()
                    .ok()
                    .and_then(|o| String::from_utf8(o.stdout).ok())
                    .map(|s| s.trim().to_string())
            });
            
            (
                branch_handle.join().ok().flatten(),
                main_handle.join().ok().flatten(),
                status_handle.join().ok().flatten(),
                log_handle.join().ok().flatten(),
                user_handle.join().ok().flatten(),
            )
        });
        
        Self {
            current_branch: branch,
            main_branch: main_branch,
            status: status,
            recent_commits: log,
            git_user: user,
            is_git_repo: true,
        }
    }
    
    /// 格式化为系统提示
    pub fn format_for_system_prompt(&self) -> String {
        const MAX_STATUS_CHARS: usize = 2000;
        
        let mut parts = vec![
            "Git status at session start (snapshot in time, will not update during conversation):".to_string(),
        ];
        
        if let Some(branch) = &self.current_branch {
            parts.push(format!("Current branch: {}", branch));
        }
        
        if let Some(main) = &self.main_branch {
            parts.push(format!("Main branch (for PRs): {}", main));
        }
        
        if let Some(user) = &self.git_user {
            parts.push(format!("Git user: {}", user));
        }
        
        let status_text = self.status.as_deref().unwrap_or("(clean)");
        let truncated_status = if status_text.len() > MAX_STATUS_CHARS {
            format!(
                "{}\n... (truncated at {} chars. Run `git status` for full details)",
                &status_text[..MAX_STATUS_CHARS],
                status_text.len()
            )
        } else {
            status_text.to_string()
        };
        parts.push(format!("Status:\n{}", truncated_status));
        
        if let Some(log) = &self.recent_commits {
            parts.push(format!("Recent commits:\n{}", log));
        }
        
        parts.join("\n\n")
    }
}
```

#### 3.1.2 集成到 ContextManager

```rust
// crates/yode-core/src/context_manager.rs

use crate::context::GitStatus;

pub struct ContextManager {
    limits: ModelLimits,
    threshold: f64,
    last_known_prompt_tokens: Option<u32>,
    last_known_char_count: Option<usize>,
    /// Git 状态（会话开始时捕获）
    git_status: Option<GitStatus>,
    /// 工作目录
    working_dir: PathBuf,
}

impl ContextManager {
    pub fn new(model: &str, working_dir: PathBuf) -> Self {
        let git_status = GitStatus::from_working_dir(&working_dir);
        
        Self {
            limits: ModelLimits::for_model(model),
            threshold: 0.75,
            last_known_prompt_tokens: None,
            last_known_char_count: None,
            git_status: if git_status.is_git_repo { Some(git_status) } else { None },
            working_dir,
        }
    }
    
    /// 获取系统上下文（包括 git 状态）
    pub fn get_system_context(&self) -> String {
        let mut context = String::new();
        
        context.push_str("# Environment\n\n");
        context.push_str(&format!("- Working directory: {}\n", self.working_dir.display()));
        context.push_str(&format!("- Platform: {} {}\n", std::env::consts::OS, std::env::consts::ARCH));
        context.push_str(&format!("- Date: {}\n", chrono::Local::now().format("%Y-%m-%d")));
        
        if let Some(git) = &self.git_status {
            context.push_str(&format!("\n{}", git.format_for_system_prompt()));
        }
        
        context
    }
}
```

### 3.2 第二阶段：项目文档系统 (CLAUDE.md)

#### 3.2.1 项目文档发现

```rust
// crates/yode-core/src/project_docs.rs

use std::path::{Path, PathBuf};
use std::fs;

/// 项目文档发现器
pub struct ProjectDocFinder {
    /// 项目根目录
    project_root: PathBuf,
    /// 文档文件名模式
    doc_patterns: Vec<String>,
}

impl ProjectDocFinder {
    pub fn new(project_root: PathBuf) -> Self {
        Self {
            project_root,
            // 按优先级排序的文档模式
            doc_patterns: vec![
                "YODE.md".to_string(),      // Yode 特定文档
                "CLAUDE.md".to_string(),    // 通用 AI 助手文档
                "docs/YODE.md".to_string(),
                "docs/CLAUDE.md".to_string(),
                ".github/YODE.md".to_string(),
            ],
        }
    }
    
    /// 查找所有项目文档
    pub fn find_docs(&self) -> Vec<ProjectDoc> {
        let mut docs = Vec::new();
        
        for pattern in &self.doc_patterns {
            let path = self.project_root.join(pattern);
            if path.exists() {
                if let Ok(content) = fs::read_to_string(&path) {
                    docs.push(ProjectDoc {
                        path: path.clone(),
                        content,
                        priority: docs.len() as u32,
                    });
                }
            }
        }
        
        docs
    }
    
    /// 格式化所有文档为系统提示
    pub fn format_docs(&self) -> Option<String> {
        let docs = self.find_docs();
        if docs.is_empty() {
            return None;
        }
        
        let mut formatted = String::new();
        formatted.push_str("# Project Documentation\n\n");
        
        for doc in docs {
            formatted.push_str(&format!(
                "## From: {}\n\n{}\n\n",
                doc.path.display(),
                doc.content
            ));
        }
        
        Some(formatted)
    }
}

/// 项目文档
#[derive(Debug, Clone)]
pub struct ProjectDoc {
    pub path: PathBuf,
    pub content: String,
    pub priority: u32,
}
```

### 3.3 第三阶段：Memory 系统增强

Yode 已经有 memory 工具，但可以增强为自动注入：

```rust
// crates/yode-core/src/memory.rs

use std::path::{Path, PathBuf};
use std::fs;

/// Memory 文件管理器
pub struct MemoryManager {
    /// Memory 目录 (~/.yode/memory/{project_hash}/)
    memory_dir: PathBuf,
    /// 项目标识（用于区分项目）
    project_hash: String,
}

impl MemoryManager {
    pub fn new(project_root: &Path) -> Self {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        
        // 基于项目路径生成哈希
        let mut hasher = DefaultHasher::new();
        project_root.hash(&mut hasher);
        let project_hash = format!("{:x}", hasher.finish());
        
        let memory_dir = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".yode")
            .join("memory")
            .join(&project_hash);
        
        // 确保目录存在
        fs::create_dir_all(&memory_dir).ok();
        
        Self {
            memory_dir,
            project_hash,
        }
    }
    
    /// 获取所有 memory 文件内容
    pub fn read_memories(&self) -> Vec<MemoryEntry> {
        let mut entries = Vec::new();
        
        if let Ok(read_dir) = fs::read_dir(&self.memory_dir) {
            for entry in read_dir.flatten() {
                let path = entry.path();
                if path.extension().map(|e| e == "md").unwrap_or(false) {
                    if let Ok(content) = fs::read_to_string(&path) {
                        entries.push(MemoryEntry {
                            name: path.file_stem()
                                .and_then(|s| s.to_str())
                                .unwrap_or("unknown")
                                .to_string(),
                            path,
                            content,
                            modified: entry.metadata()
                                .and_then(|m| m.modified())
                                .ok(),
                        });
                    }
                }
            }
        }
        
        // 按修改时间排序
        entries.sort_by(|a, b| {
            b.modified.cmp(&a.modified)
        });
        
        entries
    }
    
    /// 格式化 memory 内容为系统提示
    pub fn format_memories(&self) -> Option<String> {
        let memories = self.read_memories();
        if memories.is_empty() {
            return None;
        }
        
        let mut formatted = String::new();
        formatted.push_str("# User Memory\n\n");
        formatted.push_str("The following notes are from the user's memory files.\n\n");
        
        for memory in memories {
            formatted.push_str(&format!(
                "## {}\n\n{}\n\n",
                memory.name,
                memory.content
            ));
        }
        
        Some(formatted)
    }
}

/// Memory 条目
#[derive(Debug, Clone)]
pub struct MemoryEntry {
    pub name: String,
    pub path: PathBuf,
    pub content: String,
    pub modified: Option<std::time::SystemTime>,
}
```

### 3.4 第四阶段：上下文压缩增强

#### 3.4.1 智能压缩策略

```rust
// crates/yode-core/src/context_manager.rs

/// 压缩策略配置
#[derive(Debug, Clone)]
pub struct CompressionConfig {
    /// 触发压缩的阈值（占 context window 的比例）
    pub compression_threshold: f64,
    /// 压缩后的目标使用量
    pub target_utilization: f64,
    /// 始终保留的最近消息数
    pub preserve_recent_count: usize,
    /// 工具结果最大字符数（压缩后）
    pub max_tool_result_chars: usize,
    /// 是否使用 LLM 进行智能总结
    pub use_llm_summarization: bool,
}

impl Default for CompressionConfig {
    fn default() -> Self {
        Self {
            compression_threshold: 0.75,
            target_utilization: 0.60,
            preserve_recent_count: 6,
            max_tool_result_chars: 500,
            use_llm_summarization: false,  // 可以启用 LLM 总结
        }
    }
}

impl ContextManager {
    /// 增强版压缩方法
    pub fn compress(&self, messages: &mut Vec<Message>) -> CompressionResult {
        let config = CompressionConfig::default();
        
        let original_len = messages.len();
        let original_tokens = self.estimate_tokens(messages);
        
        if messages.len() <= config.preserve_recent_count + 1 {
            return CompressionResult {
                messages_removed: 0,
                tokens_before: original_tokens,
                tokens_after: original_tokens,
                method: CompressionMethod::Noop,
            };
        }
        
        let mut method = CompressionMethod::None;
        
        // Phase 1: 截断工具结果
        for msg in messages.iter_mut() {
            if matches!(msg.role, Role::Tool) {
                if let Some(content) = &msg.content {
                    if content.len() > config.max_tool_result_chars {
                        let truncated: String = content
                            .chars()
                            .take(config.max_tool_result_chars)
                            .collect();
                        msg.content = Some(format!(
                            "{}... [truncated for context compression]",
                            truncated
                        ));
                        method = CompressionMethod::TruncateToolResults;
                    }
                }
            }
        }
        
        // Phase 2: 如果仍然超过目标，移除低优先级消息
        let target_tokens = (self.limits.context_window as f64 * config.target_utilization) as usize;
        
        while self.estimate_tokens(messages) > target_tokens 
            && messages.len() > config.preserve_recent_count + 1 
        {
            self.remove_lowest_priority_message(messages);
            method = match method {
                CompressionMethod::None => CompressionMethod::RemoveMessages,
                _ => CompressionMethod::Mixed,
            };
        }
        
        CompressionResult {
            messages_removed: original_len - messages.len(),
            tokens_before: original_tokens,
            tokens_after: self.estimate_tokens(messages),
            method,
        }
    }
    
    /// 移除最低优先级的消息
    fn remove_lowest_priority_message(&self, messages: &mut Vec<Message>) {
        let preserve_end = messages.len().saturating_sub(PRESERVE_RECENT);
        
        if preserve_end <= 1 {
            return;
        }
        
        // 找到最低优先级的消息
        let mut min_priority = u32::MAX;
        let mut min_idx = 1;
        
        for i in 1..preserve_end {
            let priority = message_priority(&messages[i]);
            if priority < min_priority {
                min_priority = priority;
                min_idx = i;
            }
        }
        
        messages.remove(min_idx);
    }
}

#[derive(Debug, Clone)]
pub struct CompressionResult {
    pub messages_removed: usize,
    pub tokens_before: usize,
    pub tokens_after: usize,
    pub method: CompressionMethod,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CompressionMethod {
    None,
    TruncateToolResults,
    RemoveMessages,
    LlmSummarization,
    Mixed,
    Noop,
}
```

---

## 4. 完整上下文组装

```rust
// crates/yode-core/src/context.rs

use crate::project_docs::ProjectDocFinder;
use crate::memory::MemoryManager;
use crate::context_manager::{ContextManager, GitStatus};

/// 完整的上下文组装器
pub struct ContextAssembler {
    context_manager: ContextManager,
    doc_finder: ProjectDocFinder,
    memory_manager: MemoryManager,
}

impl ContextAssembler {
    pub fn new(model: String, working_dir: PathBuf) -> Self {
        Self {
            context_manager: ContextManager::new(&model, working_dir.clone()),
            doc_finder: ProjectDocFinder::new(working_dir.clone()),
            memory_manager: MemoryManager::new(&working_dir),
        }
    }
    
    /// 组装完整的系统提示
    pub fn assemble_system_prompt(&self) -> String {
        let mut prompt = String::new();
        
        // 1. 基础系统提示（来自 prompts/system.md）
        prompt.push_str(include_str!("../../prompts/system.md"));
        
        // 2. 环境信息
        prompt.push_str("\n\n# Environment\n\n");
        prompt.push_str(&self.context_manager.get_system_context());
        
        // 3. 项目文档（如果有）
        if let Some(docs) = self.doc_finder.format_docs() {
            prompt.push_str("\n\n");
            prompt.push_str(&docs);
        }
        
        // 4. Memory 文件（如果有）
        if let Some(memories) = self.memory_manager.format_memories() {
            prompt.push_str("\n\n");
            prompt.push_str(&memories);
        }
        
        prompt
    }
}
```

---

## 5. 配置文件设计

```toml
# ~/.config/yode/config.toml

[context]
# Git 状态注入
inject_git_status = true

# 项目文档发现
project_doc_patterns = ["YODE.md", "CLAUDE.md", "docs/YODE.md"]

# Memory 系统
inject_memory = true
memory_dir = "~/.yode/memory"

# 上下文压缩
[context.compression]
enabled = true
threshold = 0.75        # 75% 使用量时触发
target = 0.60           # 压缩到 60% 使用量
preserve_recent = 6     # 保留最近 6 条消息
max_tool_result = 500   # 工具结果最大 500 字符
```

---

## 6. 总结

Claude Code 的上下文管理特点：

1. **Memoized 缓存** - 系统上下文和用户上下文都是缓存的
2. **Git 状态集成** - 自动注入 git 状态到系统提示
3. **CLAUDE.md 系统** - 项目文档自动发现
4. **Memory 文件** - 用户笔记持久化
5. **智能压缩** - 多级压缩策略

Yode 已有良好的压缩基础，建议增强：
1. Git 状态自动注入
2. 项目文档自动发现
3. Memory 系统集成
4. 可配置的压缩策略
