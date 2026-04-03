# Hook 系统深度分析与优化建议

## 1. Claude Code Hook 系统架构

### 1.1 Hook 事件类型

```typescript
// src/entrypoints/agentSdkTypes.ts

export type HookEvent =
  // 会话生命周期
  | 'session_start'
  | 'session_end'
  | 'setup'
  | 'stop'
  | 'stop_failure'
  
  // 用户交互
  | 'user_prompt_submit'
  | 'permission_request'
  | 'permission_denied'
  | 'elicitation'
  | 'elicitation_result'
  
  // 工具执行
  | 'pre_tool_use'
  | 'post_tool_use'
  | 'post_tool_use_failure'
  
  // 上下文管理
  | 'pre_compact'
  | 'post_compact'
  
  // 子代理
  | 'subagent_start'
  | 'subagent_stop'
  
  // 任务
  | 'task_created'
  | 'task_completed'
  
  // 状态变化
  | 'config_change'
  | 'cwd_changed'
  | 'file_changed'
  | 'instructions_loaded'
  
  // Teammate
  | 'teammate_idle'
```

### 1.2 Hook 配置来源

```typescript
// src/utils/hooks.ts

type PermissionRuleSource =
  | 'userSettings'      // ~/.config/claude/settings.json
  | 'projectSettings'   // .claude/settings.json
  | 'localSettings'     // .claude/local.json
  | 'flagSettings'      // 特性标志配置
  | 'policySettings'    // 企业策略配置
  | 'cliArg'            // 命令行参数
  | 'command'           // 命令来源
  | 'session'           // 会话级规则 (临时)

// Hook 命令类型
type HookCommand = {
  type: 'command' | 'prompt' | 'function' | 'http' | 'agent'
  event: HookEvent
  matcher: string          // 匹配模式
  command?: string         // shell 命令
  prompt?: string          // 提示覆盖
  timeout?: number         // 超时 (ms)
  async?: boolean          // 异步执行
  asyncRewake?: boolean    // 异步唤醒模型
  pluginId?: string        // 插件 ID
}
```

### 1.3 Hook 执行流程

```typescript
// src/utils/hooks.ts - 简化执行流程

async function executeHooks(
  event: HookEvent,
  input: HookInput,
  context: ToolUseContext,
): Promise<AggregatedHookResult> {
  const startTime = Date.now()
  
  // 1. 获取匹配的 Hook
  const hooks = getMatchingHooks(event, input)
  
  if (hooks.length === 0) {
    return { shouldBlock: false, results: [] }
  }
  
  // 2. 发射 Hook 开始事件
  emitHookStarted({ event, hookCount: hooks.length })
  
  // 3. 并行执行所有 Hook
  const results = await Promise.all(
    hooks.map(async hook => {
      const hookStartTime = Date.now()
      
      // 发射 Hook 执行事件
      emitHookExecutionStarted({ hookId: hook.id })
      
      let result: HookResult
      
      // 根据类型执行
      switch (hook.type) {
        case 'command':
          result = await executeCommandHook(hook, input, context)
          break
        case 'prompt':
          result = await execPromptHook(hook, input)
          break
        case 'function':
          result = await executeFunctionHook(hook, input)
          break
        case 'http':
          result = await execHttpHook(hook, input)
          break
        case 'agent':
          result = await execAgentHook(hook, input)
          break
      }
      
      // 记录执行时间
      const duration = Date.now() - hookStartTime
      addToTurnHookDuration(duration)
      
      // 发射完成事件
      emitHookExecutionCompleted({ hookId: hook.id, duration })
      
      return result
    })
  )
  
  // 4. 聚合结果
  const aggregated = aggregateResults(results)
  
  // 5. 发射完成事件
  emitHookCompleted({ event, duration: Date.now() - startTime })
  
  return aggregated
}
```

### 1.4 会话级 Hook (Session Hooks)

```typescript
// src/utils/hooks/sessionHooks.ts

/**
 * 会话级 Hook 存储
 * 仅存在于内存中，会话结束即清除
 */
export type SessionHooksState = Map<string, SessionStore>

type SessionStore = {
  hooks: {
    [event in HookEvent]?: SessionHookMatcher[]
  }
}

type SessionHookMatcher = {
  matcher: string
  skillRoot?: string
  hooks: Array<{
    hook: HookCommand | FunctionHook
    onHookSuccess?: OnHookSuccess
  }>
}

/**
 * Function Hook - TypeScript 回调验证
 * 用于动态验证场景
 */
export type FunctionHook = {
  type: 'function'
  id?: string                    // 可选唯一 ID
  timeout?: number               // 超时 ms
  callback: FunctionHookCallback // TS 回调
  errorMessage: string
  statusMessage?: string
}

export type FunctionHookCallback = (
  messages: Message[],
  signal?: AbortSignal,
) => boolean | Promise<boolean>

/**
 * 添加会话级 Function Hook
 * 返回 hook ID 用于后续移除
 */
export function addFunctionHook(
  setAppState: (updater: (prev: AppState) => AppState) => void,
  sessionId: string,
  event: HookEvent,
  matcher: string,
  callback: FunctionHookCallback,
  errorMessage: string,
  options?: {
    timeout?: number
    id?: string
  },
): string {
  const id = options?.id || `function-hook-${Date.now()}-${Math.random()}`
  
  const hook: FunctionHook = {
    type: 'function',
    id,
    timeout: options?.timeout || 5000,
    callback,
    errorMessage,
  }
  
  addHookToSession(setAppState, sessionId, event, matcher, hook)
  return id
}

/**
 * 移除 Function Hook
 */
export function removeFunctionHook(
  setAppState: (updater: (prev: AppState) => AppState) => void,
  sessionId: string,
  event: HookEvent,
  hookId: string,
): void {
  setAppState(prev => {
    const store = prev.sessionHooks.get(sessionId)
    if (!store) return prev
    
    // 从所有 matcher 中移除匹配的 hook
    const eventMatchers = store.hooks[event] || []
    const updatedMatchers = eventMatchers
      .map(matcher => ({
        ...matcher,
        hooks: matcher.hooks.filter(h => 
          h.hook.type !== 'function' || h.hook.id !== hookId
        ),
      }))
      .filter(m => m.hooks.length > 0)
    
    // 更新 state
    prev.sessionHooks.set(sessionId, { 
      hooks: {
        ...store.hooks,
        [event]: updatedMatchers.length > 0 ? updatedMatchers : undefined,
      }
    })
    return prev
  })
}
```

### 1.5 异步 Hook 与唤醒机制

```typescript
// src/utils/hooks/AsyncHookRegistry.ts

/**
 * 异步 Hook 注册表
 * 跟踪后台运行的 Hook 状态
 */
type PendingAsyncHook = {
  hookId: string
  processId: string
  hookEvent: HookEvent
  hookName: string
  command: string
  startTime: number
  shellCommand: ShellCommand
}

const pendingAsyncHooks = new Map<string, PendingAsyncHook>()

/**
 * 注册异步 Hook
 */
function registerPendingAsyncHook(hook: PendingAsyncHook): void {
  pendingAsyncHooks.set(hook.hookId, hook)
}

/**
 * 异步 Hook 完成时的处理
 */
async function handleAsyncHookCompletion(
  hookId: string,
  result: { code: number; stdout: string; stderr: string }
): Promise<void> {
  const hook = pendingAsyncHooks.get(hookId)
  if (!hook) return
  
  // 清理注册表
  pendingAsyncHooks.delete(hookId)
  
  // 发射响应事件
  emitHookResponse({
    hookId,
    hookName: hook.hookName,
    hookEvent: hook.hookEvent,
    output: result.stdout + result.stderr,
    stdout: result.stdout,
    stderr: result.stderr,
    exitCode: result.code,
    outcome: result.code === 0 ? 'success' : 'error',
  })
  
  // asyncRewake: 退出码 2 时唤醒模型
  if (hook.asyncRewake && result.code === 2) {
    enqueuePendingNotification({
      value: wrapInSystemReminder(
        `Stop hook blocking error from command "${hook.hookName}": ${result.stderr || result.stdout}`
      ),
      mode: 'task-notification',
    })
  }
}
```

### 1.6 Hook 超时管理

```typescript
// src/utils/hooks.ts

// 工具 Hook 执行超时：10 分钟
const TOOL_HOOK_EXECUTION_TIMEOUT_MS = 10 * 60 * 1000

// SessionEnd Hook 超时：1.5 秒（默认）
const SESSION_END_HOOK_TIMEOUT_MS_DEFAULT = 1500

/**
 * 获取 SessionEnd Hook 超时
 * 可通过环境变量覆盖
 */
export function getSessionEndHookTimeoutMs(): number {
  const raw = process.env.CLAUDE_CODE_SESSIONEND_HOOKS_TIMEOUT_MS
  const parsed = raw ? parseInt(raw, 10) : NaN
  return Number.isFinite(parsed) && parsed > 0
    ? parsed
    : SESSION_END_HOOK_TIMEOUT_MS_DEFAULT
}

/**
 * 执行带超时的 Hook
 */
async function executeHookWithTimeout(
  hook: HookCommand,
  input: HookInput,
  signal?: AbortSignal
): Promise<HookResult> {
  const timeout = hook.timeout ?? TOOL_HOOK_EXECUTION_TIMEOUT_MS
  
  const controller = new AbortController()
  signal?.addEventListener('abort', () => controller.abort())
  
  const timeoutId = setTimeout(() => controller.abort(), timeout)
  
  try {
    return await executeHook(hook, input, { signal: controller.signal })
  } catch (error) {
    if (error instanceof AbortError) {
      return {
        blocked: true,
        reason: `Hook '${hook.matcher}' timed out after ${timeout}ms`,
      }
    }
    throw error
  } finally {
    clearTimeout(timeoutId)
  }
}
```

### 1.7 Hook 事件遥测

```typescript
// src/utils/hooks/hookEvents.ts

import { startHookSpan, endHookSpan } from '../telemetry/sessionTracing'

/**
 * 发射 Hook 开始事件
 */
export function emitHookStarted(data: {
  event: HookEvent
  hookCount: number
}): void {
  logEvent('hook_started', {
    event: data.event,
    hook_count: data.hookCount,
    timestamp: Date.now(),
  })
}

/**
 * 发射 Hook 执行开始
 */
export function emitHookExecutionStarted(data: {
  hookId: string
  hookName: string
}): void {
  startHookSpan(data.hookId, data.hookName)
  
  logForDiagnosticsNoPII('hook_execution_started', {
    hook_id: data.hookId,
    hook_name: data.hookName,
  })
}

/**
 * 发射 Hook 执行完成
 */
export function emitHookExecutionCompleted(data: {
  hookId: string
  duration: number
  exitCode?: number
}): void {
  endHookSpan(data.hookId, {
    duration_ms: data.duration,
    exit_code: data.exitCode,
  })
  
  logEvent('hook_completed', {
    hook_id: data.hookId,
    duration_ms: data.duration,
  })
}

/**
 * 开始 Hook 进度轮询
 * 每 2 秒发射一次进度事件
 */
export function startHookProgressInterval(
  hookId: string,
  hookName: string
): () => void {
  const intervalId = setInterval(() => {
    emitHookResponse({
      hookId,
      hookName,
      output: `Hook ${hookName} still running...`,
      exitCode: -1,
      outcome: 'running',
    })
  }, 2000)
  
  return () => clearInterval(intervalId)
}
```

---

## 2. Yode 实现建议

### 2.1 Hook 事件枚举

```rust
// crates/yode-core/src/hooks/events.rs

use serde::{Deserialize, Serialize};

/// Hook 事件类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookEvent {
    // 会话生命周期
    SessionStart,
    SessionEnd,
    Setup,
    Stop,
    
    // 用户交互
    UserPromptSubmit,
    PermissionRequest,
    PermissionDenied,
    
    // 工具执行
    PreToolUse,
    PostToolUse,
    PostToolUseFailure,
    
    // 上下文管理
    PreCompact,
    PostCompact,
    
    // 子代理
    SubagentStart,
    SubagentStop,
    
    // 任务
    TaskCreated,
    TaskCompleted,
    
    // 状态变化
    ConfigChange,
    CwdChanged,
    FileChanged,
    InstructionsLoaded,
}

/// Hook 输入类型
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event")]
pub enum HookInput {
    #[serde(rename = "pre_tool_use")]
    PreToolUse(PreToolUseInput),
    
    #[serde(rename = "post_tool_use")]
    PostToolUse(PostToolUseInput),
    
    #[serde(rename = "user_prompt_submit")]
    UserPromptSubmit(UserPromptSubmitInput),
    
    #[serde(rename = "permission_request")]
    PermissionRequest(PermissionRequestInput),
    
    // ... 更多事件类型
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreToolUseInput {
    pub tool_name: String,
    pub tool_input: serde_json::Value,
    pub session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostToolUseInput {
    pub tool_name: String,
    pub tool_input: serde_json::Value,
    pub tool_output: String,
    pub is_error: bool,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserPromptSubmitInput {
    pub prompt: String,
    pub session_id: String,
}
```

### 2.2 Hook 命令定义

```rust
// crates/yode-core/src/hooks/command.rs

use std::time::Duration;

/// Hook 命令类型
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HookCommandType {
    /// Shell 命令
    Command { command: String },
    /// 提示覆盖
    Prompt { prompt: String },
    /// HTTP 请求
    Http { url: String, method: String },
    /// Agent 执行
    Agent { agent_type: String },
}

/// Hook 命令
#[derive(Debug, Clone)]
pub struct HookCommand {
    /// 事件类型
    pub event: HookEvent,
    /// 匹配模式（glob 或 regex）
    pub matcher: String,
    /// 命令类型
    pub command_type: HookCommandType,
    /// 超时时间
    pub timeout: Duration,
    /// 是否异步
    pub r#async: bool,
    /// 异步唤醒（退出码 2 时唤醒模型）
    pub async_rewake: bool,
    /// 插件 ID（如果来自插件）
    pub plugin_id: Option<String>,
}

impl HookCommand {
    pub fn new_command(
        event: HookEvent,
        matcher: String,
        command: String,
    ) -> Self {
        Self {
            event,
            matcher,
            command_type: HookCommandType::Command { command },
            timeout: Duration::from_secs(30),
            r#async: false,
            async_rewake: false,
            plugin_id: None,
        }
    }
    
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
    
    pub fn with_async(mut self, rewake: bool) -> Self {
        self.r#async = true;
        self.async_rewake = rewake;
        self
    }
}
```

### 2.3 Hook 执行器

```rust
// crates/yode-core/src/hooks/executor.rs

use anyhow::{Context, Result};
use std::sync::Arc;
use tokio::process::Command as TokioCommand;
use tokio::sync::broadcast;

/// Hook 执行结果
#[derive(Debug, Clone)]
pub struct HookResult {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub duration_ms: u64,
    pub blocked: bool,
    pub reason: Option<String>,
}

/// Hook 执行器
pub struct HookExecutor {
    /// 工作目录
    working_dir: PathBuf,
    /// 进度广播
    progress_tx: broadcast::Sender<HookProgress>,
}

#[derive(Debug, Clone)]
pub enum HookProgress {
    Started { hook_id: String },
    Output { stdout: String, stderr: String },
    Completed { exit_code: i32 },
}

impl HookExecutor {
    pub fn new(working_dir: PathBuf) -> Self {
        let (progress_tx, _) = broadcast::channel(100);
        Self {
            working_dir,
            progress_tx,
        }
    }
    
    /// 执行 Hook 命令
    pub async fn execute(
        &self,
        hook: &HookCommand,
        input: &HookInput,
    ) -> Result<HookResult> {
        let start_time = Instant::now();
        
        match &hook.command_type {
            HookCommandType::Command { command } => {
                self.execute_shell_command(command, hook.timeout).await
            }
            HookCommandType::Prompt { prompt } => {
                // Prompt hook 返回新的系统提示
                Ok(HookResult {
                    exit_code: 0,
                    stdout: prompt.clone(),
                    stderr: String::new(),
                    duration_ms: 0,
                    blocked: false,
                    reason: None,
                })
            }
            HookCommandType::Http { url, method } => {
                self.execute_http_hook(url, method, input, hook.timeout).await
            }
            HookCommandType::Agent { agent_type } => {
                self.execute_agent_hook(agent_type, input, hook.timeout).await
            }
        }
    }
    
    /// 执行 Shell 命令
    async fn execute_shell_command(
        &self,
        command: &str,
        timeout: Duration,
    ) -> Result<HookResult> {
        let start_time = Instant::now();
        
        let mut cmd = TokioCommand::new("sh");
        cmd.arg("-c").arg(command);
        cmd.current_dir(&self.working_dir);
        cmd.kill_on_drop(true);
        
        // 带超时执行
        let output = tokio::time::timeout(timeout, cmd.output())
            .await
            .context("Hook command timed out")?
            .context("Failed to execute hook command")?;
        
        let duration_ms = start_time.elapsed().as_millis() as u64;
        
        Ok(HookResult {
            exit_code: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            duration_ms,
            blocked: false,
            reason: None,
        })
    }
    
    /// 执行 HTTP Hook
    async fn execute_http_hook(
        &self,
        url: &str,
        method: &str,
        input: &HookInput,
        timeout: Duration,
    ) -> Result<HookResult> {
        let client = reqwest::Client::builder()
            .timeout(timeout)
            .build()?;
        
        let start_time = Instant::now();
        
        let response = client
            .request(reqwest::Method::from_bytes(method.as_bytes())?, url)
            .json(input)
            .send()
            .await?;
        
        let status = response.status();
        let body = response.text().await?;
        let duration_ms = start_time.elapsed().as_millis() as u64;
        
        Ok(HookResult {
            exit_code: status.as_u16() as i32,
            stdout: body,
            stderr: String::new(),
            duration_ms,
            blocked: !status.is_success(),
            reason: if !status.is_success() {
                Some(format!("HTTP {}", status))
            } else {
                None
            },
        })
    }
}
```

### 2.4 Hook 注册表

```rust
// crates/yode-core/src/hooks/registry.rs

use std::collections::HashMap;
use parking_lot::RwLock;

/// Hook 注册表
pub struct HookRegistry {
    /// 按事件类型分组的 Hook
    hooks_by_event: RwLock<HashMap<HookEvent, Vec<HookCommand>>>,
    /// 会话级 Hook（临时）
    session_hooks: RwLock<HashMap<String, SessionHookStore>>,
}

struct SessionHookStore {
    hooks: HashMap<HookEvent, Vec<HookCommand>>,
}

impl HookRegistry {
    pub fn new() -> Self {
        Self {
            hooks_by_event: RwLock::new(HashMap::new()),
            session_hooks: RwLock::new(HashMap::new()),
        }
    }
    
    /// 注册 Hook
    pub fn register(&self, hook: HookCommand) {
        let event = hook.event;
        self.hooks_by_event
            .write()
            .entry(event)
            .or_default()
            .push(hook);
    }
    
    /// 注册会话级 Hook
    pub fn register_session_hook(
        &self,
        session_id: String,
        hook: HookCommand,
    ) {
        let mut stores = self.session_hooks.write();
        let store = stores
            .entry(session_id.clone())
            .or_insert_with(|| SessionHookStore {
                hooks: HashMap::new(),
            });
        
        store
            .hooks
            .entry(hook.event)
            .or_default()
            .push(hook);
    }
    
    /// 获取匹配的 Hook
    pub fn get_matching_hooks(
        &self,
        event: HookEvent,
        input: &HookInput,
        session_id: Option<&str>,
    ) -> Vec<HookCommand> {
        let mut hooks = Vec::new();
        
        // 1. 全局 Hook
        if let Some(global_hooks) = self.hooks_by_event.read().get(&event) {
            for hook in global_hooks {
                if self.matches(&hook.matcher, input) {
                    hooks.push(hook.clone());
                }
            }
        }
        
        // 2. 会话级 Hook
        if let Some(sid) = session_id {
            if let Some(store) = self.session_hooks.read().get(sid) {
                if let Some(session_hooks) = store.hooks.get(&event) {
                    for hook in session_hooks {
                        if self.matches(&hook.matcher, input) {
                            hooks.push(hook.clone());
                        }
                    }
                }
            }
        }
        
        hooks
    }
    
    /// 检查输入是否匹配模式
    fn matches(&self, pattern: &str, input: &HookInput) -> bool {
        // 支持 glob 和 regex 模式
        // 简化实现：完全匹配或前缀匹配
        match input {
            HookInput::PreToolUse(input) => {
                pattern == "*" || 
                pattern == input.tool_name ||
                pattern.starts_with(&input.tool_name)
            }
            _ => true,
        }
    }
    
    /// 清理会话 Hook
    pub fn cleanup_session(&self, session_id: &str) {
        self.session_hooks.write().remove(session_id);
    }
}
```

### 2.5 配置文件设计

```toml
# ~/.config/yode/config.toml

[[hooks]]
event = "pre_tool_use"
matcher = "Bash"
type = "command"
command = "echo 'Executing bash command: $INPUT'"
timeout_secs = 10

[[hooks]]
event = "user_prompt_submit"
matcher = "*"
type = "http"
url = "http://localhost:8080/hook"
method = "POST"
timeout_secs = 5

[[hooks]]
event = "post_tool_use"
matcher = "FileEditTool"
type = "command"
command = "git diff --stat"
async = true

# 项目级 Hook (.yode/config.toml)
[[hooks]]
event = "instructions_loaded"
matcher = "*"
type = "command"
command = "echo 'Project instructions loaded'"
```

---

## 3. 关键设计要点

### 3.1 Hook 执行顺序

```
1. session 级 Hook（最高优先级）
2. command 级 Hook
3. cliArg 级 Hook
4. localSettings 级 Hook
5. projectSettings 级 Hook
6. userSettings 级 Hook（最低优先级）
```

### 3.2 退出码约定

| 退出码 | 含义 |
|--------|------|
| 0 | 成功，继续执行 |
| 1 | 失败，阻止操作 |
| 2 | asyncRewake：异步唤醒模型 |

### 3.3 异步 Hook 生命周期

```
注册 → 后台执行 → 完成/超时 → 清理
         ↓
    发射进度事件
         ↓
    退出码 2 → 唤醒模型
```

---

## 4. 总结

Claude Code Hook 系统特点：

1. **事件驱动** - 30+ 种事件类型覆盖完整生命周期
2. **多类型支持** - command/prompt/function/http/agent
3. **会话级 Hook** - 临时、内存中、自动清理
4. **异步执行** - 后台运行、进度追踪、唤醒机制
5. **遥测集成** - 完整的执行追踪和日志

Yode 可以借鉴：
- 会话级 Function Hook（TS 回调的 Rust 等价物：闭包）
- 异步 Hook 与唤醒机制
- Hook 事件遥测
- 统一的超时管理
