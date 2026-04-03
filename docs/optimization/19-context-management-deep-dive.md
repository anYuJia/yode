# 上下文管理系统深度分析与优化

## 1. 上下文管理架构概述

Claude Code 的上下文管理系统 (`src/context.ts`) 负责组装和缓存系统提示所需的各类上下文信息，包括 Git 状态、CLAUDE.md 文档、Memory 文件、环境信息等。

### 1.1 上下文类型定义

```typescript
// src/context.ts

/**
 * 系统上下文（缓存）
 */
export const getSystemContext = memoize(async (): Promise<{
  gitStatus?: string      // Git 状态
  cacheBreaker?: string   // 缓存破坏器（仅内部）
}> => {
  // ...
})

/**
 * 用户上下文（缓存）
 */
export const getUserContext = memoize(async (): Promise<{
  claudeMd?: string       // CLAUDE.md 内容
  currentDate?: string    // 当前日期
}> => {
  // ...
})

/**
 * Git 状态（缓存）
 */
export const getGitStatus = memoize(async (): Promise<string | null> => {
  // ...
})
```

### 1.2 系统提示组装流程

```typescript
// src/utils/systemPrompt.ts

async function buildSystemPrompt(
  options: SystemPromptOptions,
): Promise<string> {
  const parts: string[] = []
  
  // 1. 基础系统提示（内置模板）
  parts.push(BASE_SYSTEM_PROMPT)
  
  // 2. 系统上下文（Git 状态等）
  const systemContext = await getSystemContext()
  if (systemContext.gitStatus) {
    parts.push(`# Git Status\n\n${systemContext.gitStatus}`)
  }
  if (systemContext.cacheBreaker) {
    parts.push(systemContext.cacheBreaker)
  }
  
  // 3. 用户上下文（CLAUDE.md、Memory、日期）
  const userContext = await getUserContext()
  if (userContext.claudeMd) {
    parts.push(`# Project Documentation\n\n${userContext.claudeMd}`)
  }
  parts.push(userContext.currentDate)
  
  // 4. 工具定义
  parts.push(buildToolDefinitions(options.tools))
  
  // 5. 自定义系统提示
  if (options.customSystemPrompt) {
    parts.push(options.customSystemPrompt)
  }
  
  // 6. 附加系统提示
  if (options.appendSystemPrompt) {
    parts.push(options.appendSystemPrompt)
  }
  
  return parts.join('\n\n')
}
```

---

## 2. Git 状态获取

### 2.1 Git 状态检测流程

```typescript
// src/context.ts

const MAX_STATUS_CHARS = 2000  // 最大字符数限制

export const getGitStatus = memoize(async (): Promise<string | null> => {
  // 测试环境跳过
  if (process.env.NODE_ENV === 'test') {
    return null
  }
  
  const startTime = Date.now()
  logForDiagnosticsNoPII('info', 'git_status_started')
  
  // 1. 检查是否为 Git 仓库
  const isGitStart = Date.now()
  const isGit = await getIsGit()
  logForDiagnosticsNoPII('info', 'git_is_git_check_completed', {
    duration_ms: Date.now() - isGitStart,
    is_git: isGit,
  })
  
  if (!isGit) {
    logForDiagnosticsNoPII('info', 'git_status_skipped_not_git', {
      duration_ms: Date.now() - startTime,
    })
    return null
  }
  
  try {
    // 2. 并行执行所有 Git 命令
    const gitCmdsStart = Date.now()
    const [branch, mainBranch, status, log, userName] = await Promise.all([
      getBranch(),  // 当前分支
      getDefaultBranch(),  // 主分支
      execFileNoThrow(gitExe(), [
        '--no-optional-locks', 
        'status', 
        '--short'
      ]).then(({ stdout }) => stdout.trim()),
      execFileNoThrow(gitExe(), [
        '--no-optional-locks', 
        'log', 
        '--oneline', 
        '-n', 
        '5'
      ]).then(({ stdout }) => stdout.trim()),
      execFileNoThrow(gitExe(), ['config', 'user.name']).then(
        ({ stdout }) => stdout.trim()
      ),
    ])
    
    logForDiagnosticsNoPII('info', 'git_commands_completed', {
      duration_ms: Date.now() - gitCmdsStart,
      status_length: status.length,
    })
    
    // 3. 检查是否超过字符限制
    const truncatedStatus = status.length > MAX_STATUS_CHARS
      ? status.substring(0, MAX_STATUS_CHARS) +
        '\n... (truncated because it exceeds 2k characters. ' +
        'If you need more information, run "git status" using BashTool)'
      : status
    
    logForDiagnosticsNoPII('info', 'git_status_completed', {
      duration_ms: Date.now() - startTime,
      truncated: status.length > MAX_STATUS_CHARS,
    })
    
    // 4. 格式化输出
    return [
      `This is the git status at the start of the conversation. ` +
      `Note that this status is a snapshot in time, and will not ` +
      `update during the conversation.`,
      `Current branch: ${branch}`,
      `Main branch (you will usually use this for PRs): ${mainBranch}`,
      ...(userName ? [`Git user: ${userName}`] : []),
      `Status:\n${truncatedStatus || '(clean)'}`,
      `Recent commits:\n${log}`,
    ].join('\n\n')
    
  } catch (error) {
    logForDiagnosticsNoPII('error', 'git_status_failed', {
      duration_ms: Date.now() - startTime,
    })
    logError(error)
    return null
  }
})
```

### 2.2 Git 命令优化

```typescript
// src/utils/git.ts

// 缓存 Git 可执行文件路径
let gitExePath: string | null = null

/**
 * 获取 Git 可执行文件路径
 */
export function gitExe(): string {
  if (gitExePath) {
    return gitExePath
  }
  
  // 查找 Git 路径
  gitExePath = findInPath('git') || 'git'
  return gitExePath
}

/**
 * 获取当前分支
 */
export async function getBranch(): Promise<string> {
  const { stdout } = await execFileNoThrow(gitExe(), [
    'rev-parse',
    '--abbrev-ref',
    'HEAD',
  ])
  return stdout.trim()
}

/**
 * 获取主分支（尝试多个名称）
 */
export async function getDefaultBranch(): Promise<string> {
  // 尝试 symbolic-ref
  const { stdout: ref } = await execFileNoThrow(gitExe(), [
    'symbolic-ref',
    'refs/remotes/origin/HEAD',
  ])
  
  if (ref) {
    // origin/HEAD -> origin/main -> main
    return ref.replace('refs/remotes/origin/', '').trim()
  }
  
  // 回退到常见名称
  const commonNames = ['main', 'master', 'develop']
  for (const name of commonNames) {
    const exists = await checkBranchExists(name)
    if (exists) {
      return name
    }
  }
  
  return 'main'  // 默认
}

/**
 * 检查 Git 仓库
 */
export async function getIsGit(): Promise<boolean> {
  try {
    const { stdout } = await execFileNoThrow(gitExe(), [
      'rev-parse',
      '--git-dir',
    ])
    return stdout.trim() !== ''
  } catch {
    return false
  }
}
```

### 2.3 Git 状态缓存失效

```typescript
// src/context.ts

// 系统提示注入（仅内部，用于缓存破坏）
let systemPromptInjection: string | null = null

export function getSystemPromptInjection(): string | null {
  return systemPromptInjection
}

/**
 * 设置系统提示注入
 * 同时清除上下文缓存
 */
export function setSystemPromptInjection(value: string | null): void {
  systemPromptInjection = value
  
  // 立即清除上下文缓存
  getUserContext.cache.clear?.()
  getSystemContext.cache.clear?.()
}
```

---

## 3. CLAUDE.md 文档发现

### 3.1 CLAUDE.md 发现逻辑

```typescript
// src/utils/claudemd.ts

/**
 * 获取 CLAUDE.md 内容
 */
export async function getClaudeMds(
  memoryFiles?: string[],
): Promise<string | null> {
  const startTime = Date.now()
  logForDiagnosticsNoPII('info', 'claudemd_started')
  
  // 1. 检查是否禁用
  const shouldDisableClaudeMd =
    isEnvTruthy(process.env.CLAUDE_CODE_DISABLE_CLAUDE_MDS) ||
    (isBareMode() && getAdditionalDirectoriesForClaudeMd().length === 0)
  
  if (shouldDisableClaudeMd) {
    logForDiagnosticsNoPII('info', 'claudemd_disabled')
    return null
  }
  
  // 2. 查找 CLAUDE.md 文件
  const projectRoot = getProjectRoot()
  const additionalDirs = getAdditionalDirectoriesForClaudeMd()
  
  const searchPaths = [
    projectRoot,
    ...additionalDirs,
  ]
  
  const claudeMdFiles: string[] = []
  for (const dir of searchPaths) {
    const possibleNames = [
      path.join(dir, 'CLAUDE.md'),
      path.join(dir, 'CLAUDE.ai.md'),
      path.join(dir, '.claude', 'instructions.md'),
    ]
    
    for (const filePath of possibleNames) {
      if (await pathExists(filePath)) {
        claudeMdFiles.push(filePath)
      }
    }
  }
  
  if (claudeMdFiles.length === 0) {
    logForDiagnosticsNoPII('info', 'claudemd_not_found')
    return null
  }
  
  // 3. 读取所有 CLAUDE.md 文件
  const contents: string[] = []
  for (const file of claudeMdFiles) {
    try {
      const content = await readFile(file, 'utf-8')
      contents.push(`## From: ${file}\n\n${content}`)
    } catch (error) {
      logError('Failed to read CLAUDE.md:', error)
    }
  }
  
  // 4. 添加 Memory 文件（如果提供）
  if (memoryFiles && memoryFiles.length > 0) {
    contents.push(`## Memory Files\n\n${memoryFiles.join('\n\n')}`)
  }
  
  const result = contents.join('\n\n')
  
  logForDiagnosticsNoPII('info', 'claudemd_completed', {
    duration_ms: Date.now() - startTime,
    content_length: result.length,
  })
  
  return result
}
```

### 3.2 Memory 文件过滤

```typescript
// src/utils/claudemd.ts

/**
 * 获取 Memory 文件
 */
export async function getMemoryFiles(): Promise<string[] | null> {
  const memoryDir = getMemoryDir()
  
  try {
    const files = await readdir(memoryDir)
    const mdFiles = files.filter(f => f.endsWith('.md'))
    
    const contents: string[] = []
    for (const file of mdFiles) {
      const filePath = path.join(memoryDir, file)
      const content = await readFile(filePath, 'utf-8')
      contents.push(`### ${file}\n\n${content}`)
    }
    
    return contents
  } catch {
    return null
  }
}

/**
 * 过滤注入的 Memory 文件
 * 排除自动内存文件
 */
export function filterInjectedMemoryFiles(
  files: string[] | null,
): string[] | null {
  if (!files) return null
  
  return files.filter(content => {
    // 排除自动内存文件（由系统注入）
    return !isAutoMemFile(content)
  })
}

/**
 * 检测是否为自动内存文件
 */
export function isAutoMemFile(content: string): boolean {
  // 检查是否包含自动内存标记
  return content.includes('[AUTO_MEMORY]') ||
         content.includes('[SYSTEM_GENERATED]')
}
```

### 3.3 CLAUDE.md 缓存

```typescript
// src/context.ts

// 缓存的 CLAUDE.md 内容（用于分类器）
let cachedClaudeMdContent: string | null = null

/**
 * 设置缓存的 CLAUDE.md 内容
 * 供 auto-mode 分类器使用（避免循环依赖）
 */
export function setCachedClaudeMdContent(content: string | null): void {
  cachedClaudeMdContent = content
}

/**
 * 获取缓存的 CLAUDE.md 内容
 */
export function getCachedClaudeMdContent(): string | null {
  return cachedClaudeMdContent
}
```

---

## 4. 环境信息获取

### 4.1 系统环境上下文

```typescript
// src/utils/systemPrompt.ts

interface EnvironmentInfo {
  cwd: string           // 工作目录
  platform: string      // 操作系统
  arch: string          // CPU 架构
  nodeVersion: string   // Node.js 版本
  date: string          // 当前日期
}

async function getSystemContext(): Promise<string> {
  const context: Record<string, string> = {}
  
  // Git 状态
  const gitStatus = await getGitStatus()
  if (gitStatus) {
    context.gitStatus = gitStatus
  }
  
  // 系统信息
  context.environment = [
    `- Working directory: ${process.cwd()}`,
    `- Platform: ${process.platform} ${process.arch}`,
    `- Node.js: ${process.version}`,
    `- Date: ${new Date().toISOString()}`,
  ].join('\n')
  
  return formatContext(context)
}
```

### 4.2 项目根目录检测

```typescript
// src/utils/cwd.ts

let cachedProjectRoot: string | null = null

/**
 * 获取项目根目录
 */
export function getProjectRoot(): string {
  if (cachedProjectRoot) {
    return cachedProjectRoot
  }
  
  let currentDir = getCwd()
  
  // 向上查找包含 package.json、Cargo.toml 等的目录
  while (currentDir !== path.dirname(currentDir)) {
    const markers = [
      'package.json',
      'Cargo.toml',
      'go.mod',
      'requirements.txt',
      '.git',
    ]
    
    for (const marker of markers) {
      if (pathExistsSync(path.join(currentDir, marker))) {
        cachedProjectRoot = currentDir
        return currentDir
      }
    }
    
    currentDir = path.dirname(currentDir)
  }
  
  // 回退到当前目录
  cachedProjectRoot = getCwd()
  return cachedProjectRoot
}
```

---

## 5. 上下文缓存策略

### 5.1 Memoization 缓存

```typescript
// src/context.ts 使用 memoize

import memoize from 'lodash-es/memoize.js'

/**
 * Memoized Git 状态
 * 缓存整个会话期间
 */
export const getGitStatus = memoize(async (): Promise<string | null> => {
  // ...
})

/**
 * Memoized 系统上下文
 * 缓存整个会话期间
 */
export const getSystemContext = memoize(async (): Promise<{
  [k: string]: string
}> => {
  // ...
})

/**
 * Memoized 用户上下文
 * 缓存整个会话期间
 */
export const getUserContext = memoize(async (): Promise<{
  [k: string]: string
}> => {
  // ...
})
```

### 5.2 缓存失效触发器

```typescript
// 触发缓存失效的事件

// 1. 系统提示注入变更
setSystemPromptInjection(value: string | null) {
  systemPromptInjection = value
  getUserContext.cache.clear?.()
  getSystemContext.cache.clear?.()
}

// 2. 工作目录变更
onCwdChanged(newCwd: string) {
  getGitStatus.cache.clear?.()
  getSystemContext.cache.clear?.()
}

// 3. Git 状态变更（通过 Hook 检测）
onGitStatusChanged() {
  getGitStatus.cache.clear?.()
}

// 4. CLAUDE.md 文件变更
onClaudeMdChanged() {
  getUserContext.cache.clear?.()
}
```

---

## 6. 上下文 Token 优化

### 6.1 Token 估算

```typescript
// src/services/tokenEstimation.ts

/**
 * 估算上下文的 Token 数
 */
export function estimateContextTokens(context: Record<string, string>): number {
  let totalTokens = 0
  
  for (const [key, value] of Object.entries(context)) {
    // Git 状态：每行约 10 tokens
    if (key === 'gitStatus') {
      const lines = value.split('\n').length
      totalTokens += lines * 10
    }
    // CLAUDE.md：每 4 字符约 1 token
    else if (key === 'claudeMd') {
      totalTokens += Math.ceil(value.length / 4)
    }
    // 环境信息：固定 20 tokens
    else if (key === 'environment') {
      totalTokens += 20
    }
    // 默认：每 4 字符 1 token
    else {
      totalTokens += Math.ceil(value.length / 4)
    }
  }
  
  return totalTokens
}
```

### 6.2 上下文压缩

```typescript
// src/utils/contextCompression.ts

/**
 * 压缩上下文以适配 Token 限制
 */
export function compressContext(
  context: Record<string, string>,
  maxTokens: number,
): Record<string, string> {
  const currentTokens = estimateContextTokens(context)
  
  if (currentTokens <= maxTokens) {
    return context  // 无需压缩
  }
  
  const compressed = { ...context }
  
  // 1. 首先截断 Git 状态
  if (compressed.gitStatus) {
    const lines = compressed.gitStatus.split('\n')
    const maxLines = Math.floor(maxTokens * 0.3 / 10)
    compressed.gitStatus = lines.slice(0, maxLines).join('\n')
  }
  
  // 2. 然后截断 CLAUDE.md（保留关键部分）
  if (compressed.claudeMd) {
    // 提取 Build Commands 和 Code Style 部分
    const sections = extractKeySections(compressed.claudeMd)
    compressed.claudeMd = sections.join('\n\n')
  }
  
  return compressed
}

/**
 * 提取 CLAUDE.md 关键部分
 */
function extractKeySections(claudeMd: string): string[] {
  const sections: string[] = []
  
  // 优先保留的部分
  const prioritySections = [
    'Build Commands',
    'Test Commands',
    'Code Style',
    'Architecture',
  ]
  
  for (const section of prioritySections) {
    const content = extractSection(claudeMd, section)
    if (content) {
      sections.push(`## ${section}\n\n${content}`)
    }
  }
  
  return sections
}
```

---

## 7. Yode 上下文管理优化建议

### 7.1 第一阶段：上下文类型定义

```rust
// crates/yode-core/src/context/types.rs

/// 系统上下文
#[derive(Debug, Clone, Default)]
pub struct SystemContext {
    pub git_status: Option<String>,
    pub environment: EnvironmentInfo,
}

/// 用户上下文
#[derive(Debug, Clone, Default)]
pub struct UserContext {
    pub claude_md: Option<String>,
    pub memory_files: Vec<String>,
    pub current_date: String,
}

/// 环境信息
#[derive(Debug, Clone)]
pub struct EnvironmentInfo {
    pub cwd: String,
    pub platform: String,
    pub arch: String,
    pub rust_version: String,
    pub date: String,
}

/// 上下文缓存
pub struct ContextCache {
    git_status: MemoCache<String>,
    system_context: MemoCache<SystemContext>,
    user_context: MemoCache<UserContext>,
}

impl ContextCache {
    pub fn new() -> Self {
        Self {
            git_status: MemoCache::new(),
            system_context: MemoCache::new(),
            user_context: MemoCache::new(),
        }
    }
    
    pub fn invalidate_all(&mut self) {
        self.git_status.clear();
        self.system_context.clear();
        self.user_context.clear();
    }
    
    pub fn invalidate_git(&mut self) {
        self.git_status.clear();
        self.system_context.clear();
    }
}
```

### 7.2 第二阶段：Git 状态获取

```rust
// crates/yode-core/src/context/git.rs

use tokio::process::Command;

const MAX_STATUS_CHARS: usize = 2000;

/// Git 状态信息
#[derive(Debug, Clone)]
pub struct GitStatus {
    pub current_branch: String,
    pub main_branch: String,
    pub status: String,
    pub recent_commits: String,
    pub user_name: Option<String>,
}

/// 获取 Git 状态
pub async fn get_git_status() -> Option<String> {
    // 检查是否为 Git 仓库
    let is_git = check_is_git_repo().await?;
    if !is_git {
        return None;
    }
    
    // 并行执行所有 Git 命令
    let (branch, main_branch, status, log, user_name) = tokio::join!(
        get_branch(),
        get_default_branch(),
        get_status_short(),
        get_log_oneline(5),
        get_user_name(),
    );
    
    // 截断状态（如果过长）
    let truncated_status = if status.len() > MAX_STATUS_CHARS {
        format!(
            "{}\n... (truncated because it exceeds 2k characters)",
            &status[..MAX_STATUS_CHARS]
        )
    } else {
        status
    };
    
    // 格式化输出
    let mut parts = vec![
        "This is the git status at the start of the conversation.".to_string(),
        format!("Current branch: {}", branch),
        format!("Main branch: {}", main_branch),
        format!("Status:\n{}", truncated_status),
        format!("Recent commits:\n{}", log),
    ];
    
    if let Some(name) = user_name {
        parts.push(format!("Git user: {}", name));
    }
    
    Some(parts.join("\n\n"))
}

async fn check_is_git_repo() -> Option<bool> {
    let output = Command::new("git")
        .args(&["rev-parse", "--git-dir"])
        .output()
        .await
        .ok()?;
    
    Some(output.stdout.is_empty() == false)
}

async fn get_branch() -> String {
    let output = Command::new("git")
        .args(&["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .await
        .ok();
    
    output
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

async fn get_default_branch() -> String {
    // 尝试 symbolic-ref
    let output = Command::new("git")
        .args(&["symbolic-ref", "refs/remotes/origin/HEAD"])
        .output()
        .await
        .ok();
    
    if let Some(o) = output {
        if let Ok(s) = String::from_utf8(o.stdout) {
            let branch = s.trim();
            if !branch.is_empty() {
                return branch
                    .trim_start_matches("refs/remotes/origin/")
                    .to_string();
            }
        }
    }
    
    // 回退
    "main".to_string()
}

async fn get_status_short() -> String {
    let output = Command::new("git")
        .args(&["--no-optional-locks", "status", "--short"])
        .output()
        .await
        .ok();
    
    output
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "".to_string())
}

async fn get_log_oneline(limit: usize) -> String {
    let output = Command::new("git")
        .args(&["--no-optional-locks", "log", "--oneline", "-n", &limit.to_string()])
        .output()
        .await
        .ok();
    
    output
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "".to_string())
}

async fn get_user_name() -> Option<String> {
    let output = Command::new("git")
        .args(&["config", "user.name"])
        .output()
        .await
        .ok();
    
    output
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
}
```

### 7.3 第三阶段：CLAUDE.md 发现

```rust
// crates/yode-core/src/context/claude_md.rs

use std::path::{Path, PathBuf};

/// CLAUDE.md 文件候选名
const CLAUDE_MD_NAMES: &[&str] = &[
    "CLAUDE.md",
    "CLAUDE.ai.md",
    ".yode/instructions.md",
];

/// 查找 CLAUDE.md
pub async fn find_claude_mds(project_root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    
    for &name in CLAUDE_MD_NAMES {
        let path = project_root.join(name);
        if path.exists() {
            files.push(path);
        }
    }
    
    files
}

/// 读取 CLAUDE.md 内容
pub async fn read_claude_md(path: &Path) -> Option<String> {
    tokio::fs::read_to_string(path).await.ok()
}

/// 获取 CLAUDE.md 内容（格式化后）
pub async fn get_claude_md_content(project_root: &Path) -> Option<String> {
    let files = find_claude_mds(project_root).await;
    
    if files.is_empty() {
        return None;
    }
    
    let mut contents = Vec::new();
    
    for file in &files {
        if let Some(content) = read_claude_md(file).await {
            contents.push(format!("## From: {}\n\n{}", file.display(), content));
        }
    }
    
    Some(contents.join("\n\n"))
}

/// 提取关键部分
pub fn extract_key_sections(claude_md: &str) -> String {
    let priority_sections = [
        "Build Commands",
        "Test Commands", 
        "Code Style",
        "Architecture",
    ];
    
    let mut result = Vec::new();
    
    for &section in &priority_sections {
        if let Some(content) = extract_section(claude_md, section) {
            result.push(format!("## {}\n\n{}", section, content));
        }
    }
    
    if result.is_empty() {
        claude_md.to_string()
    } else {
        result.join("\n\n")
    }
}

fn extract_section(content: &str, section_name: &str) -> Option<String> {
    // 查找 ## Section Name
    let marker = format!("## {}", section_name);
    let start = content.find(&marker)?;
    
    // 查找下一个 ## 或结尾
    let rest = &content[start + marker.len()..];
    let end = rest.find("\n## ").unwrap_or(rest.len());
    
    Some(rest[..end].trim().to_string())
}
```

### 7.4 第四阶段：上下文组装器

```rust
// crates/yode-core/src/context/assembler.rs

/// 上下文组装器
pub struct ContextAssembler {
    project_root: PathBuf,
    git_enabled: bool,
    claude_md_enabled: bool,
    memory_enabled: bool,
}

impl ContextAssembler {
    pub fn new(project_root: PathBuf) -> Self {
        Self {
            project_root,
            git_enabled: true,
            claude_md_enabled: true,
            memory_enabled: true,
        }
    }
    
    /// 组装完整上下文
    pub async fn assemble(&self) -> SystemPromptContext {
        let mut context = SystemPromptContext::default();
        
        // Git 状态
        if self.git_enabled {
            if let Some(git_status) = get_git_status().await {
                context.insert("git_status", git_status);
            }
        }
        
        // 环境信息
        context.insert("environment", self.get_environment_info());
        
        // CLAUDE.md
        if self.claude_md_enabled {
            if let Some(claude_md) = get_claude_md_content(&self.project_root).await {
                context.insert("claude_md", claude_md);
            }
        }
        
        // Memory 文件
        if self.memory_enabled {
            if let Some(memory) = get_memory_files().await {
                context.insert("memory_files", memory);
            }
        }
        
        // 当前日期
        context.insert(
            "current_date", 
            format!("Today's date is {}.", get_local_iso_date())
        );
        
        context
    }
    
    fn get_environment_info(&self) -> String {
        format!(
            "- Working directory: {}\n\
             - Platform: {} {}\n\
             - Rust: {}\n\
             - Date: {}",
            self.project_root.display(),
            std::env::consts::OS,
            std::env::consts::ARCH,
            rustc_version::version().unwrap_or_else(|_| "unknown".into()),
            get_local_iso_date(),
        )
    }
}

/// 系统提示上下文
pub type SystemPromptContext = HashMap<String, String>;
```

---

## 8. 配置文件示例

```toml
# ~/.config/yode/context.toml

# Git 状态配置
[context.git]
enabled = true
max_status_chars = 2000
recent_commits_count = 5

# CLAUDE.md 配置
[context.claude_md]
enabled = true
file_names = ["CLAUDE.md", "CLAUDE.ai.md", ".yode/instructions.md"]
extract_key_sections = true

# Memory 配置
[context.memory]
enabled = true
include_auto_memory = false
max_files = 10

# 环境信息配置
[context.environment]
include_platform = true
include_arch = true
include_rust_version = true

# Token 优化配置
[context.optimization]
max_context_tokens = 5000
compress_git_status = true
extract_claude_md_sections = true
```

---

## 9. 总结

Claude Code 上下文管理的核心特点：

1. **Memoization 缓存** - 会话级别缓存，减少重复计算
2. **并行 Git 命令** - Promise.all 并行执行 5 个 Git 命令
3. **Git 状态截断** - 2000 字符限制，防止超长
4. **CLAUDE.md 发现** - 多位置、多名称查找
5. **Memory 文件过滤** - 排除自动注入的内存文件
6. **缓存失效机制** - 系统提示注入/CWD 变更触发失效
7. **Token 估算** - 基于类型和长度的 token 估算
8. **上下文压缩** - 提取关键部分，适配 token 限制
9. **环境信息注入** - 平台/架构/Rust 版本/日期
10. **项目根目录检测** - 基于 markers 向上查找

Yode 优化优先级：
1. 上下文类型定义
2. Git 状态获取（并行命令）
3. CLAUDE.md 发现与解析
4. 上下文组装器
5. Memoization 缓存机制
6. Token 优化与压缩
