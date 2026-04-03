# AgentTool 深度分析与优化

## 1. AgentTool 架构概述

Claude Code 的 AgentTool (`src/tools/AgentTool/AgentTool.tsx`) 是一个 800+ 行的复杂工具，支持同步/异步 Agent 执行、子 Agent 类型、Worktree 隔离、远程 Agent 等功能。

### 1.1 工具输入输出 Schema

```typescript
// 基础输入 Schema（无多 Agent 参数）
const baseInputSchema = z.object({
  description: z.string().describe('A short (3-5 word) description of the task'),
  prompt: z.string().describe('The task for the agent to perform'),
  subagent_type: z.string().optional().describe('The type of specialized agent to use'),
  model: z.enum(['sonnet', 'opus', 'haiku']).optional()
    .describe('Optional model override for this agent'),
  run_in_background: z.boolean().optional()
    .describe('Set to true to run this agent in background'),
})

// 多 Agent 参数扩展
const multiAgentInputSchema = z.object({
  name: z.string().optional()
    .describe('Name for spawned agent. Makes it addressable via SendMessage'),
  team_name: z.string().optional()
    .describe('Team name for spawning'),
  mode: permissionModeSchema().optional()
    .describe('Permission mode for spawned teammate'),
})

// 完整输入 Schema
const fullInputSchema = baseInputSchema.merge(multiAgentInputSchema).extend({
  isolation: z.enum(['worktree', 'remote']).optional()
    .describe('Isolation mode'),
  cwd: z.string().optional()
    .describe('Absolute path to run the agent in'),
})

// 输出 Schema - 同步/异步区分
const outputSchema = z.union([
  // 同步输出
  agentToolResultSchema().extend({
    status: z.literal('completed'),
    prompt: z.string(),
  }),
  // 异步输出
  z.object({
    status: z.literal('async_launched'),
    agentId: z.string(),
    description: z.string(),
    prompt: z.string(),
    outputFile: z.string(),
    canReadOutputFile: z.boolean().optional(),
  }),
  // 远程启动输出（内部类型）
  z.object({
    status: z.literal('remote_launched'),
    taskId: z.string(),
    sessionUrl: z.string(),
    description: z.string(),
    prompt: z.string(),
    outputFile: z.string(),
  }),
])
```

### 1.2 内置 Agent 类型

```typescript
// src/tools/AgentTool/builtInAgents.ts

// 内置 Agent 类型常量
const ONE_SHOT_BUILTIN_AGENT_TYPES = new Set([
  'general-purpose',    // 通用 Agent
  'explore',            // 探索 Agent
  'plan',               // 计划 Agent
  'code-reviewer',      // 代码审查 Agent
  'statusline-setup',   // 状态行设置 Agent
  'verification',       // 验证 Agent
])

// Agent 定义
interface AgentDefinition {
  name: string
  description: string
  prompt: string
  model?: 'sonnet' | 'opus' | 'haiku'
  subagent_type?: string
  mcpServers?: string[]  // 需要的 MCP 服务器
}

// 内置 Agent 列表
const BUILTIN_AGENTS: AgentDefinition[] = [
  {
    name: 'general-purpose',
    description: 'General-purpose agent for complex tasks',
    prompt: 'You are a general-purpose assistant...',
    model: 'opus',
  },
  {
    name: 'explore',
    description: 'Fast agent for exploring codebases',
    prompt: 'You are a code exploration specialist...',
    model: 'haiku',
  },
  {
    name: 'plan',
    description: 'Software architect agent for planning',
    prompt: 'You are a software architect...',
    model: 'opus',
  },
  {
    name: 'code-reviewer',
    description: 'Reviews code for bugs and standards',
    prompt: 'You are a code reviewer...',
    model: 'sonnet',
  },
]
```

### 1.3 进度追踪常量

```typescript
// 进度显示常量
const PROGRESS_THRESHOLD_MS = 2000  // 2 秒后显示后台提示

// 自动后台时间（毫秒）
function getAutoBackgroundMs(): number {
  if (isEnvTruthy(process.env.CLAUDE_AUTO_BACKGROUND_TASKS) || 
      getFeatureValue_CACHED_MAY_BE_STALE('tengu_auto_background_agents', false)) {
    return 120_000  // 120 秒后自动后台
  }
  return 0  // 禁用
}

// 后台任务禁用检查
const isBackgroundTasksDisabled =
  isEnvTruthy(process.env.CLAUDE_CODE_DISABLE_BACKGROUND_TASKS)
```

---

## 2. Agent 执行流程

### 2.1 同步 Agent 执行

```typescript
// src/tools/AgentTool/runAgent.ts

/**
 * 运行同步 Agent
 */
async function runSyncAgent(
  input: AgentToolInput,
  toolUseContext: ToolUseContext,
  canUseTool: CanUseToolFn,
): Promise<Output> {
  const { prompt, subagent_type, model, description } = input
  
  // 1. 获取 Agent 定义
  const agentDef = await getAgentDefinition(subagent_type || 'general-purpose')
  
  // 2. 构建系统提示
  const systemPrompt = await buildAgentSystemPrompt(
    agentDef,
    toolUseContext,
  )
  
  // 3. 准备消息历史
  const messages = await buildAgentMessages(
    toolUseContext.getAppState().messages,
    prompt,
  )
  
  // 4. 获取模型
  const agentModel = getAgentModel(model || agentDef.model)
  
  // 5. 运行 Agent 主循环
  const result = await runAgentLoop({
    model: agentModel,
    systemPrompt,
    messages,
    tools: await getAgentTools(toolUseContext),
    onProgress: (progress) => {
      // 转发进度
      onProgress?.({
        type: 'agent_progress',
        description: progress.description,
        currentTask: progress.currentTask,
      })
    },
  })
  
  // 6. 返回结果
  return {
    status: 'completed',
    content: result.content,
    prompt,
  }
}
```

### 2.2 异步 Agent 执行

```typescript
// src/tools/AgentTool/AgentTool.tsx

/**
 * 运行异步 Agent
 */
async function runAsyncAgent(
  input: AgentToolInput,
  toolUseContext: ToolUseContext,
): Promise<Output> {
  const { prompt, description, subagent_type, name, team_name } = input
  
  // 1. 创建 Agent ID
  const agentId = createAgentId()
  
  // 2. 注册异步 Agent
  registerAsyncAgent(agentId, {
    description,
    prompt,
    subagent_type: subagent_type || 'general-purpose',
    name,
    team_name,
  })
  
  // 3. 启动后台任务
  const taskPromise = runWithAgentContext(agentId, async () => {
    // 运行 Agent 循环
    const result = await runAgentLoop({ ... })
    
    // 完成后更新状态
    completeAgentTask(agentId, result)
  })
  
  // 4. 如果启用自动后台，设置超时
  const autoBgMs = getAutoBackgroundMs()
  if (autoBgMs > 0) {
    setTimeout(() => {
      // 超过阈值，自动后台化
      if (!taskCompleted(agentId)) {
        // 显示后台提示
        emitAgentNotification({
          type: 'auto_background',
          agentId,
          message: `Agent "${description}" is taking longer than expected. Running in background.`,
        })
      }
    }, autoBgMs)
  }
  
  // 5. 返回异步启动结果
  return {
    status: 'async_launched',
    agentId,
    description,
    prompt,
    outputFile: getTaskOutputPath(agentId),
    canReadOutputFile: canUseTool('TaskOutput') || canUseTool('Bash'),
  }
}
```

### 2.3 Agent 生命周期

```typescript
// src/tools/AgentTool/agentToolUtils.ts

/**
 * 运行异步 Agent 生命周期
 */
export async function runAsyncAgentLifecycle(
  agentId: AgentId,
  input: AgentToolInput,
  context: ToolUseContext,
): Promise<void> {
  // ========== 启动阶段 ==========
  emitAgentEvent({
    type: 'agent_started',
    agentId,
    timestamp: Date.now(),
  })
  
  try {
    // ========== 执行阶段 ==========
    const result = await executeAgent(agentId, input, context)
    
    // ========== 完成阶段 ==========
    emitAgentEvent({
      type: 'agent_completed',
      agentId,
      timestamp: Date.now(),
      result,
    })
    
    // 更新 Agent 元数据
    await writeAgentMetadata(agentId, {
      status: 'completed',
      completedAt: new Date().toISOString(),
      result,
    })
    
  } catch (error) {
    // ========== 失败阶段 ==========
    emitAgentEvent({
      type: 'agent_failed',
      agentId,
      timestamp: Date.now(),
      error: errorMessage(error),
    })
    
    failAgentTask(agentId, error)
  } finally {
    // ========== 清理阶段 ==========
    cleanupAgent(agentId)
  }
}
```

---

## 3. Worktree 隔离

### 3.1 Worktree 创建

```typescript
// src/utils/worktree.ts

/**
 * 为 Agent 创建 Worktree
 */
export async function createAgentWorktree(
  agentId: AgentId,
): Promise<string> {
  const projectRoot = getProjectRoot()
  const worktreePath = path.join(
    projectRoot,
    '.claude',
    'worktrees',
    agentId,
  )
  
  // 检查是否已存在
  const exists = await pathExists(worktreePath)
  if (exists) {
    // 清理现有 worktree
    await removeAgentWorktree(agentId)
  }
  
  // 创建 worktree
  const branchName = `agent/${agentId}`
  await execFileNoThrow(gitExe(), [
    'worktree',
    'add',
    worktreePath,
    '-b',
    branchName,
  ])
  
  return worktreePath
}

/**
 * 移除 Agent Worktree
 */
export async function removeAgentWorktree(
  agentId: AgentId,
): Promise<void> {
  const worktreePath = path.join(
    projectRoot,
    '.claude',
    'worktrees',
    agentId,
  )
  
  // 检查是否存在
  const exists = await pathExists(worktreePath)
  if (!exists) {
    return
  }
  
  // 移除 worktree
  await execFileNoThrow(gitExe(), [
    'worktree',
    'remove',
    worktreePath,
    '--force',
  ])
}

/**
 * 检查 Worktree 是否有变更
 */
export async function hasWorktreeChanges(
  worktreePath: string,
): Promise<boolean> {
  const result = await execFileNoThrow(gitExe(), [
    'status',
    '--porcelain',
  ], {
    cwd: worktreePath,
  })
  
  return result.stdout.trim().length > 0
}
```

### 3.2 Worktree 模式检查

```typescript
// src/utils/worktreeModeEnabled.ts

/**
 * 检查 Worktree 模式是否启用
 */
export function isWorktreeModeEnabled(): boolean {
  return isEnvTruthy(process.env.CLAUDE_CODE_WORKTREE_MODE)
}

/**
 * 获取 Worktree 配置
 */
export function getWorktreeConfig(): WorktreeConfig {
  return {
    enabled: isWorktreeModeEnabled(),
    basePath: path.join(getProjectRoot(), '.claude', 'worktrees'),
    autoRemove: isEnvTruthy(process.env.CLAUDE_CODE_WORKTREE_AUTO_REMOVE),
  }
}
```

---

## 4. 远程 Agent

### 4.1 远程 Agent 资格检查

```typescript
// src/tasks/RemoteAgentTask/RemoteAgentTask.ts

/**
 * 检查远程 Agent 资格
 */
export async function checkRemoteAgentEligibility(
  input: AgentToolInput,
): Promise<{ eligible: boolean; reason?: string }> {
  // 1. 检查是否启用远程 Agent
  if (!feature('REMOTE_AGENT')) {
    return { eligible: false, reason: 'Remote agent feature not enabled' }
  }
  
  // 2. 检查隔离模式
  if (input.isolation !== 'remote') {
    return { eligible: false, reason: 'Isolation mode is not "remote"' }
  }
  
  // 3. 检查 CCR 可用性
  const ccrAvailable = await checkCCRAvailability()
  if (!ccrAvailable) {
    return { eligible: false, reason: 'CCR environment not available' }
  }
  
  // 4. 检查资源配额
  const quotaAvailable = await checkResourceQuota()
  if (!quotaAvailable) {
    return { eligible: false, reason: 'Resource quota exceeded' }
  }
  
  return { eligible: true }
}

/**
 * 注册远程 Agent 任务
 */
export async function registerRemoteAgentTask(
  input: AgentToolInput,
): Promise<{ taskId: string; sessionUrl: string }> {
  // 1. 调用远程 API 注册任务
  const response = await fetch(
    'https://api.claude.ai/v1/code/triggers',
    {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'Authorization': `Bearer ${getAuthToken()}`,
      },
      body: JSON.stringify({
        prompt: input.prompt,
        description: input.description,
        subagent_type: input.subagent_type,
      }),
    },
  )
  
  const data = await response.json()
  
  // 2. 返回任务信息
  return {
    taskId: data.trigger_id,
    sessionUrl: getRemoteTaskSessionUrl(data.trigger_id),
  }
}
```

### 4.2 远程任务 URL 生成

```typescript
/**
 * 获取远程任务会话 URL
 */
export function getRemoteTaskSessionUrl(
  triggerId: string,
): string {
  const baseUrl = process.env.CLAUDE_CODE_REMOTE_URL || 
                  'https://claude.ai/code'
  return `${baseUrl}/sessions/${triggerId}`
}
```

---

## 5. Agent 工具池组装

### 5.1 Agent 可用工具过滤

```typescript
// src/tools/AgentTool/loadAgentsDir.ts

/**
 * 根据 MCP 需求过滤 Agent
 */
export function filterAgentsByMcpRequirements(
  agents: AgentDefinition[],
  availableMcpServers: string[],
): AgentDefinition[] {
  return agents.filter(agent => {
    if (!agent.mcpServers) {
      return true  // 无 MCP 需求
    }
    
    // 检查所有需要的 MCP 服务器是否可用
    return agent.mcpServers.every(server =>
      availableMcpServers.includes(server),
    )
  })
}

/**
 * 检查是否有需要的 MCP 服务器
 */
export function hasRequiredMcpServers(
  agent: AgentDefinition,
  availableMcpServers: string[],
): boolean {
  if (!agent.mcpServers) {
    return true
  }
  
  return agent.mcpServers.every(server =>
    availableMcpServers.includes(server),
  )
}
```

### 5.2 权限规则过滤

```typescript
// src/utils/permissions/permissions.ts

/**
 * 过滤被拒绝的 Agent
 */
export function filterDeniedAgents<T extends { agentType: string }>(
  agents: T[],
  context: ToolPermissionContext,
  agentToolName: string,
): T[] {
  // 解析拒绝规则一次，收集 Agent(x) 内容到 Set
  const deniedAgentTypes = new Set<string>()
  for (const rule of getDenyRules(context)) {
    if (
      rule.ruleValue.toolName === agentToolName &&
      rule.ruleValue.ruleContent !== undefined
    ) {
      deniedAgentTypes.add(rule.ruleValue.ruleContent)
    }
  }
  
  return agents.filter(agent => 
    !deniedAgentTypes.has(agent.agentType)
  )
}
```

---

## 6. Agent 进度追踪

### 6.1 进度跟踪器

```typescript
// src/tasks/LocalAgentTask/LocalAgentTask.ts

/**
 * 创建进度跟踪器
 */
export function createProgressTracker(
  agentId: AgentId,
): ProgressTracker {
  return {
    updates: [],
    
    // 更新进度
    update(description: string, currentTask: string) {
      this.updates.push({
        agentId,
        description,
        currentTask,
        timestamp: Date.now(),
      })
      
      // 发出进度事件
      emitAgentProgress({
        agentId,
        description,
        currentTask,
      })
    },
    
    // 获取进度摘要
    getSummary(): string {
      return this.updates
        .map(u => `[${new Date(u.timestamp).toISOString()}] ${u.description}`)
        .join('\n')
    },
  }
}
```

### 6.2 进度转发

```typescript
// AgentTool 转发子 Agent 进度
export function forwardAgentProgress(
  parentAgentId: AgentId,
  childAgentId: AgentId,
  progress: AgentToolProgress,
): void {
  // 转发 Agent 进度
  onProgress?.({
    type: 'agent_progress',
    description: `[${childAgentId}] ${progress.description}`,
    currentTask: progress.currentTask,
  })
  
  // 也转发 Shell 进度
  if (progress.type === 'shell_progress') {
    onProgress?.({
      type: 'shell_progress',
      command: progress.command,
      output: progress.output,
      isRunning: progress.isRunning,
    })
  }
}
```

---

## 7. Agent 元数据

### 7.1 Agent 元数据写入

```typescript
// src/utils/sessionStorage.ts

interface AgentMetadata {
  agentId: string
  status: 'running' | 'completed' | 'failed'
  description: string
  prompt: string
  subagent_type?: string
  model?: string
  startedAt: string
  completedAt?: string
  result?: unknown
  error?: string
}

/**
 * 写入 Agent 元数据
 */
export async function writeAgentMetadata(
  agentId: string,
  metadata: Partial<AgentMetadata>,
): Promise<void> {
  const metadataPath = path.join(
    getSessionDir(),
    'agents',
    `${agentId}.json`,
  )
  
  // 确保目录存在
  await fs.mkdir(path.dirname(metadataPath), { recursive: true })
  
  // 写入元数据
  await fs.writeFile(
    metadataPath,
    JSON.stringify(metadata, null, 2),
    'utf-8',
  )
}

/**
 * 读取 Agent 元数据
 */
export async function readAgentMetadata(
  agentId: string,
): Promise<AgentMetadata | null> {
  const metadataPath = path.join(
    getSessionDir(),
    'agents',
    `${agentId}.json`,
  )
  
  try {
    const content = await fs.readFile(metadataPath, 'utf-8')
    return JSON.parse(content)
  } catch {
    return null
  }
}
```

---

## 8. Yode Agent 工具优化建议

### 8.1 第一阶段：Agent 类型定义

```rust
// crates/yode-tools/src/builtin/agent/types.rs

/// Agent 类型
#[derive(Debug, Clone, PartialEq)]
pub enum AgentType {
    GeneralPurpose,
    Explore,
    Plan,
    CodeReviewer,
    Custom(String),
}

/// Agent 定义
#[derive(Debug, Clone)]
pub struct AgentDefinition {
    pub name: String,
    pub description: String,
    pub prompt: String,
    pub model: Option<String>,
    pub mcp_servers: Vec<String>,
}

/// Agent 输入
#[derive(Debug, Clone)]
pub struct AgentInput {
    pub description: String,
    pub prompt: String,
    pub subagent_type: Option<AgentType>,
    pub model: Option<String>,
    pub run_in_background: bool,
    pub name: Option<String>,
    pub isolation: Option<IsolationMode>,
    pub cwd: Option<String>,
}

/// 隔离模式
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum IsolationMode {
    Worktree,
    Remote,
}

/// Agent 输出
#[derive(Debug, Clone)]
pub enum AgentOutput {
    Completed {
        content: String,
        prompt: String,
    },
    AsyncLaunched {
        agent_id: String,
        description: String,
        prompt: String,
        output_file: String,
    },
    RemoteLaunched {
        task_id: String,
        session_url: String,
        description: String,
        prompt: String,
    },
}
```

### 8.2 第二阶段：Agent 注册表

```rust
// crates/yode-tools/src/builtin/agent/registry.rs

use std::collections::HashMap;

/// Agent 注册表
pub struct AgentRegistry {
    builtin_agents: HashMap<String, AgentDefinition>,
    custom_agents: HashMap<String, AgentDefinition>,
}

impl AgentRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            builtin_agents: HashMap::new(),
            custom_agents: HashMap::new(),
        };
        
        // 注册内置 Agent
        registry.register_builtin(AgentDefinition {
            name: "general-purpose".to_string(),
            description: "General-purpose agent for complex tasks".to_string(),
            prompt: "You are a general-purpose assistant...".to_string(),
            model: Some("opus".to_string()),
            mcp_servers: vec![],
        });
        
        registry.register_builtin(AgentDefinition {
            name: "explore".to_string(),
            description: "Fast agent for exploring codebases".to_string(),
            prompt: "You are a code exploration specialist...".to_string(),
            model: Some("haiku".to_string()),
            mcp_servers: vec![],
        });
        
        registry
    }
    
    /// 获取 Agent 定义
    pub fn get_agent(&self, name: &str) -> Option<&AgentDefinition> {
        self.builtin_agents.get(name)
            .or_else(|| self.custom_agents.get(name))
    }
    
    /// 过滤 Agent（按 MCP 需求）
    pub fn filter_by_mcp_requirements(
        &self,
        available_servers: &[String],
    ) -> Vec<&AgentDefinition> {
        self.builtin_agents.values()
            .chain(self.custom_agents.values())
            .filter(|agent| {
                agent.mcp_servers.is_empty() ||
                agent.mcp_servers.iter()
                    .all(|s| available_servers.contains(s))
            })
            .collect()
    }
}
```

### 8.3 第三阶段：Agent 执行器

```rust
// crates/yode-tools/src/builtin/agent/executor.rs

/// Agent 执行器
pub struct AgentExecutor {
    registry: Arc<AgentRegistry>,
    worktree_manager: Arc<WorktreeManager>,
}

impl AgentExecutor {
    pub fn new(
        registry: Arc<AgentRegistry>,
        worktree_manager: Arc<WorktreeManager>,
    ) -> Self {
        Self {
            registry,
            worktree_manager,
        }
    }
    
    /// 执行 Agent
    pub async fn execute(
        &self,
        input: AgentInput,
        context: &AgentContext,
    ) -> Result<AgentOutput> {
        // 1. 获取 Agent 定义
        let agent_type = input.subagent_type
            .unwrap_or(AgentType::GeneralPurpose);
        
        let agent_def = self.registry
            .get_agent(&agent_type.to_string())
            .ok_or_else(|| anyhow!("Unknown agent type: {:?}", agent_type))?;
        
        // 2. 检查是否需要后台运行
        if input.run_in_background {
            self.execute_async(input, agent_def, context).await
        } else {
            self.execute_sync(input, agent_def, context).await
        }
    }
    
    /// 同步执行
    async fn execute_sync(
        &self,
        input: AgentInput,
        agent_def: &AgentDefinition,
        context: &AgentContext,
    ) -> Result<AgentOutput> {
        // 1. 创建工作目录（如果需要隔离）
        let work_dir = if let Some(IsolationMode::Worktree) = input.isolation {
            Some(self.worktree_manager.create_worktree().await?)
        } else {
            None
        };
        
        // 2. 构建系统提示
        let system_prompt = self.build_system_prompt(agent_def, context)?;
        
        // 3. 运行 Agent 循环
        let result = self.run_agent_loop(
            &system_prompt,
            &input.prompt,
            input.model.as_deref(),
            work_dir.as_ref(),
        ).await?;
        
        Ok(AgentOutput::Completed {
            content: result.content,
            prompt: input.prompt,
        })
    }
    
    /// 异步执行
    async fn execute_async(
        &self,
        input: AgentInput,
        agent_def: &AgentDefinition,
        context: &AgentContext,
    ) -> Result<AgentOutput> {
        let agent_id = generate_agent_id();
        
        // 启动后台任务
        let executor = self.clone();
        tokio::spawn(async move {
            let _ = executor.execute_sync(input, agent_def, context).await;
        });
        
        Ok(AgentOutput::AsyncLaunched {
            agent_id,
            description: input.description,
            prompt: input.prompt,
            output_file: format!("/tmp/agent_{}.log", agent_id),
        })
    }
}
```

### 8.4 第四阶段：Worktree 管理器

```rust
// crates/yode-tools/src/builtin/agent/worktree.rs

use tokio::process::Command;
use std::path::PathBuf;

/// Worktree 管理器
pub struct WorktreeManager {
    base_path: PathBuf,
}

impl WorktreeManager {
    pub fn new(project_root: &Path) -> Self {
        Self {
            base_path: project_root.join(".claude").join("worktrees"),
        }
    }
    
    /// 创建 Worktree
    pub async fn create_worktree(&self) -> Result<PathBuf> {
        let agent_id = generate_agent_id();
        let worktree_path = self.base_path.join(&agent_id);
        let branch_name = format!("agent/{}", agent_id);
        
        // 确保目录存在
        tokio::fs::create_dir_all(&self.base_path).await?;
        
        // 创建 worktree
        let output = Command::new("git")
            .args(&["worktree", "add", &worktree_path.to_string_lossy(), "-b", &branch_name])
            .output()
            .await?;
        
        if !output.status.success() {
            return Err(anyhow!("Failed to create worktree: {}", 
                String::from_utf8_lossy(&output.stderr)));
        }
        
        Ok(worktree_path)
    }
    
    /// 移除 Worktree
    pub async fn remove_worktree(&self, agent_id: &str) -> Result<()> {
        let worktree_path = self.base_path.join(agent_id);
        
        if !worktree_path.exists() {
            return Ok(());
        }
        
        let output = Command::new("git")
            .args(&["worktree", "remove", &worktree_path.to_string_lossy(), "--force"])
            .output()
            .await?;
        
        if !output.status.success() {
            return Err(anyhow!("Failed to remove worktree: {}", 
                String::from_utf8_lossy(&output.stderr)));
        }
        
        Ok(())
    }
    
    /// 检查变更
    pub async fn has_changes(&self, worktree_path: &Path) -> Result<bool> {
        let output = Command::new("git")
            .args(&["status", "--porcelain"])
            .current_dir(worktree_path)
            .output()
            .await?;
        
        Ok(!output.stdout.is_empty())
    }
}
```

---

## 9. 配置文件示例

```toml
# ~/.config/yode/agents.toml

# Agent 配置
[agents]
# 默认模型
default_model = "sonnet"

# 后台运行配置
[agents.background]
enabled = true
auto_background_after_seconds = 120

# Worktree 配置
[agents.worktree]
enabled = true
auto_remove = true
base_path = "~/.config/yode/worktrees"

# 远程 Agent 配置
[agents.remote]
enabled = false
endpoint = "https://api.claude.ai/v1/code/triggers"

# 内置 Agent 配置
[agents.builtin]
# 探索 Agent
[agents.builtin.explore]
model = "haiku"
max_tool_calls = 50

# 计划 Agent
[agents.builtin.plan]
model = "opus"
require_explicit_approval = true

# 代码审查 Agent
[agents.builtin.code_reviewer]
model = "sonnet"
check_style = true
check_security = true

# 自定义 Agent
[[agents.custom]]
name = "data-analyst"
description = "Agent specialized in data analysis"
prompt = "You are a data analysis expert..."
model = "opus"
mcp_servers = ["pandas", "matplotlib"]

[[agents.custom]]
name = "frontend-designer"
description = "Frontend UI/UX specialist"
prompt = "You are a frontend design expert..."
model = "sonnet"
```

---

## 10. 总结

Claude Code AgentTool 的核心特点：

1. **多 Agent 类型** - 内置 6+ 种专用 Agent
2. **同步/异步执行** - 灵活的任务执行模式
3. **Worktree 隔离** - Git Worktree 提供文件隔离
4. **远程 Agent** - CCR 远程环境执行
5. **进度追踪** - 实时进度更新和转发
6. **MCP 需求过滤** - 根据 MCP 服务器可用性过滤
7. **权限规则过滤** - Agent 类型级别的权限控制
8. **自动后台** - 超时自动后台化
9. **Agent 元数据** - 完整的执行记录
10. **生命周期管理** - 启动/执行/完成/失败/清理

Yode 优化优先级：
1. Agent 类型定义与注册表
2. 同步/异步执行器
3. Worktree 管理器
4. 进度追踪框架
5. 远程 Agent 支持
6. Agent 元数据存储
