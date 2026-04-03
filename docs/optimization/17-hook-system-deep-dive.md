# Hook 系统深度分析与优化

## 1. Hook 系统架构概述

Claude Code 的 Hook 系统 (`src/utils/hooks.ts`) 是一个 1200+ 行的复杂系统，支持 30+ 种事件类型，允许用户在 Claude Code 生命周期的各个阶段执行自定义命令。

### 1.1 Hook 事件类型

```typescript
// src/types/hooks.ts

type HookEvent =
  // ========== 会话生命周期 ==========
  | 'session_start'      // 会话开始
  | 'session_end'        // 会话结束
  | 'setup'              // 设置阶段
  | 'stop'               // 停止请求
  | 'stop_failure'       // 停止失败
  
  // ========== 工具执行 ==========
  | 'pre_tool_use'       // 工具使用前
  | 'post_tool_use'      // 工具使用后
  | 'post_tool_use_failure'  // 工具使用失败
  | 'permission_request'     // 权限请求
  | 'permission_denied'      // 权限被拒绝
  
  // ========== 用户交互 ==========
  | 'user_prompt_submit' // 用户提交提示
  | 'ask_user_question'  // 询问用户问题
  
  // ========== Agent 相关 ==========
  | 'subagent_start'     // 子 Agent 启动
  | 'subagent_stop'      // 子 Agent 停止
  | 'teammate_idle'      // 协作者空闲
  | 'task_created'       // 任务创建
  | 'task_completed'     // 任务完成
  
  // ========== 上下文管理 ==========
  | 'pre_compact'        // 上下文压缩前
  | 'post_compact'       // 上下文压缩后
  | 'cwd_changed'        // 工作目录变更
  | 'file_changed'       // 文件变更
  | 'config_change'      // 配置变更
  
  // ========== 通知类型 ==========
  | 'instructions_loaded' // 指令加载完成
  | 'elicitation'         // 诱导请求
  | 'elicitation_result'  // 诱导结果
  
  // ========== 特殊 Hook ==========
  | 'StatusLine'         // 状态行更新
  | 'FileSuggestion'     // 文件建议
```

### 1.2 Hook 类型定义

```typescript
// src/types/hooks.ts

// Hook 匹配器
export interface HookMatcher {
  // 匹配所有
  '*'?: boolean
  // 匹配工具名
  toolName?: string
  // 匹配事件类型
  hookEvent?: HookEvent
}

// Hook 命令配置
export interface HookCommand {
  // 命令类型
  type: 'shell' | 'http' | 'prompt' | 'agent'
  // 命令内容
  command: string
  // 超时时间
  timeout?: number
  // 是否异步
  async?: boolean
  // 异步唤醒条件
  asyncRewake?: boolean
}

// 完整的 Hook 配置
export interface HookConfiguration {
  matcher: HookMatcher
  command: HookCommand
  // 插件 ID（如果是插件 Hook）
  pluginId?: string
  // 技能目录（如果是技能 Hook）
  skillDir?: string
}
```

### 1.3 Hook 注册与管理

```typescript
// src/utils/hooks/sessionHooks.ts

// 会话级别 Hook 注册表
interface SessionHooks {
  // 函数 Hook（内存中）
  functionHooks: Map<string, FunctionHook[]>
  // HTTP Hook
  httpHooks: Map<string, HttpHook[]>
  // Agent Hook
  agentHooks: Map<string, AgentHook[]>
}

let sessionHooks: SessionHooks | null = null

// 注册会话 Hook
export function registerSessionHook(
  event: HookEvent,
  hook: FunctionHook | HttpHook | AgentHook,
): void {
  if (!sessionHooks) {
    sessionHooks = {
      functionHooks: new Map(),
      httpHooks: new Map(),
      agentHooks: new Map(),
    }
  }
  
  const hooks = sessionHooks.functionHooks.get(event) || []
  hooks.push(hook)
  sessionHooks.functionHooks.set(event, hooks)
}

// 获取会话 Hook
export function getSessionHookCallback(
  event: HookEvent,
): HookCallback | null {
  if (!sessionHooks) return null
  
  const hooks = sessionHooks.functionHooks.get(event)
  if (!hooks || hooks.length === 0) return null
  
  return async (input) => {
    for (const hook of hooks) {
      try {
        await hook(input)
      } catch (error) {
        logError('Session hook error:', error)
      }
    }
  }
}

// 清除会话 Hook
export function clearSessionHooks(): void {
  sessionHooks = null
}
```

---

## 2. Hook 执行流程

### 2.1 Hook 执行核心函数

```typescript
// src/utils/hooks.ts

const TOOL_HOOK_EXECUTION_TIMEOUT_MS = 10 * 60 * 1000  // 10 分钟

/**
 * 执行 Hook 的核心函数
 */
async function executeHook(
  hookEvent: HookEvent,
  hookName: string,
  input: unknown,
  context: HookExecutionContext,
): Promise<HookJSONOutput | null> {
  const hookId = randomUUID()
  const startTime = Date.now()
  
  // 发出 Hook 开始事件
  emitHookStarted({ hookId, hookName, hookEvent })
  
  try {
    // 获取 Hook 配置
    const hookConfig = getHookConfig(hookName)
    
    if (!hookConfig) {
      return null
    }
    
    // 根据类型执行不同类型的 Hook
    switch (hookConfig.command.type) {
      case 'shell':
        return await executeShellHook(
          hookEvent,
          hookName,
          hookId,
          hookConfig,
          input,
          context,
        )
      
      case 'http':
        return await executeHttpHook(
          hookEvent,
          hookName,
          hookId,
          hookConfig,
          input,
          context,
        )
      
      case 'prompt':
        return await execPromptHook(
          hookEvent,
          hookName,
          hookId,
          hookConfig,
          input,
          context,
        )
      
      case 'agent':
        return await execAgentHook(
          hookEvent,
          hookName,
          hookId,
          hookConfig,
          input,
          context,
        )
    }
  } catch (error) {
    logError('Hook execution failed:', error)
    return null
  } finally {
    // 记录 Hook 执行时间
    const duration = Date.now() - startTime
    addToTurnHookDuration(duration)
    
    // 发出 Hook 完成事件（如果启用 Beta 追踪）
    if (isBetaTracingEnabled()) {
      endHookSpan(hookId, { duration, success: true })
    }
  }
}
```

### 2.2 Shell Hook 执行

```typescript
/**
 * 执行 Shell Hook
 */
async function executeShellHook(
  hookEvent: HookEvent,
  hookName: string,
  hookId: string,
  config: HookConfiguration,
  input: unknown,
  context: HookExecutionContext,
): Promise<HookJSONOutput> {
  const { command, timeout, async, asyncRewake } = config.command
  
  // 准备环境变量
  const env = await buildHookEnv(input, hookEvent)
  
  // 生成 Shell 命令
  const shellCommand = formatShellPrefixCommand(command, env)
  
  // 创建 ShellCommand 实例
  const shellCmd = await spawnShellTask(
    shellCommand,
    {
      cwd: getCwd(),
      env,
      timeout: timeout || TOOL_HOOK_EXECUTION_TIMEOUT_MS,
    },
  )
  
  // 异步 Hook 处理
  if (async) {
    const backgroundSuccess = executeInBackground({
      processId: context.processId,
      hookId,
      shellCommand: shellCmd,
      asyncResponse: { type: 'async', hookId },
      hookEvent,
      hookName,
      command,
      asyncRewake,
    })
    
    if (backgroundSuccess) {
      return { type: 'async', hookId }
    }
  }
  
  // 同步等待结果
  const result = await shellCmd.result
  const stdout = await shellCmd.taskOutput.getStdout()
  const stderr = shellCmd.taskOutput.getStderr()
  
  // 清理
  shellCmd.cleanup()
  
  // 发出 Hook 响应事件
  emitHookResponse({
    hookId,
    hookName,
    hookEvent,
    output: stdout + stderr,
    stdout,
    stderr,
    exitCode: result.code,
    outcome: result.code === 0 ? 'success' : 'error',
  })
  
  // 检查是否为异步唤醒 Hook
  if (asyncRewake && result.code === 2) {
    // 退出码 2 表示阻塞错误，需要唤醒模型
    enqueuePendingNotification({
      value: wrapInSystemReminder(
        `Stop hook blocking error from command "${hookName}": ${stderr || stdout}`,
      ),
      mode: 'task-notification',
    })
  }
  
  return {
    type: 'sync',
    stdout,
    stderr,
    exitCode: result.code,
  }
}
```

### 2.3 异步 Hook 唤醒机制

```typescript
/**
 * 在后台执行 Hook
 * 用于长时间运行的任务
 */
function executeInBackground({
  processId,
  hookId,
  shellCommand,
  asyncResponse,
  hookEvent,
  hookName,
  command,
  asyncRewake,
  pluginId,
}: {
  processId: string
  hookId: string
  shellCommand: ShellCommand
  asyncResponse: AsyncHookJSONOutput
  hookEvent: HookEvent | 'StatusLine' | 'FileSuggestion'
  hookName: string
  command: string
  asyncRewake?: boolean
  pluginId?: string
}): boolean {
  if (asyncRewake) {
    // asyncRewake Hook 完全绕过注册表
    // 退出码 2 时，作为任务通知入队，唤醒模型
    void shellCommand.result.then(async result => {
      // 等待 stdout/stderr 数据事件完成
      await new Promise(resolve => setImmediate(resolve))
      
      const stdout = await shellCommand.taskOutput.getStdout()
      const stderr = shellCommand.taskOutput.getStderr()
      
      shellCommand.cleanup()
      
      emitHookResponse({
        hookId,
        hookName,
        hookEvent,
        output: stdout + stderr,
        stdout,
        stderr,
        exitCode: result.code,
        outcome: result.code === 0 ? 'success' : 'error',
      })
      
      // 退出码 2 表示阻塞错误，唤醒模型
      if (result.code === 2) {
        enqueuePendingNotification({
          value: wrapInSystemReminder(
            `Stop hook blocking error from command "${hookName}": ${stderr || stdout}`,
          ),
          mode: 'task-notification',
        })
      }
    })
    return true
  }
  
  // 标准异步 Hook：注册到挂起 Hook 注册表
  if (!shellCommand.background(processId)) {
    return false
  }
  
  registerPendingAsyncHook(hookId, {
    hookName,
    hookEvent,
    shellCommand,
    pluginId,
  })
  
  return true
}
```

---

## 3. Hook 超时管理

### 3.1 超时配置

```typescript
// 工具 Hook 执行超时
const TOOL_HOOK_EXECUTION_TIMEOUT_MS = 10 * 60 * 1000  // 10 分钟

// SessionEnd Hook 超时（更严格）
const SESSION_END_HOOK_TIMEOUT_MS_DEFAULT = 1500  // 1.5 秒

/**
 * 获取 SessionEnd Hook 超时时间
 * 可通过环境变量覆盖
 */
export function getSessionEndHookTimeoutMs(): number {
  const raw = process.env.CLAUDE_CODE_SESSIONEND_HOOKS_TIMEOUT_MS
  const parsed = raw ? parseInt(raw, 10) : NaN
  return Number.isFinite(parsed) && parsed > 0
    ? parsed
    : SESSION_END_HOOK_TIMEOUT_MS_DEFAULT
}
```

### 3.2 超时处理

```typescript
/**
 * 带超时的 Hook 执行
 */
async function executeHookWithTimeout(
  hookEvent: HookEvent,
  hookName: string,
  input: unknown,
  context: HookExecutionContext,
): Promise<HookJSONOutput | null> {
  const timeout = hookEvent === 'session_end'
    ? getSessionEndHookTimeoutMs()
    : TOOL_HOOK_EXECUTION_TIMEOUT_MS
  
  const abortController = new AbortController()
  const timeoutId = setTimeout(() => {
    abortController.abort('Hook timeout')
  }, timeout)
  
  try {
    const result = await Promise.race([
      executeHook(hookEvent, hookName, input, context),
      new Promise<null>((_, reject) => {
        abortController.signal.addEventListener('abort', () => {
          reject(new AbortError('Hook timeout'))
        })
      }),
    ])
    
    clearTimeout(timeoutId)
    return result
  } catch (error) {
    if (error instanceof AbortError) {
      logError('Hook timed out:', { hookName, hookEvent, timeout })
      return {
        type: 'sync',
        stdout: '',
        stderr: `Hook '${hookName}' timed out after ${timeout}ms`,
        exitCode: 1,
      }
    }
    throw error
  }
}
```

---

## 4. Hook 匹配器

### 4.1 匹配器逻辑

```typescript
// src/utils/hooks/hooksSettings.ts

/**
 * 检查 Hook 是否匹配给定事件
 */
function doesHookMatch(
  matcher: HookMatcher,
  hookEvent: HookEvent,
  toolName?: string,
): boolean {
  // 匹配所有
  if (matcher['*']) {
    return true
  }
  
  // 精确匹配事件类型
  if (matcher.hookEvent && matcher.hookEvent !== hookEvent) {
    return false
  }
  
  // 精确匹配工具名
  if (matcher.toolName && matcher.toolName !== toolName) {
    return false
  }
  
  return true
}

/**
 * 获取所有匹配的 Hook
 */
export function getMatchingHooks(
  hookEvent: HookEvent,
  toolName?: string,
): HookConfiguration[] {
  const allHooks = getAllRegisteredHooks()
  
  return allHooks.filter(hook =>
    doesHookMatch(hook.matcher, hookEvent, toolName),
  )
}
```

### 4.2 插件 Hook 匹配

```typescript
// Plugin Hook 匹配器
interface PluginHookMatcher {
  pluginId: string
  event: HookEvent
  matcher: HookMatcher
}

/**
 * 检查插件 Hook 是否匹配
 */
function doesPluginHookMatch(
  pluginMatcher: PluginHookMatcher,
  hookEvent: HookEvent,
  toolName?: string,
): boolean {
  // 首先检查插件 ID 是否匹配
  if (pluginMatcher.event !== hookEvent) {
    return false
  }
  
  // 然后检查匹配器
  return doesHookMatch(pluginMatcher.matcher, hookEvent, toolName)
}
```

---

## 5. Hook 环境构建

### 5.1 环境变量构建

```typescript
// src/utils/sessionEnvironment.js

/**
 * 构建 Hook 环境变量
 */
async function buildHookEnv(
  input: unknown,
  hookEvent: HookEvent,
): Promise<Record<string, string>> {
  const env = {
    ...process.env,
    ...subprocessEnv,
    
    // Hook 特定变量
    CLAUDE_HOOK_EVENT: hookEvent,
    CLAUDE_SESSION_ID: getSessionId(),
    CLAUDE_PROJECT_ROOT: getProjectRoot(),
    CLAUDE_CWD: getCwd(),
  }
  
  // 根据事件类型添加特定变量
  switch (hookEvent) {
    case 'pre_tool_use':
    case 'post_tool_use':
      const toolInput = input as PreToolUseHookInput
      env.CLAUDE_TOOL_NAME = toolInput.toolName
      env.CLAUDE_TOOL_INPUT = JSON.stringify(toolInput.input)
      break
    
    case 'permission_request':
      const permInput = input as PermissionRequestHookInput
      env.CLAUDE_TOOL_NAME = permInput.toolName
      env.CLAUDE_PERMISSION_DECISION = permInput.decision
      break
    
    case 'user_prompt_submit':
      const promptInput = input as UserPromptSubmitHookInput
      env.CLAUDE_PROMPT = promptInput.prompt
      break
  }
  
  // 加载 .env 文件（如果存在）
  const hookEnvFilePath = await getHookEnvFilePath()
  if (hookEnvFilePath) {
    const envFileContent = await readFile(hookEnvFilePath, 'utf-8')
    const parsedEnv = dotenv.parse(envFileContent)
    Object.assign(env, parsedEnv)
  }
  
  return env
}
```

### 5.2 会话环境缓存

```typescript
/**
 * 使会话环境缓存失效
 * 在配置变更时调用
 */
export function invalidateSessionEnvCache(): void {
  // 清除缓存的 Hook 配置
  cachedHookConfig = null
  
  // 清除缓存的环境变量
  cachedHookEnv = null
  
  // 重新加载配置
  if (hooksConfigFileExists()) {
    reloadHooksConfig()
  }
}
```

---

## 6. Hook 遥测与日志

### 6.1 Hook 事件发射

```typescript
// src/utils/hooks/hookEvents.ts

/**
 * 发出 Hook 开始事件
 */
export function emitHookStarted(data: {
  hookId: string
  hookName: string
  hookEvent: HookEvent
}): void {
  logOTelEvent('hook_started', {
    hook_id: data.hookId,
    hook_name: data.hookName,
    hook_event: data.hookEvent,
    timestamp: Date.now(),
  })
  
  // 如果使用 Beta 追踪，启动 Span
  if (isBetaTracingEnabled()) {
    startHookSpan(data.hookId, {
      hookName: data.hookName,
      hookEvent: data.hookEvent,
    })
  }
}

/**
 * 发出 Hook 响应事件
 */
export function emitHookResponse(data: {
  hookId: string
  hookName: string
  hookEvent: HookEvent
  output: string
  stdout: string
  stderr: string
  exitCode: number
  outcome: 'success' | 'error'
}): void {
  logOTelEvent('hook_completed', {
    hook_id: data.hookId,
    hook_name: data.hookName,
    hook_event: data.hookEvent,
    exit_code: data.exitCode,
    outcome: data.outcome,
    output_length: data.output.length,
    timestamp: Date.now(),
  })
  
  // 结束 Span
  if (isBetaTracingEnabled()) {
    endHookSpan(data.hookId, {
      exitCode: data.exitCode,
      outcome: data.outcome,
    })
  }
}
```

### 6.2 Hook 进度追踪

```typescript
/**
 * 启动 Hook 进度间隔
 * 用于长时间运行的 Hook
 */
export function startHookProgressInterval(
  hookId: string,
  hookName: string,
): NodeJS.Timeout {
  return setInterval(() => {
    emitHookProgress({
      hookId,
      hookName,
      message: 'Hook still running...',
    })
  }, 5000)  // 每 5 秒报告一次
}

function emitHookProgress(data: {
  hookId: string
  hookName: string
  message: string
}): void {
  logOTelEvent('hook_progress', {
    hook_id: data.hookId,
    hook_name: data.hookName,
    message: data.message,
    timestamp: Date.now(),
  })
}
```

---

## 7. Yode Hook 系统优化建议

### 7.1 第一阶段：Hook 事件类型定义

```rust
// crates/yode-core/src/hooks/events.rs

/// Hook 事件类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HookEvent {
    // 会话生命周期
    SessionStart,
    SessionEnd,
    Setup,
    Stop,
    StopFailure,
    
    // 工具执行
    PreToolUse,
    PostToolUse,
    PostToolUseFailure,
    PermissionRequest,
    PermissionDenied,
    
    // 用户交互
    UserPromptSubmit,
    AskUserQuestion,
    
    // Agent 相关
    SubagentStart,
    SubagentStop,
    TaskCreated,
    TaskCompleted,
    
    // 上下文管理
    PreCompact,
    PostCompact,
    CwdChanged,
    FileChanged,
    ConfigChange,
    
    // 通知
    InstructionsLoaded,
}

impl HookEvent {
    /// 转换为字符串
    pub fn as_str(&self) -> &'static str {
        match self {
            HookEvent::SessionStart => "session_start",
            HookEvent::SessionEnd => "session_end",
            HookEvent::PreToolUse => "pre_tool_use",
            HookEvent::PostToolUse => "post_tool_use",
            // ...
        }
    }
}
```

### 7.2 第二阶段：Hook 注册表

```rust
// crates/yode-core/src/hooks/registry.rs

use std::collections::HashMap;
use tokio::sync::RwLock;

/// Hook 配置
#[derive(Debug, Clone)]
pub struct HookConfig {
    pub matcher: HookMatcher,
    pub command: HookCommand,
    pub plugin_id: Option<String>,
}

/// Hook 匹配器
#[derive(Debug, Clone)]
pub struct HookMatcher {
    pub match_all: bool,
    pub hook_event: Option<HookEvent>,
    pub tool_name: Option<String>,
}

/// Hook 命令
#[derive(Debug, Clone)]
pub enum HookCommand {
    Shell {
        command: String,
        timeout: Option<u64>,
        r#async: bool,
    },
    Http {
        url: String,
        method: String,
        timeout: Option<u64>,
    },
}

/// Hook 注册表
pub struct HookRegistry {
    hooks: RwLock<Vec<HookConfig>>,
}

impl HookRegistry {
    pub fn new() -> Self {
        Self {
            hooks: RwLock::new(Vec::new()),
        }
    }
    
    /// 注册 Hook
    pub async fn register(&self, config: HookConfig) {
        let mut hooks = self.hooks.write().await;
        hooks.push(config);
    }
    
    /// 获取匹配的 Hook
    pub async fn get_matching_hooks(
        &self,
        event: HookEvent,
        tool_name: Option<&str>,
    ) -> Vec<HookConfig> {
        let hooks = self.hooks.read().await;
        hooks
            .iter()
            .filter(|h| self.does_hook_match(&h.matcher, event, tool_name))
            .cloned()
            .collect()
    }
    
    fn does_hook_match(
        &self,
        matcher: &HookMatcher,
        event: HookEvent,
        tool_name: Option<&str>,
    ) -> bool {
        if matcher.match_all {
            return true;
        }
        
        if let Some(hook_event) = &matcher.hook_event {
            if *hook_event != event {
                return false;
            }
        }
        
        if let Some(name) = &matcher.tool_name {
            if let Some(tn) = tool_name {
                if name != tn {
                    return false;
                }
            } else {
                return false;
            }
        }
        
        true
    }
}
```

### 7.3 第三阶段：Hook 执行器

```rust
// crates/yode-core/src/hooks/executor.rs

use tokio::process::Command;
use std::time::Duration;

/// Hook 执行器
pub struct HookExecutor {
    registry: Arc<HookRegistry>,
}

impl HookExecutor {
    pub fn new(registry: Arc<HookRegistry>) -> Self {
        Self { registry }
    }
    
    /// 执行 Hook
    pub async fn execute_hook(
        &self,
        event: HookEvent,
        tool_name: Option<&str>,
        input: &serde_json::Value,
    ) -> Vec<HookResult> {
        let hooks = self.registry
            .get_matching_hooks(event, tool_name)
            .await;
        
        let mut results = Vec::new();
        
        for hook in hooks {
            let result = match &hook.command {
                HookCommand::Shell { command, timeout, r#async } => {
                    if *r#async {
                        self.execute_shell_hook_async(command, *timeout).await
                    } else {
                        self.execute_shell_hook_sync(command, *timeout).await
                    }
                }
                HookCommand::Http { url, method, timeout } => {
                    self.execute_http_hook(url, method, *timeout).await
                }
            };
            
            results.push(result);
        }
        
        results
    }
    
    /// 同步执行 Shell Hook
    async fn execute_shell_hook_sync(
        &self,
        command: &str,
        timeout: Option<u64>,
    ) -> HookResult {
        let timeout = timeout.unwrap_or(600_000); // 10 分钟默认
        
        let output = tokio::time::timeout(
            Duration::from_millis(timeout),
            Command::new("sh")
                .arg("-c")
                .arg(command)
                .output(),
        )
        .await;
        
        match output {
            Ok(Ok(output)) => HookResult::Success {
                stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                stderr: String::from_utf8_lossy(&output.stderr).to_string(),
                exit_code: output.status.code().unwrap_or(-1),
            },
            Ok(Err(e)) => HookResult::Error(e.to_string()),
            Err(_) => HookResult::Timeout,
        }
    }
}

/// Hook 执行结果
#[derive(Debug)]
pub enum HookResult {
    Success {
        stdout: String,
        stderr: String,
        exit_code: i32,
    },
    Error(String),
    Timeout,
}
```

### 7.4 第四阶段：异步 Hook 唤醒

```rust
// crates/yode-core/src/hooks/async.rs

use tokio::sync::mpsc;

/// 异步 Hook 注册表
pub struct AsyncHookRegistry {
    pending_hooks: RwLock<HashMap<String, PendingHook>>,
    wake_tx: mpsc::Sender<WakeNotification>,
}

struct PendingHook {
    hook_name: String,
    event: HookEvent,
    command: String,
}

pub struct WakeNotification {
    pub hook_name: String,
    pub message: String,
}

impl AsyncHookRegistry {
    pub fn new() -> Self {
        let (wake_tx, mut wake_rx) = mpsc::channel::<WakeNotification>(100);
        
        // 启动唤醒处理器
        tokio::spawn(async move {
            while let Some(notification) = wake_rx.recv().await {
                // 处理唤醒通知
                Self::handle_wake_notification(notification).await;
            }
        });
        
        Self {
            pending_hooks: RwLock::new(HashMap::new()),
            wake_tx,
        }
    }
    
    /// 注册异步 Hook
    pub async fn register_pending(&self, hook_id: String, hook: PendingHook) {
        let mut hooks = self.pending_hooks.write().await;
        hooks.insert(hook_id, hook);
    }
    
    /// 完成异步 Hook
    pub async fn complete_hook(
        &self,
        hook_id: &str,
        result: HookResult,
    ) -> Option<HookResult> {
        let mut hooks = self.pending_hooks.write().await;
        
        if let Some(hook) = hooks.remove(hook_id) {
            // 检查是否需要唤醒
            if let HookResult::Success { exit_code: 2, .. } = &result {
                // 退出码 2 表示需要唤醒
                let _ = self.wake_tx.send(WakeNotification {
                    hook_name: hook.hook_name.clone(),
                    message: format!("Hook '{}' blocking error", hook.hook_name),
                }).await;
            }
            
            return Some(result);
        }
        
        None
    }
    
    async fn handle_wake_notification(notification: WakeNotification) {
        // 发送系统提醒
        log::warn!("Wake notification: {}", notification.message);
    }
}
```

---

## 8. 配置文件示例

```yaml
# ~/.config/yode/hooks.yaml

hooks:
  # Session Start Hook
  - matcher:
      hook_event: session_start
    command:
      type: shell
      command: echo "Session started at $(date)"
      timeout: 5000

  # Pre Tool Use Hook - 记录所有 Bash 命令
  - matcher:
      hook_event: pre_tool_use
      tool_name: Bash
    command:
      type: shell
      command: |
        echo "Pre-executing: $CLAUDE_TOOL_INPUT" >> ~/.yode/tool_log.txt
      timeout: 5000

  # Post Tool Use Hook - 发送遥测
  - matcher:
      hook_event: post_tool_use
    command:
      type: http
      url: http://localhost:8080/hooks/tool-completed
      method: POST
      timeout: 10000

  # Permission Request Hook - 自动批准只读命令
  - matcher:
      hook_event: permission_request
    command:
      type: shell
      command: |
        if echo "$CLAUDE_TOOL_INPUT" | grep -qE "^(ls|cat|head|tail|grep)"; then
          exit 0  # 批准
        fi
        exit 1    # 需要用户确认
      timeout: 5000

  # Async Hook - 长时间运行任务
  - matcher:
      hook_event: subagent_start
    command:
      type: shell
      command: |
        # 启动后台监控
        while true; do
          echo "Agent running..." >> /tmp/agent_status.txt
          sleep 60
        done
      async: true
      async_rewake: true

  # File Changed Hook - Git 自动提交
  - matcher:
      hook_event: file_changed
    command:
      type: shell
      command: |
        git add -A
        git commit -m "Auto-commit: $(date)"
      timeout: 30000
```

---

## 9. 总结

Claude Code Hook 系统的核心特点：

1. **30+ 事件类型** - 覆盖会话、工具、用户、Agent 全生命周期
2. **4 种 Hook 类型** - Shell、HTTP、Prompt、Agent
3. **异步支持** - 后台执行 + 唤醒机制
4. **超时管理** - 每 Hook 类型独立超时配置
5. **匹配器系统** - 事件/工具名/通配符匹配
6. **插件集成** - 插件可注册自定义 Hook
7. **会话 Hook** - 内存中函数 Hook（ephemeral）
8. **环境变量** - 丰富的 Hook 上下文变量
9. **遥测追踪** - Hook 开始/完成/进度事件
10. **退出码语义** - 退出码 2 表示唤醒需求

Yode 优化优先级：
1. Hook 事件类型定义
2. Hook 注册表与匹配器
3. Shell/HTTP Hook 执行器
4. 异步 Hook 唤醒机制
5. 超时管理系统
6. 配置文件格式
