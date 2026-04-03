# Bash 工具深度分析与优化

## 1. Bash 工具架构概述

Claude Code 的 Bash 工具 (`src/tools/BashTool/BashTool.tsx`) 是一个 800+ 行的复杂工具，包含以下核心模块：

### 1.1 工具输入输出 Schema

```typescript
// 输入 Schema
const fullInputSchema = z.strictObject({
  command: z.string().describe('The command to execute'),
  timeout: semanticNumber(z.number().optional())
    .describe(`Optional timeout in milliseconds (max ${getMaxTimeoutMs()})`),
  description: z.string().optional()
    .describe('Clear, concise description of what this command does'),
  run_in_background: semanticBoolean(z.boolean().optional())
    .describe('Set to true to run this command in background'),
  dangerouslyDisableSandbox: semanticBoolean(z.boolean().optional())
    .describe('Override sandbox mode'),
  _simulatedSedEdit: z.object({
    filePath: z.string(),
    newContent: z.string()
  }).optional().describe('Internal: pre-computed sed edit result')
})

// 输出 Schema
const outputSchema = z.object({
  stdout: z.string().describe('Standard output'),
  stderr: z.string().describe('Standard error'),
  rawOutputPath: z.string().optional().describe('Path for large MCP outputs'),
  interrupted: z.boolean().describe('Whether command was interrupted'),
  isImage: z.boolean().optional().describe('Flag for image data in stdout'),
  backgroundTaskId: z.string().optional().describe('Background task ID'),
  backgroundedByUser: z.boolean().optional(),
  assistantAutoBackgrounded: z.boolean().optional(),
  returnCodeInterpretation: z.string().optional(),
  noOutputExpected: z.boolean().optional(),
  structuredContent: z.array(z.any()).optional(),
  persistedOutputPath: z.string().optional().describe('Path in tool-results dir'),
  persistedOutputSize: z.number().optional()
})
```

### 1.2 命令分类器

```typescript
// 搜索命令（可折叠显示）
const BASH_SEARCH_COMMANDS = new Set([
  'find', 'grep', 'rg', 'ag', 'ack', 'locate', 'which', 'whereis'
])

// 读取命令（可折叠显示）
const BASH_READ_COMMANDS = new Set([
  'cat', 'head', 'tail', 'less', 'more',
  // 分析命令
  'wc', 'stat', 'file', 'strings',
  // 数据处理
  'jq', 'awk', 'cut', 'sort', 'uniq', 'tr'
])

// 目录列表命令（单独分类）
const BASH_LIST_COMMANDS = new Set(['ls', 'tree', 'du'])

// 语义中性命令（不影响只读性质）
const BASH_SEMANTIC_NEUTRAL_COMMANDS = new Set([
  'echo', 'printf', 'true', 'false', ':'
])

// 静默命令（成功时无输出）
const BASH_SILENT_COMMANDS = new Set([
  'mv', 'cp', 'rm', 'mkdir', 'rmdir', 'chmod', 'chown', 'chgrp', 'touch', 'cd', 'ln'
])

/**
 * 判断命令是否为搜索/读取操作
 * 管道命令中所有部分都必须是搜索/读取命令
 */
export function isSearchOrReadBashCommand(command: string): {
  isSearch: boolean
  isRead: boolean
  isList: boolean
} {
  const partsWithOperators = splitCommandWithOperators(command)
  
  let hasSearch = false
  let hasRead = false
  let hasList = false
  let hasNonNeutralCommand = false
  
  for (const part of partsWithOperators) {
    // 跳过重定向目标
    if (part === '>' || part === '>>' || part === '>&') {
      continue
    }
    
    const baseCommand = part.trim().split(/\s+/)[0]
    
    // 跳过中性命令
    if (BASH_SEMANTIC_NEUTRAL_COMMANDS.has(baseCommand)) {
      continue
    }
    
    hasNonNeutralCommand = true
    
    if (BASH_SEARCH_COMMANDS.has(baseCommand)) hasSearch = true
    if (BASH_READ_COMMANDS.has(baseCommand)) hasRead = true
    if (BASH_LIST_COMMANDS.has(baseCommand)) hasList = true
    
    // 任何部分不是搜索/读取/列表命令，则整体不是
    if (!hasSearch && !hasRead && !hasList) {
      return { isSearch: false, isRead: false, isList: false }
    }
  }
  
  // 只有中性命令（如 "echo foo"）不可折叠
  if (!hasNonNeutralCommand) {
    return { isSearch: false, isRead: false, isList: false }
  }
  
  return { isSearch: hasSearch, isRead: hasRead, isList: hasList }
}
```

### 1.3 阻止的设备路径

```typescript
// 会导致进程挂起的设备文件
const BLOCKED_DEVICE_PATHS = new Set([
  // 无限输出 - 永远不会到达 EOF
  '/dev/zero',
  '/dev/random',
  '/dev/urandom',
  '/dev/full',
  // 阻塞等待输入
  '/dev/stdin',
  '/dev/tty',
  '/dev/console',
  // 无意义读取
  '/dev/stdout',
  '/dev/stderr',
  // fd 别名
  '/dev/fd/0',
  '/dev/fd/1',
  '/dev/fd/2',
])

function isBlockedDevicePath(filePath: string): boolean {
  if (BLOCKED_DEVICE_PATHS.has(filePath)) return true
  
  // Linux 别名：/proc/self/fd/0-2
  if (
    filePath.startsWith('/proc/') &&
    (filePath.endsWith('/fd/0') ||
      filePath.endsWith('/fd/1') ||
      filePath.endsWith('/fd/2'))
  ) {
    return true
  }
  
  return false
}
```

---

## 2. 进度追踪机制

### 2.1 进度显示常量

```typescript
// 2 秒后显示进度
const PROGRESS_THRESHOLD_MS = 2000

// Assistant 模式下，阻塞命令在 15 秒后自动后台运行
const ASSISTANT_BLOCKING_BUDGET_MS = 15_000
```

### 2.2 进度类型定义

```typescript
// src/types/tools.ts
export type BashProgress = {
  type: 'bash_progress'
  command: string
  output: string
  isRunning: boolean
}

// 进度消息渲染
function renderToolUseProgressMessage(
  toolCall: ToolCall,
  progress: BashProgress,
): ReactNode {
  return (
    <div>
      <strong>Running:</strong> {progress.command}
      <pre>{progress.output}</pre>
      {progress.isRunning && <span>Still running...</span>}
    </div>
  )
}
```

### 2.3 后台任务管理

```typescript
// 不应自动后台的命令
const DISALLOWED_AUTO_BACKGROUND_COMMANDS = ['sleep']

// 检查后台任务是否被禁用
const isBackgroundTasksDisabled = 
  isEnvTruthy(process.env.CLAUDE_CODE_DISABLE_BACKGROUND_TASKS)

// 常见后台命令类型
const COMMON_BACKGROUND_COMMANDS = [
  'npm', 'yarn', 'pnpm', 'node', 'python', 'python3',
  'go', 'cargo', 'make', 'docker', 'terraform',
  'webpack', 'vite', 'jest', 'pytest',
  'curl', 'wget', 'build', 'test', 'serve', 'watch', 'dev'
] as const

function getCommandTypeForLogging(command: string): string {
  const parts = splitCommand_DEPRECATED(command)
  if (parts.length === 0) return 'other'
  
  for (const part of parts) {
    const baseCommand = part.split(' ')[0] || ''
    if (COMMON_BACKGROUND_COMMANDS.includes(baseCommand as any)) {
      return baseCommand
    }
  }
  return 'other'
}
```

---

## 3. 权限与安全

### 3.1 权限检查集成

```typescript
// bashPermissions.ts
export function bashToolHasPermission(
  command: string,
  tool: PermissionContext,
): boolean {
  // 检查命令是否有权限执行
  // 支持通配符模式匹配
}

export function matchWildcardPattern(
  pattern: string,
  command: string,
): boolean {
  // 通配符匹配实现
}

export function permissionRuleExtractPrefix(
  rule: PermissionRule,
): string | null {
  // 从规则中提取前缀用于快速匹配
}
```

### 3.2 只读约束检查

```typescript
// readOnlyValidation.ts
export function checkReadOnlyConstraints(
  command: string,
  context: PermissionContext,
): {
  allowed: boolean
  reason?: string
} {
  // 检查命令是否违反只读约束
  // 支持路径级别的权限控制
}
```

### 3.3 命令语义解释

```typescript
// commandSemantics.ts
export function interpretCommandResult(
  command: string,
  exitCode: number,
  stderr: string,
): string | undefined {
  // 为非错误退出码提供语义解释
  // 例如：grep 未找到匹配返回 1，这是正常的
  
  if (command.startsWith('grep ') && exitCode === 1) {
    return 'No matches found (this is normal for grep)'
  }
  
  return undefined
}
```

---

## 4. 输出处理

### 4.1 大输出处理

```typescript
// 输出过大时写入文件
const persistedOutputPath = getToolResultPath(sessionId, toolCallId)
const persistedOutputSize = outputBytes.length

// 构建大结果消息
function buildLargeToolResultMessage(
  outputPath: string,
  size: number,
): string {
  return `Output written to ${outputPath} (${formatFileSize(size)})`
}
```

### 4.2 图片输出处理

```typescript
// 检测图片输出
function isImageOutput(output: string): boolean {
  // 检测 stdout 是否包含图片数据
}

// 调整图片大小
async function resizeShellImageOutput(
  buffer: Buffer,
  maxWidth: number,
): Promise<Buffer> {
  // 图片调整逻辑
}

// 构建图片结果
function buildImageToolResult(imageBuffer: Buffer): ToolResult {
  return {
    content: [{
      type: 'image',
      source: {
        type: 'base64',
        media_type: 'image/png',
        data: imageBuffer.toString('base64')
      }
    }]
  }
}
```

### 4.3 输出截断检测

```typescript
// 检测输出是否被截断
function isOutputLineTruncated(output: string): boolean {
  // 检查最后一行是否完整
}
```

---

## 5. 任务管理

### 5.1 后台任务注册

```typescript
// 注册前台任务
registerForeground(taskId, task)

// 生成后台任务
spawnShellTask(command, options)

// 取消前台任务
unregisterForeground(taskId)

// 标记任务已通知
markTaskNotified(taskId)
```

### 5.2 任务输出路径

```typescript
// 获取任务输出路径
function getTaskOutputPath(taskId: string): string {
  // 返回任务输出的完整路径
}
```

---

## 6. 工具 UI 渲染

### 6.1 工具使用消息

```typescript
function renderToolUseMessage(
  toolCall: ToolCall,
  agentId?: AgentId,
): ReactNode {
  const command = toolCall.input.command
  const description = toolCall.input.description
  
  return (
    <div>
      <strong>Bash</strong>
      {description && <p>{description}</p>}
      <code>{command}</code>
    </div>
  )
}
```

### 6.2 工具结果消息

```typescript
function renderToolResultMessage(
  toolCall: ToolCall,
  result: Out,
): ReactNode {
  const { stdout, stderr, backgroundTaskId } = result
  
  if (backgroundTaskId) {
    return <div>Task running in background (ID: {backgroundTaskId})</div>
  }
  
  if (result.noOutputExpected && !stdout && !stderr) {
    return <div>Done</div>
  }
  
  if (!stdout && !stderr) {
    return <div>(No output)</div>
  }
  
  return (
    <div>
      {stdout && <pre>{stdout}</pre>}
      {stderr && <pre className="error">{stderr}</pre>}
    </div>
  )
}
```

### 6.3 工具错误消息

```typescript
function renderToolUseErrorMessage(
  toolCall: ToolCall,
  error: Error,
): ReactNode {
  return (
    <div className="error">
      <strong>Error:</strong> {error.message}
    </div>
  )
}
```

---

## 7. Yode Bash 工具优化建议

### 7.1 第一阶段：命令分类器

```rust
// crates/yode-tools/src/builtin/bash/classifier.rs

use std::collections::HashSet;

/// Bash 命令分类器
pub struct BashCommandClassifier {
    search_commands: HashSet<&'static str>,
    read_commands: HashSet<&'static str>,
    list_commands: HashSet<&'static str>,
    silent_commands: HashSet<&'static str>,
    neutral_commands: HashSet<&'static str>,
}

impl BashCommandClassifier {
    pub fn new() -> Self {
        let mut classifier = Self {
            search_commands: HashSet::new(),
            read_commands: HashSet::new(),
            list_commands: HashSet::new(),
            silent_commands: HashSet::new(),
            neutral_commands: HashSet::new(),
        };
        
        // 搜索命令
        classifier.search_commands.extend([
            "find", "grep", "rg", "ag", "ack", "locate", "which", "whereis",
        ]);
        
        // 读取命令
        classifier.read_commands.extend([
            "cat", "head", "tail", "less", "more",
            "wc", "stat", "file", "strings",
            "jq", "awk", "cut", "sort", "uniq", "tr",
        ]);
        
        // 列表命令
        classifier.list_commands.extend(["ls", "tree", "du"]);
        
        // 静默命令
        classifier.silent_commands.extend([
            "mv", "cp", "rm", "mkdir", "rmdir", "chmod", "chown", "touch", "cd", "ln",
        ]);
        
        // 中性命令
        classifier.neutral_commands.extend([
            "echo", "printf", "true", "false", ":",
        ]);
        
        classifier
    }
    
    /// 判断命令是否为搜索/读取操作
    pub fn is_search_or_read(&self, command: &str) -> CommandClassification {
        let parts = self.split_command_with_operators(command);
        
        let mut has_search = false;
        let mut has_read = false;
        let mut has_list = false;
        let mut has_non_neutral = false;
        
        for part in parts {
            // 跳过重定向
            if [" >", ">>", ">&"].contains(&part) {
                continue;
            }
            
            let base_command = part.trim().split_whitespace().next().unwrap_or("");
            
            // 跳过中性命令
            if self.neutral_commands.contains(base_command) {
                continue;
            }
            
            has_non_neutral = true;
            
            if self.search_commands.contains(base_command) {
                has_search = true;
            }
            if self.read_commands.contains(base_command) {
                has_read = true;
            }
            if self.list_commands.contains(base_command) {
                has_list = true;
            }
            
            // 任何部分不是搜索/读取/列表，则整体不是
            if !has_search && !has_read && !has_list {
                return CommandClassification::NotCollapsible
            }
        }
        
        if !has_non_neutral {
            return CommandClassification::NotCollapsible
        }
        
        CommandClassification::Collapsible {
            is_search: has_search,
            is_read: has_read,
            is_list: has_list,
        }
    }
}

#[derive(Debug, Clone)]
pub enum CommandClassification {
    Collapsible {
        is_search: bool,
        is_read: bool,
        is_list: bool,
    },
    NotCollapsible,
}
```

### 7.2 第二阶段：阻止设备路径

```rust
// crates/yode-tools/src/builtin/bash/security.rs

use std::collections::HashSet;

/// 阻止的设备路径（防止进程挂起）
fn get_blocked_device_paths() -> &'static HashSet<&'static str> {
    lazy_static! {
        static ref BLOCKED: HashSet<&'static str> = [
            // 无限输出
            "/dev/zero",
            "/dev/random",
            "/dev/urandom",
            "/dev/full",
            // 阻塞输入
            "/dev/stdin",
            "/dev/tty",
            "/dev/console",
            // 无意义读取
            "/dev/stdout",
            "/dev/stderr",
            "/dev/fd/0",
            "/dev/fd/1",
            "/dev/fd/2",
        ].iter().copied().collect();
    }
    &BLOCKED
}

/// 检查是否为阻止的设备路径
pub fn is_blocked_device_path(path: &str) -> bool {
    if get_blocked_device_paths().contains(path) {
        return true;
    }
    
    // Linux /proc 别名检查
    if path.starts_with("/proc/") 
        && (path.ends_with("/fd/0") || path.ends_with("/fd/1") || path.ends_with("/fd/2")) 
    {
        return true;
    }
    
    false
}
```

### 7.3 第三阶段：进度追踪

```rust
// crates/yode-tools/src/builtin/bash/progress.rs

use tokio::sync::mpsc::Sender;

/// Bash 进度数据
#[derive(Debug, Clone)]
pub struct BashProgress {
    pub command: String,
    pub output: String,
    pub is_running: bool,
}

/// 进度追踪器
pub struct BashProgressTracker {
    progress_tx: Option<Sender<BashProgress>>,
    threshold_ms: u64,
}

impl BashProgressTracker {
    pub fn new(progress_tx: Option<Sender<BashProgress>>) -> Self {
        Self {
            progress_tx,
            threshold_ms: 2000, // 2 秒后显示进度
        }
    }
    
    /// 发送进度更新
    pub async fn send_update(&self, output: String, is_running: bool) {
        if let Some(tx) = &self.progress_tx {
            let _ = tx.send(BashProgress {
                command: String::new(), // 可在构造函数中设置
                output,
                is_running,
            }).await;
        }
    }
    
    /// 检查是否超过进度阈值
    pub fn exceeds_threshold(&self, elapsed_ms: u64) -> bool {
        elapsed_ms >= self.threshold_ms
    }
}
```

### 7.4 第四阶段：大输出处理

```rust
// crates/yode-tools/src/builtin/bash/output.rs

use std::path::PathBuf;

/// 输出大小阈值（超过此值写入文件）
const OUTPUT_SIZE_THRESHOLD_BYTES: usize = 100 * 1024; // 100KB

/// 大输出处理器
pub struct LargeOutputHandler {
    tool_result_dir: PathBuf,
}

impl LargeOutputHandler {
    pub fn new(tool_result_dir: PathBuf) -> Self {
        Self { tool_result_dir }
    }
    
    /// 处理大输出
    pub async fn handle_large_output(
        &self,
        output: &[u8],
        session_id: &str,
        tool_call_id: &str,
    ) -> Result<OutputLocation> {
        if output.len() <= OUTPUT_SIZE_THRESHOLD_BYTES {
            return Ok(OutputLocation::Inline(output.to_vec()));
        }
        
        // 写入文件
        let output_path = self.tool_result_dir
            .join(session_id)
            .join(format!("{}.out", tool_call_id));
        
        tokio::fs::write(&output_path, output).await?;
        
        Ok(OutputLocation::File {
            path: output_path,
            size: output.len(),
        })
    }
}

pub enum OutputLocation {
    Inline(Vec<u8>),
    File {
        path: PathBuf,
        size: usize,
    },
}
```

---

## 8. 配置文件示例

```toml
# ~/.config/yode/config.toml

[tools.bash]
# 进度追踪
enable_progress = true
progress_threshold_ms = 2000

# 后台任务
auto_background_threshold_ms = 15000
disable_background_tasks = false

# 输出处理
output_size_threshold_bytes = 102400  # 100KB

# 阻止的设备路径
blocked_device_paths = [
    "/dev/zero",
    "/dev/random",
    "/dev/urandom",
    "/dev/full",
    "/dev/stdin",
    "/dev/tty",
    "/dev/console",
]

# 命令分类
[tools.bash.classifier]
enabled = true

[tools.bash.classifier.search_commands]
commands = ["find", "grep", "rg", "ag", "ack", "locate", "which", "whereis"]

[tools.bash.classifier.read_commands]
commands = ["cat", "head", "tail", "less", "more", "wc", "stat", "file", "jq", "awk"]

[tools.bash.classifier.silent_commands]
commands = ["mv", "cp", "rm", "mkdir", "chmod", "chown", "touch", "cd", "ln"]
```

---

## 9. 总结

Claude Code Bash 工具的核心特点：

1. **命令分类器** - 智能识别搜索/读取/列表命令，支持 UI 折叠
2. **安全阻止** - 阻止危险设备路径读取
3. **进度追踪** - 实时输出进度，后台任务管理
4. **大输出处理** - 超过阈值时写入文件
5. **图片输出** - 自动检测和调整图片
6. **语义解释** - 为非错误退出码提供解释
7. **权限集成** - 与权限系统深度集成

Yode 优化优先级：
1. 命令分类器（UI 折叠）
2. 阻止设备路径（安全）
3. 进度追踪框架
4. 大输出文件化
