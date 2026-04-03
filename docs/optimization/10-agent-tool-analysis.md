# AgentTool 深度分析

## 1. Claude Code AgentTool 架构

### 1.1 工具定义概览

```typescript
// src/tools/AgentTool/AgentTool.tsx (800+ 行)

export const AgentTool = buildTool({
  name: AGENT_TOOL_NAME,
  searchHint: 'delegate work to a subagent',
  maxResultSizeChars: 100_000,
  async description() {
    return 'Launch a new agent'
  },
  inputSchema: () => inputSchema(),
  outputSchema: () => outputSchema(),
  async call(input, toolUseContext, canUseTool, assistantMessage, onProgress) {
    // 完整的子代理启动逻辑
  },
})
```

### 1.2 输入 Schema（支持多代理）

```typescript
// 基础输入 Schema
const baseInputSchema = lazySchema(() => z.object({
  description: z.string().describe('A short (3-5 word) description of the task'),
  prompt: z.string().describe('The task for the agent to perform'),
  subagent_type: z.string().optional().describe('The type of specialized agent to use'),
  model: z.enum(['sonnet', 'opus', 'haiku']).optional().describe('Optional model override'),
  run_in_background: z.boolean().optional().describe('Set to true to run in background'),
}))

// 完整 Schema（包含多代理参数）
const fullInputSchema = lazySchema(() => {
  const multiAgentInputSchema = z.object({
    name: z.string().optional().describe('Name for the spawned agent'),
    team_name: z.string().optional().describe('Team name for spawning'),
    mode: permissionModeSchema().optional().describe('Permission mode for spawned teammate'),
  })
  return baseInputSchema().merge(multiAgentInputSchema).extend({
    isolation: z.enum(['worktree', 'remote']).optional().describe('Isolation mode'),
    cwd: z.string().optional().describe('Absolute path to run the agent in'),
  })
})

// 条件性 Schema（根据特性标志和模式）
export const inputSchema = lazySchema(() => {
  const schema = feature('KAIROS') ? fullInputSchema() : fullInputSchema().omit({ cwd: true })
  
  return isBackgroundTasksDisabled || isForkSubagentEnabled() 
    ? schema.omit({ run_in_background: true })
    : schema
})
```

### 1.3 输出 Schema（同步/异步）

```typescript
export const outputSchema = lazySchema(() => {
  // 同步执行输出
  const syncOutputSchema = agentToolResultSchema().extend({
    status: z.literal('completed'),
    prompt: z.string()
  })
  
  // 异步执行输出
  const asyncOutputSchema = z.object({
    status: z.literal('async_launched'),
    agentId: z.string(),
    description: z.string(),
    prompt: z.string(),
    outputFile: z.string().describe('Path to the output file'),
    canReadOutputFile: z.boolean().optional(),
  })
  
  return z.union([syncOutputSchema, asyncOutputSchema])
})
```

### 1.4 call 执行流程

```typescript
async call(input, toolUseContext, canUseTool, assistantMessage, onProgress) {
  const startTime = Date.now()
  const { 
    prompt, 
    subagent_type, 
    description, 
    model: modelParam, 
    run_in_background,
    name,
    team_name,
    mode: spawnMode,
    isolation,
    cwd 
  } = input
  
  const appState = toolUseContext.getAppState()
  const permissionMode = appState.toolPermissionContext.mode
  const rootSetAppState = toolUseContext.setAppStateForTasks ?? toolUseContext.setAppState
  
  // 1. 检查 Agent Teams 可用性
  if (team_name && !isAgentSwarmsEnabled()) {
    throw new Error('Agent Teams is not yet available on your plan.')
  }
  
  // 2. 解析 Team 名称
  const teamName = resolveTeamName({ team_name }, appState)
  
  // 3. 检查嵌套 spawn（不允许）
  if (isTeammate() && teamName && name) {
    throw new Error('Teammates cannot spawn other teammates...')
  }
  
  // 4. 检查 in-process teammate 后台任务
  if (isInProcessTeammate() && teamName && run_in_background === true) {
    throw new Error('In-process teammates cannot spawn background agents.')
  }
  
  // 5. 多代理 spawn 请求
  if (teamName && name) {
    const agentDef = toolUseContext.options.agentDefinitions.activeAgents
      .find(a => a.agentType === subagent_type)
    
    if (agentDef?.color) {
      setAgentColor(subagent_type!, agentDef.color)
    }
    
    const result = await spawnTeammate({
      name,
      prompt,
      description,
      team_name: teamName,
      use_splitpane: true,
      plan_mode_required: spawnMode === 'plan',
      model: model ?? agentDef?.model,
      agent_type: subagent_type,
      invokingRequestId: assistantMessage?.requestId
    }, toolUseContext)
    
    return { data: result }
  }
  
  // 6. 计算自动后台阈值
  const autoBackgroundMs = getAutoBackgroundMs()
  const shouldForceAsync = autoBackgroundMs > 0 && 
    !isInForkChild() && 
    permissionMode !== 'plan' &&
    (run_in_background !== false)
  
  // 7. 同步/前台执行
  if (!shouldForceAsync) {
    return await runAgentSync({
      prompt,
      subagent_type,
      description,
      model,
      name,
      teamName,
      isolation,
      cwd,
      toolUseContext,
      canUseTool,
      onProgress,
      startTime,
    })
  }
  
  // 8. 异步/后台执行
  return await runAgentAsync({
    prompt,
    subagent_type,
    description,
    model,
    name,
    teamName,
    isolation,
    cwd,
    toolUseContext,
    canUseTool,
    startTime,
  })
}
```

### 1.5 同步执行流程

```typescript
async function runAgentSync(options) {
  const { 
    prompt, subagent_type, description, model, 
    name, teamName, isolation, cwd,
    toolUseContext, canUseTool, onProgress, startTime 
  } = options
  
  // 1. 工作树隔离（如果启用）
  let worktreePath: string | undefined
  if (isolation === 'worktree') {
    worktreePath = await createAgentWorktree()
  }
  
  // 2. 获取子代理定义
  const agents = toolUseContext.options.agentDefinitions.activeAgents
  const agentDef = agents.find(a => a.agentType === subagent_type) || 
                   agents.find(a => a.agentType === 'general-purpose')
  
  // 3. 获取模型
  const agentModel = getAgentModel(model, agentDef)
  
  // 4. 构建系统提示
  const systemPrompt = await buildEffectiveSystemPrompt({
    model: agentModel,
    agentDefinition: agentDef,
    options: toolUseContext.options,
    permissionContext: toolUseContext.getAppState().toolPermissionContext,
  })
  
  // 5. 构建消息历史（forked messages）
  const forkedMessages = buildForkedMessages({
    prompt,
    parentMessage: toolUseContext.getParentMessage(),
  })
  
  // 6. 运行子代理
  const result = await runAgent({
    model: agentModel,
    systemPrompt,
    messages: forkedMessages,
    tools: assembleToolPool(...),
    cwd: cwd || getCwd(),
    onProgress,
    canUseTool,
  })
  
  // 7. 清理工作树
  if (isolation === 'worktree' && worktreePath) {
    await removeAgentWorktree(worktreePath)
  }
  
  // 8. 记录遥测
  logEvent('agent_sync_completed', {
    duration_ms: Date.now() - startTime,
    agent_type: subagent_type,
  })
  
  return { data: { status: 'completed', prompt, ...result } }
}
```

### 1.6 异步执行流程

```typescript
async function runAgentAsync(options) {
  const { 
    prompt, subagent_type, description, model,
    name, teamName, isolation, cwd,
    toolUseContext, startTime 
  } = options
  
  // 1. 创建 Agent ID
  const agentId = asAgentId(createAgentId())
  
  // 2. 注册异步代理任务
  const taskInfo = await registerAsyncAgent({
    agentId,
    description,
    prompt,
    model,
    subagent_type,
    teamName,
    isolation,
    cwd,
    rootSetAppState,
  })
  
  // 3. 后台启动任务
  const { taskPromise } = createLocalAgentTask({
    agentId,
    prompt,
    model,
    tools: assembleToolPool(...),
    systemPrompt: await buildEffectiveSystemPrompt(...),
    onProgress: (progress) => {
      updateAgentProgress(agentId, progress)
      onProgress?.(progress)
    },
  })
  
  // 4. 启动后台执行
  taskPromise.catch((err) => {
    failAsyncAgent(agentId, err)
  })
  
  // 5. 记录遥测
  logEvent('agent_async_launched', {
    duration_ms: Date.now() - startTime,
    agent_type: subagent_type,
    agent_id: agentId,
  })
  
  // 6. 返回启动通知
  return { 
    data: {
      status: 'async_launched',
      agentId,
      description,
      prompt,
      outputFile: getTaskOutputPath(agentId),
      canReadOutputFile: canUseTool('read_file') || canUseTool('bash'),
    }
  }
}
```

### 1.7 进度追踪

```typescript
// src/types/tools.ts

export type AgentToolProgress = {
  type: 'agent_progress'
  description: string
  currentTask: string
  agentId?: string
}

// AgentTool 内部进度追踪
function createProgressTracker(agentId: string) {
  return {
    update(description: string, currentTask: string) {
      emitTaskProgress(agentId, {
        type: 'agent_progress',
        description,
        currentTask,
      })
    },
    
    tokenCount(tokens: number) {
      emitTaskProgress(agentId, {
        type: 'agent_token_usage',
        tokens,
      })
    },
  }
}

// 转发 Bash 进度
function forwardBashProgress(agentId: string, bashProgress: BashProgress) {
  emitTaskProgress(agentId, {
    type: 'bash_progress',
    command: bashProgress.command,
    output: bashProgress.output,
    isRunning: bashProgress.isRunning,
  })
}
```

### 1.8 内置 Agent 类型

```typescript
// src/tools/AgentTool/builtInAgents.ts

export const BUILTIN_AGENTS = [
  {
    agentType: 'general-purpose',
    name: 'General Purpose Agent',
    description: 'A versatile agent for various tasks',
    model: 'sonnet',
  },
  {
    agentType: 'explore',
    name: 'Explore Agent',
    description: 'Specialized in exploring codebases',
    model: 'sonnet',
  },
  {
    agentType: 'plan',
    name: 'Plan Agent',
    description: 'Creates detailed plans for complex tasks',
    model: 'opus',
  },
  {
    agentType: 'statusline-setup',
    name: 'Statusline Setup Agent',
    description: 'Configures the statusline',
    model: 'haiku',
  },
]
```

---

## 2. Yode 实现建议

### 2.1 Rust 实现

```rust
// crates/yode-tools/src/builtin/agent.rs

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;

/// Agent 输入
#[derive(Debug, Clone, Deserialize)]
pub struct AgentInput {
    /// 3-5 词的任务描述
    pub description: String,
    /// 详细任务提示
    pub prompt: String,
    /// 子代理类型
    #[serde(default = "default_subagent_type")]
    pub subagent_type: String,
    /// 模型覆盖
    pub model: Option<String>,
    /// 是否后台运行
    #[serde(default)]
    pub run_in_background: bool,
    /// 代理名称（用于寻址）
    pub name: Option<String>,
    /// 工作树隔离
    pub isolation: Option<IsolationMode>,
    /// 工作目录覆盖
    pub cwd: Option<PathBuf>,
}

fn default_subagent_type() -> String {
    "general-purpose".to_string()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IsolationMode {
    /// 创建临时 git worktree
    Worktree,
    /// 远程环境（总是后台运行）
    Remote,
}

/// Agent 输出
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "status")]
pub enum AgentOutput {
    /// 同步执行完成
    #[serde(rename = "completed")]
    Completed {
        prompt: String,
        result: String,
        token_usage: TokenUsage,
    },
    /// 异步启动
    #[serde(rename = "async_launched")]
    AsyncLaunched {
        agent_id: String,
        description: String,
        prompt: String,
        output_file: PathBuf,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct TokenUsage {
    input_tokens: u32,
    output_tokens: u32,
    total_tokens: u32,
}

/// Agent 进度
#[derive(Debug, Clone)]
pub enum AgentProgress {
    Message {
        description: String,
        current_task: String,
    },
    TokenUsage {
        tokens: u32,
    },
    BashOutput {
        command: String,
        output: String,
    },
}

/// Agent 工具实现
pub struct AgentTool {
    /// 内置代理定义
    agent_definitions: Vec<AgentDefinition>,
    /// 任务运行时
    task_runtime: Arc<TaskRuntime>,
}

struct AgentDefinition {
    agent_type: String,
    name: String,
    description: String,
    default_model: String,
}

impl AgentTool {
    pub fn new() -> Self {
        let agent_definitions = vec![
            AgentDefinition {
                agent_type: "general-purpose".to_string(),
                name: "General Purpose Agent".to_string(),
                description: "A versatile agent for various tasks".to_string(),
                default_model: "claude-sonnet".to_string(),
            },
            AgentDefinition {
                agent_type: "explore".to_string(),
                name: "Explore Agent".to_string(),
                description: "Specialized in exploring codebases".to_string(),
                default_model: "claude-sonnet".to_string(),
            },
            AgentDefinition {
                agent_type: "plan".to_string(),
                name: "Plan Agent".to_string(),
                description: "Creates detailed plans for complex tasks".to_string(),
                default_model: "claude-opus".to_string(),
            },
        ];
        
        Self {
            agent_definitions,
            task_runtime: Arc::new(TaskRuntime::new()),
        }
    }
    
    /// 执行 Agent 调用
    pub async fn execute(
        &self,
        input: AgentInput,
        context: &ToolContext,
    ) -> ToolResult {
        // 1. 确定是否强制异步
        let should_force_async = self.should_force_async(&input, context);
        
        if should_force_async {
            self.execute_async(input, context).await
        } else {
            self.execute_sync(input, context).await
        }
    }
    
    /// 判断是否强制异步
    fn should_force_async(&self, input: &AgentInput, context: &ToolContext) -> bool {
        // 用户显式请求后台运行
        if input.run_in_background {
            return true;
        }
        
        // 任务预计较长（基于 prompt 长度或类型）
        let is_long_task = input.prompt.len() > 1000 || 
                          input.subagent_type == "explore";
        
        // 当前不是 plan 模式
        context.permission_mode != PermissionMode::Plan
    }
    
    /// 同步执行
    async fn execute_sync(
        &self,
        input: AgentInput,
        context: &ToolContext,
    ) -> ToolResult {
        // 1. 创建工作树（如果启用隔离）
        let worktree_guard = if matches!(input.isolation, Some(IsolationMode::Worktree)) {
            Some(create_agent_worktree(context.working_dir.clone())?)
        } else {
            None
        };
        
        // 2. 获取代理定义
        let agent_def = self.agent_definitions
            .iter()
            .find(|d| d.agent_type == input.subagent_type)
            .unwrap_or_else(|| {
                self.agent_definitions
                    .iter()
                    .find(|d| d.agent_type == "general-purpose")
                    .expect("general-purpose agent should exist")
            });
        
        // 3. 确定模型
        let model = input.model
            .unwrap_or_else(|| agent_def.default_model.clone());
        
        // 4. 构建系统提示
        let system_prompt = self.build_system_prompt(agent_def, context);
        
        // 5. 构建 forked 消息
        let forked_messages = vec![
            Message::user(&input.prompt),
        ];
        
        // 6. 创建进度通道
        let (progress_tx, mut progress_rx) = mpsc::channel(100);
        
        // 7. 运行子代理
        let result = tokio::select! {
            result = self.run_subagent(
                &model,
                &system_prompt,
                forked_messages,
                input.cwd.unwrap_or_else(|| context.working_dir.clone()),
                progress_tx,
            ) => result,
            
            // 处理进度更新
            Some(progress) = progress_rx.recv() => {
                self.handle_progress(progress, context);
                // 继续等待结果...
            }
        }?;
        
        // 8. 清理工作树
        drop(worktree_guard);
        
        // 9. 记录遥测
        log_event("agent_completed", serde_json::json!({
            "agent_type": input.subagent_type,
            "duration_ms": result.duration_ms,
            "token_usage": result.token_usage,
        }));
        
        ToolResult::success(AgentOutput::Completed {
            prompt: input.prompt,
            result: result.content,
            token_usage: result.token_usage,
        })
    }
    
    /// 异步执行
    async fn execute_async(
        &self,
        input: AgentInput,
        context: &ToolContext,
    ) -> ToolResult {
        // 1. 创建 Agent ID
        let agent_id = format!("agent_{}", uuid::Uuid::new_v4());
        
        // 2. 注册异步任务
        let task_info = self.task_runtime.register_agent(AgentTaskInfo {
            agent_id: agent_id.clone(),
            description: input.description.clone(),
            prompt: input.prompt.clone(),
            model: input.model.clone(),
            subagent_type: input.subagent_type.clone(),
            cwd: input.cwd.clone(),
        });
        
        // 3. 后台启动任务
        let task_runtime = Arc::clone(&self.task_runtime);
        tokio::spawn(async move {
            let result = task_runtime.run_agent_task(task_info).await;
            
            if let Err(err) = result {
                log_error!("Agent task failed: {}", err);
                task_runtime.fail_agent(&task_info.agent_id, err);
            }
        });
        
        // 4. 返回启动通知
        ToolResult::success(AgentOutput::AsyncLaunched {
            agent_id,
            description: input.description,
            prompt: input.prompt,
            output_file: self.task_runtime.get_output_path(&agent_id),
        })
    }
    
    /// 运行子代理
    async fn run_subagent(
        &self,
        model: &str,
        system_prompt: &str,
        messages: Vec<Message>,
        cwd: PathBuf,
        progress_tx: mpsc::Sender<AgentProgress>,
    ) -> Result<AgentResult> {
        // 创建子代理运行器
        let mut subagent = SubAgent::new(
            model,
            system_prompt,
            cwd,
            progress_tx,
        );
        
        // 运行对话循环
        subagent.run(messages).await
    }
    
    /// 处理进度更新
    fn handle_progress(&self, progress: AgentProgress, context: &ToolContext) {
        match progress {
            AgentProgress::Message { description, current_task } => {
                // 发送到 TUI 状态栏
                context.status_tx.send(format!("{}: {}", description, current_task)).ok();
            }
            AgentProgress::TokenUsage { tokens } => {
                // 更新 token 计数
                context.token_counter.add(tokens);
            }
            AgentProgress::BashOutput { command, output } => {
                // 转发 Bash 输出
                context.status_tx.send(format!("$ {}", command)).ok();
            }
        }
    }
    
    /// 构建系统提示
    fn build_system_prompt(&self, agent_def: &AgentDefinition, context: &ToolContext) -> String {
        format!(
            r#"You are {name}, a specialized AI agent.

# Your Role
{description}

# Available Tools
{tools}

# Guidelines
- Be concise and action-oriented
- Report progress frequently
- Ask for clarification when needed
- Use Chinese for communication
"#,
            name = agent_def.name,
            description = agent_def.description,
            tools = context.available_tools.join("\n"),
        )
    }
}
```

### 2.2 任务运行时

```rust
// crates/yode-core/src/task_runtime.rs

use std::collections::HashMap;
use std::sync::Arc;
use parking_lot::RwLock;
use tokio::sync::broadcast;

pub struct TaskRuntime {
    /// 活跃任务
    tasks: RwLock<HashMap<String, Arc<TaskInfo>>>,
    /// 输出目录
    output_dir: PathBuf,
    /// 进度广播
    progress_tx: broadcast::Sender<TaskProgress>,
}

pub struct TaskInfo {
    pub agent_id: String,
    pub description: String,
    pub prompt: String,
    pub status: TaskStatus,
    pub start_time: Instant,
    pub end_time: Option<Instant>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

impl TaskRuntime {
    pub fn new() -> Self {
        let (progress_tx, _) = broadcast::channel(100);
        
        Self {
            tasks: RwLock::new(HashMap::new()),
            output_dir: std::env::temp_dir().join("yode_tasks"),
            progress_tx,
        }
    }
    
    /// 注册 Agent 任务
    pub fn register_agent(&self, info: AgentTaskInfo) -> Arc<TaskInfo> {
        let task_info = Arc::new(TaskInfo {
            agent_id: info.agent_id.clone(),
            description: info.description,
            prompt: info.prompt,
            status: TaskStatus::Pending,
            start_time: Instant::now(),
            end_time: None,
            error: None,
        });
        
        self.tasks
            .write()
            .insert(info.agent_id, Arc::clone(&task_info));
        
        task_info
    }
    
    /// 运行 Agent 任务
    pub async fn run_agent_task(&self, task_info: Arc<TaskInfo>) -> Result<()> {
        // 更新状态为 Running
        self.update_status(&task_info.agent_id, TaskStatus::Running);
        
        // 发送进度通知
        self.send_progress(TaskProgress::Started {
            agent_id: task_info.agent_id.clone(),
        });
        
        // ... 实际执行逻辑
        
        // 更新状态为 Completed
        self.update_status(&task_info.agent_id, TaskStatus::Completed);
        
        Ok(())
    }
    
    /// 失败 Agent 任务
    pub fn fail_agent(&self, agent_id: &str, err: anyhow::Error) {
        self.update_status(agent_id, TaskStatus::Failed);
        
        if let Some(task) = self.tasks.read().get(agent_id) {
            let mut task = Arc::make_mut(task);
            task.error = Some(err.to_string());
            task.end_time = Some(Instant::now());
        }
        
        self.send_progress(TaskProgress::Failed {
            agent_id: agent_id.to_string(),
            error: err.to_string(),
        });
    }
    
    /// 更新状态
    pub fn update_status(&self, agent_id: &str, status: TaskStatus) {
        if let Some(task) = self.tasks.read().get(agent_id) {
            let mut task = Arc::make_mut(task);
            task.status = status;
            
            if matches!(status, TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Cancelled) {
                task.end_time = Some(Instant::now());
            }
        }
    }
    
    /// 发送进度
    pub fn send_progress(&self, progress: TaskProgress) {
        let _ = self.progress_tx.send(progress);
    }
    
    /// 获取输出路径
    pub fn get_output_path(&self, agent_id: &str) -> PathBuf {
        self.output_dir.join(format!("{}.jsonl", agent_id))
    }
}
```

---

## 3. 关键设计要点

### 3.1 同步 vs 异步决策

| 条件 | 执行模式 |
|------|----------|
| `run_in_background=true` | 异步 |
| prompt > 1000 字符 | 异步 |
| `subagent_type=explore` | 异步 |
| `permission_mode=plan` | 同步（但只生成计划） |
| 其他情况 | 同步 |

### 3.2 隔离模式

| 模式 | 描述 | 实现 |
|------|------|------|
| `worktree` | 创建临时 git worktree | `git worktree add` |
| `remote` | 远程 CCR 环境 | 总是后台运行 |
| `cwd` | 覆盖工作目录 | 文件系统隔离 |

### 3.3 进度类型

```typescript
type AgentProgress =
  | { type: 'agent_progress'; description: string; currentTask: string }
  | { type: 'agent_token_usage'; tokens: number }
  | { type: 'bash_progress'; command: string; output: string }
```

---

## 4. 总结

Claude Code AgentTool 核心特点：

1. **灵活的执行模式** - 同步/异步自动切换
2. **多代理支持** - name, team_name, mode
3. **隔离模式** - worktree, remote, cwd
4. **进度追踪** - 实时转发子代理进度
5. **内置 Agent** - 预定义专用代理类型

Yode 可以借鉴：
- 同步/异步自动决策
- 工作树隔离集成
- 进度追踪通道
- 内置专用 Agent 定义
