# 工具系统设计深度分析与优化建议

## 1. Claude Code 工具系统架构

### 1.1 工具类型定义

Claude Code 的工具定义位于 `src/Tool.ts`，核心类型：

```typescript
// src/Tool.ts

export type ToolInputJSONSchema = {
  [x: string]: unknown
  type: 'object'
  properties?: {
    [x: string]: unknown
  }
}

export type ToolUseContext = {
  options: {
    commands: Command[]
    debug: boolean
    mainLoopModel: string
    tools: Tools
    verbose: boolean
    mcpClients: MCPServerConnection[]
    mcpResources: Record<string, ServerResource[]>
    maxBudgetUsd?: number
    customSystemPrompt?: string
  }
  abortController: AbortController
  getAppState(): AppState
  setAppState(f: (prev: AppState) => AppState): void
  // ... 更多上下文
}

// 工具执行结果
export type ToolResult = {
  toolUseId: string
  result: {
    content: Array<{
      type: string
      text?: string
      image?: {
        type: 'base64'
        mediaType: string
        data: string
      }
    }>
    is_error?: boolean
  }
}
```

### 1.2 工具目录结构

Claude Code 有 30+ 个内置工具：

```
src/tools/
├── AgentTool/           # 子代理工具
├── AskUserQuestionTool/ # 询问用户
├── BashTool/           # Bash 命令
├── BriefTool/          # 生成简报
├── ConfigTool/         # 配置管理
├── DiscoverSkillsTool/ # 技能发现
├── EnterPlanModeTool/  # 进入计划模式
├── EnterWorktreeTool/  # 进入工作树
├── ExitPlanModeTool/   # 退出计划模式
├── FileEditTool/       # 文件编辑
├── FileReadTool/       # 文件读取
├── FileWriteTool/      # 文件写入
├── GlobTool/           # 文件搜索
├── GrepTool/           # 内容搜索
├── LSPTool/            # LSP 集成
├── MCPTool/            # MCP 工具
├── NotebookEditTool/   # Notebook 编辑
├── PowerShellTool/     # PowerShell
├── REPLTool/           # REPL
├── ReviewArtifactTool/ # 审查工件
├── ScheduleCronTool/   # 定时任务
├── SendMessageTool/    # 发送消息
└── ... (更多)
```

### 1.3 工具注册与管理

```typescript
// src/tools/registry.ts (简化版)

class ToolRegistry {
  private tools: Map<string, Tool> = new Map();
  private deferredTools: Map<string, Tool> = new Map();
  private toolSearchEnabled: boolean = false;
  
  register(tool: Tool): void {
    this.tools.set(tool.name, tool);
  }
  
  registerDeferred(tool: Tool): void {
    this.deferredTools.set(tool.name, tool);
  }
  
  activate(name: string): boolean {
    const tool = this.deferredTools.get(name);
    if (tool) {
      this.deferredTools.delete(name);
      this.tools.set(name, tool);
      return true;
    }
    return false;
  }
  
  get(name: string): Tool | undefined {
    return this.tools.get(name) || this.deferredTools.get(name);
  }
  
  definitions(): ToolDefinition[] {
    return Array.from(this.tools.values()).map(tool => ({
      name: tool.name,
      description: tool.description,
      inputSchema: tool.inputSchema,
    }));
  }
}
```

### 1.4 工具执行流程

```typescript
// src/query.ts - 工具执行核心流程

async function executeToolCall(
  toolCall: ToolUseBlockParam,
  context: ToolUseContext,
): Promise<ToolResult> {
  const tool = getTool(toolCall.name);
  
  if (!tool) {
    return {
      toolUseId: toolCall.id,
      result: {
        content: [{ type: 'text', text: `Unknown tool: ${toolCall.name}` }],
        is_error: true,
      },
    };
  }
  
  // 1. 权限检查
  const permissionResult = await checkPermission(toolCall.name, toolCall.input, context);
  
  if (permissionResult.behavior === 'deny') {
    return {
      toolUseId: toolCall.id,
      result: {
        content: [{ type: 'text', text: 'Tool execution denied' }],
        is_error: true,
      },
    };
  }
  
  if (permissionResult.behavior === 'ask') {
    // 需要用户确认
    const userResponse = await showPermissionDialog(toolCall, permissionResult);
    if (!userResponse.allowed) {
      return { toolUseId: toolCall.id, result: { content: [], is_error: false } };
    }
  }
  
  // 2. 执行工具
  try {
    const result = await tool.execute(toolCall.input, context);
    return {
      toolUseId: toolCall.id,
      result,
    };
  } catch (error) {
    return {
      toolUseId: toolCall.id,
      result: {
        content: [{ type: 'text', text: `Error: ${error.message}` }],
        is_error: true,
      },
    };
  }
}
```

### 1.5 工具进度追踪

```typescript
// src/types/tools.ts

// Bash 工具进度
export type BashProgress = {
  type: 'bash_progress'
  command: string
  output: string
  isRunning: boolean
}

// Agent 工具进度
export type AgentToolProgress = {
  type: 'agent_progress'
  description: string
  currentTask: string
}

// Web 搜索进度
export type WebSearchProgress = {
  type: 'web_search_progress'
  query: string
  resultsFound: number
}

// 通用进度类型
export type ToolProgressData =
  | BashProgress
  | AgentToolProgress
  | WebSearchProgress
  | MCPProgress
  | SkillToolProgress
  | TaskOutputProgress
  | REPLToolProgress;
```

---

## 2. Yode 当前工具系统分析

### 2.1 当前工具列表

Yode 已有 32 个内置工具：

```
crates/yode-tools/src/builtin/
├── agent.rs          # 子代理
├── ask_user.rs       # 询问用户
├── bash.rs           # Bash 命令
├── batch.rs          # 批量操作
├── cron.rs           # 定时任务
├── edit_file.rs      # 编辑文件
├── file_diff.rs      # 文件差异
├── git_commit.rs     # Git 提交
├── git_diff.rs       # Git 差异
├── git_log.rs        # Git 日志
├── git_status.rs     # Git 状态
├── glob.rs           # 文件搜索
├── grep.rs           # 内容搜索
├── hypothesis.rs     # 假设工具
├── ls.rs             # 目录列表
├── lsp.rs            # LSP 集成
├── mcp_resources.rs  # MCP 资源
├── memory.rs         # Memory
├── multi_edit.rs     # 多重编辑
├── notebook_edit.rs  # Notebook 编辑
├── plan_mode.rs      # 计划模式
├── project_map.rs    # 项目地图
├── read_file.rs      # 读取文件
├── skill.rs          # 技能
├── test_runner.rs    # 测试运行
├── todo.rs           # Todo 列表
├── tool_search.rs    # 工具搜索
├── web_fetch.rs      # Web 抓取
├── web_search.rs     # Web 搜索
├── worktree.rs       # 工作树
└── write_file.rs     # 写入文件
```

### 2.2 工具注册实现

Yode 的工具注册位于 `crates/yode-tools/src/registry.rs`：

```rust
pub struct ToolRegistry {
    /// 活跃工具（发送给 LLM）
    tools: HashMap<String, Arc<dyn Tool>>,
    /// 延迟工具（已知但不发送给 LLM）
    deferred: HashMap<String, Arc<dyn Tool>>,
    /// 是否启用工具搜索
    tool_search_enabled: bool,
}

impl ToolRegistry {
    pub fn register(&mut self, tool: Arc<dyn Tool>) {
        let name = tool.name().to_string();
        self.tools.insert(name, tool);
    }
    
    pub fn register_deferred(&mut self, tool: Arc<dyn Tool>) {
        let name = tool.name().to_string();
        self.deferred.insert(name, tool);
    }
    
    pub fn activate_tool(&mut self, name: &str) -> bool {
        if let Some(tool) = self.deferred.remove(name) {
            self.tools.insert(name.to_string(), tool);
            true
        } else {
            false
        }
    }
    
    pub fn definitions(&self) -> Vec<ToolDefinition> {
        self.tools
            .values()
            .map(|tool| ToolDefinition {
                name: tool.name().to_string(),
                description: tool.description().to_string(),
                parameters: tool.parameters_schema(),
            })
            .collect()
    }
}
```

### 2.3 工具 Trait 定义

```rust
// crates/yode-tools/src/tool.rs

#[async_trait]
pub trait Tool: Send + Sync {
    /// 工具名称（LLM 调用时使用）
    fn name(&self) -> &str;
    
    /// 工具描述（发送给 LLM）
    fn description(&self) -> &str;
    
    /// JSON Schema 参数定义
    fn parameters_schema(&self) -> Value;
    
    /// 执行工具
    async fn execute(
        &self,
        args: Value,
        context: &ToolContext,
    ) -> ToolResult;
}

pub struct ToolContext {
    pub working_dir: PathBuf,
    pub ask_user_tx: Option<Sender<UserQuery>>,
    pub ask_user_rx: Option<Receiver<String>>,
    // ...
}
```

---

## 3. 优化建议

### 3.1 第一阶段：工具进度追踪

#### 3.1.1 进度类型定义

```rust
// crates/yode-tools/src/tool.rs

/// 工具进度数据
#[derive(Debug, Clone)]
pub enum ToolProgress {
    /// Bash 命令输出
    BashOutput {
        command: String,
        output: String,
        is_running: bool,
    },
    /// 子代理进度
    AgentProgress {
        description: String,
        current_task: String,
    },
    /// Web 搜索进度
    WebSearchProgress {
        query: String,
        results_found: usize,
    },
    /// 文件读取进度
    FileReadProgress {
        path: String,
        lines_read: usize,
        total_lines: usize,
    },
    /// 通用进度消息
    Message {
        message: String,
    },
}

/// 工具执行状态
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ToolExecutionState {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

/// 工具执行跟踪
#[derive(Debug, Clone)]
pub struct ToolExecutionTrace {
    pub tool_name: String,
    pub args: serde_json::Value,
    pub state: ToolExecutionState,
    pub start_time: std::time::Instant,
    pub end_time: Option<std::time::Instant>,
    pub progress_updates: Vec<ToolProgress>,
}

impl ToolExecutionTrace {
    pub fn duration(&self) -> Option<std::time::Duration> {
        self.end_time.map(|end| end - self.start_time)
    }
}
```

#### 3.1.2 进度报告通道

```rust
// crates/yode-tools/src/tool.rs

use tokio::sync::mpsc;

pub struct ToolContext {
    pub working_dir: PathBuf,
    pub ask_user_tx: Option<Sender<UserQuery>>,
    pub ask_user_rx: Option<Receiver<String>>,
    /// 进度报告通道
    pub progress_tx: Option<Sender<ToolProgress>>,
    /// 取消令牌
    pub cancellation_token: Option<CancellationToken>,
}

/// 进度报告辅助宏
#[macro_export]
macro_rules! report_progress {
    ($ctx:expr, $progress:expr) => {
        if let Some(tx) = &$ctx.progress_tx {
            let _ = tx.send($progress);
        }
    };
}

/// 检查是否被取消
#[macro_export]
macro_rules! check_cancellation {
    ($ctx:expr) => {
        if let Some(token) = &$ctx.cancellation_token {
            if token.is_cancelled() {
                return ToolResult::cancelled();
            }
        }
    };
}
```

### 3.2 第二阶段：工具超时与重试

#### 3.2.1 超时配置

```rust
// crates/yode-tools/src/config.rs

use std::time::Duration;

/// 工具超时配置
#[derive(Debug, Clone)]
pub struct ToolTimeoutConfig {
    /// 默认超时（秒）
    pub default_timeout: Duration,
    /// Bash 工具超时
    pub bash_timeout: Duration,
    /// Web 工具超时
    pub web_timeout: Duration,
    /// LSP 工具超时
    pub lsp_timeout: Duration,
    /// 代理工具超时
    pub agent_timeout: Duration,
}

impl Default for ToolTimeoutConfig {
    fn default() -> Self {
        Self {
            default_timeout: Duration::from_secs(30),
            bash_timeout: Duration::from_secs(120),
            web_timeout: Duration::from_secs(30),
            lsp_timeout: Duration::from_secs(60),
            agent_timeout: Duration::from_secs(300),
        }
    }
}
```

#### 3.2.2 带超时的工具执行

```rust
// crates/yode-core/src/engine.rs

use tokio::time::{timeout, Duration};

impl AgentEngine {
    async fn execute_tool_call_with_timeout(
        &mut self,
        tool_call: &ToolCall,
    ) -> Result<ToolResult> {
        let tool = self.tools.get(&tool_call.name)
            .ok_or_else(|| anyhow::anyhow!("Unknown tool: {}", tool_call.name))?;
        
        // 获取超时配置
        let timeout_duration = self.get_tool_timeout(&tool_call.name);
        
        // 执行带超时的工具调用
        let args = serde_json::from_str::<serde_json::Value>(&tool_call.arguments)?;
        let ctx = self.create_tool_context();
        
        match timeout(timeout_duration, tool.execute(args, ctx)).await {
            Ok(result) => Ok(result),
            Err(_) => Ok(ToolResult {
                content: format!("工具 {} 执行超时（{} 秒）", tool_call.name, timeout_duration.as_secs()),
                is_error: true,
                error_type: Some(ToolErrorType::Timeout),
                suggestion: Some("尝试更小范围的操作或分批处理".to_string()),
            }),
        }
    }
    
    fn get_tool_timeout(&self, tool_name: &str) -> Duration {
        match tool_name {
            "bash" => self.tool_timeout_config.bash_timeout,
            "web_fetch" | "web_search" => self.tool_timeout_config.web_timeout,
            "lsp" => self.tool_timeout_config.lsp_timeout,
            "agent" => self.tool_timeout_config.agent_timeout,
            _ => self.tool_timeout_config.default_timeout,
        }
    }
}
```

### 3.3 第三阶段：工具预算控制

#### 3.3.1 预算追踪

```rust
// crates/yode-tools/src/budget.rs

use std::sync::atomic::{AtomicU32, Ordering};

/// 工具调用预算追踪器
pub struct ToolBudgetTracker {
    /// 总工具调用次数
    total_calls: AtomicU32,
    /// 每次对话的预算限制
    budget_per_turn: u32,
    /// 总预算限制
    total_budget: u32,
}

impl ToolBudgetTracker {
    pub fn new(budget_per_turn: u32, total_budget: u32) -> Self {
        Self {
            total_calls: AtomicU32::new(0),
            budget_per_turn,
            total_budget,
        }
    }
    
    /// 检查是否超过预算
    pub fn check_budget(&self) -> BudgetStatus {
        let calls = self.total_calls.load(Ordering::Relaxed);
        
        if calls >= self.total_budget {
            BudgetStatus::Exceeded {
                used: calls,
                limit: self.total_budget,
            }
        } else if calls >= self.budget_per_turn {
            BudgetStatus::TurnLimitReached {
                used: calls,
                limit: self.budget_per_turn,
            }
        } else if calls >= self.budget_per_turn / 2 {
            BudgetStatus::Warning {
                used: calls,
                limit: self.budget_per_turn,
            }
        } else {
            BudgetStatus::Ok {
                used: calls,
                limit: self.budget_per_turn,
            }
        }
    }
    
    /// 记录工具调用
    pub fn record_call(&self) {
        self.total_calls.fetch_add(1, Ordering::Relaxed);
    }
    
    /// 重置每轮计数
    pub fn reset_turn(&self) {
        self.total_calls.store(0, Ordering::Relaxed);
    }
}

#[derive(Debug, Clone)]
pub enum BudgetStatus {
    Ok { used: u32, limit: u32 },
    Warning { used: u32, limit: u32 },
    TurnLimitReached { used: u32, limit: u32 },
    Exceeded { used: u32, limit: u32 },
}
```

### 3.4 第四阶段：工具执行追踪与遥测

#### 3.4.1 执行追踪

```rust
// crates/yode-tools/src/telemetry.rs

use std::time::{Duration, Instant};
use serde::Serialize;

/// 工具执行遥测数据
#[derive(Debug, Clone, Serialize)]
pub struct ToolTelemetry {
    pub tool_name: String,
    pub tool_call_id: String,
    pub start_time: u64,  // Unix timestamp ms
    pub duration_ms: u64,
    pub success: bool,
    pub error_type: Option<String>,
    pub input_size_bytes: usize,
    pub output_size_bytes: usize,
}

/// 遥测收集器
pub struct TelemetryCollector {
    traces: parking_lot::Mutex<Vec<ToolTelemetry>>,
    session_id: String,
}

impl TelemetryCollector {
    pub fn new(session_id: String) -> Self {
        Self {
            traces: parking_lot::Mutex::new(Vec::new()),
            session_id,
        }
    }
    
    /// 记录工具执行
    pub fn record(&self, telemetry: ToolTelemetry) {
        self.traces.lock().push(telemetry);
    }
    
    /// 获取所有追踪
    pub fn get_traces(&self) -> Vec<ToolTelemetry> {
        self.traces.lock().clone()
    }
    
    /// 生成报告
    pub fn generate_report(&self) -> ToolTelemetryReport {
        let traces = self.get_traces();
        
        let total_calls = traces.len();
        let success_count = traces.iter().filter(|t| t.success).count();
        let failed_count = total_calls - success_count;
        let total_duration: Duration = traces
            .iter()
            .map(|t| Duration::from_millis(t.duration_ms))
            .sum();
        
        ToolTelemetryReport {
            session_id: self.session_id.clone(),
            total_calls,
            success_count,
            failed_count,
            success_rate: if total_calls > 0 {
                success_count as f64 / total_calls as f64
            } else {
                0.0
            },
            avg_duration_ms: if total_calls > 0 {
                total_duration.as_millis() as f64 / total_calls as f64
            } else {
                0.0
            },
            by_tool: self.group_by_tool(&traces),
        }
    }
    
    fn group_by_tool(&self, traces: &[ToolTelemetry]) -> Vec<ToolStats> {
        use std::collections::HashMap;
        
        let mut map: HashMap<String, Vec<&ToolTelemetry>> = HashMap::new();
        for trace in traces {
            map.entry(trace.tool_name.clone())
                .or_default()
                .push(trace);
        }
        
        map.into_iter()
            .map(|(name, tool_traces)| {
                let count = tool_traces.len();
                let success = tool_traces.iter().filter(|t| t.success).count();
                ToolStats {
                    name,
                    count,
                    success_count: success,
                    avg_duration_ms: tool_traces
                        .iter()
                        .map(|t| t.duration_ms)
                        .sum::<u64>() as f64 / count as f64,
                }
            })
            .collect()
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolTelemetryReport {
    pub session_id: String,
    pub total_calls: usize,
    pub success_count: usize,
    pub failed_count: usize,
    pub success_rate: f64,
    pub avg_duration_ms: f64,
    pub by_tool: Vec<ToolStats>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolStats {
    pub name: String,
    pub count: usize,
    pub success_count: usize,
    pub avg_duration_ms: f64,
}
```

---

## 4. 工具优化：Bash 工具

### 4.1 命令分类器集成

```rust
// crates/yode-tools/src/builtin/bash.rs

/// Bash 命令风险级别
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CommandRiskLevel {
    /// 安全命令（只读）
    Safe,
    /// 未知风险
    Unknown,
    /// 潜在风险
    PotentiallyRisky,
    /// 危险命令
    Dangerous,
    /// 破坏性命令
    Destructive,
}

/// Bash 命令分类器
pub struct BashCommandClassifier {
    /// 自定义规则
    custom_rules: Vec<ClassifierRule>,
}

struct ClassifierRule {
    pattern: regex::Regex,
    risk_level: CommandRiskLevel,
    description: String,
}

impl BashCommandClassifier {
    pub fn new() -> Self {
        let mut classifier = Self {
            custom_rules: Vec::new(),
        };
        
        // 内置破坏性命令模式
        const DESTRUCTIVE_PATTERNS: &[&str] = &[
            r"rm\s+-rf\s+/",
            r"mkfs",
            r"dd\s+if=/dev/zero",
            r":\(\)\{:\|:&\};:",  // Fork bomb
            r">\s*/dev/sda",
        ];
        
        for pattern in DESTRUCTIVE_PATTERNS {
            classifier.custom_rules.push(ClassifierRule {
                pattern: regex::Regex::new(pattern).unwrap(),
                risk_level: CommandRiskLevel::Destructive,
                description: "破坏性命令".to_string(),
            });
        }
        
        // 潜在风险命令
        const RISKY_PATTERNS: &[&str] = &[
            r"git\s+push\s+--force",
            r"git\s+reset\s+--hard",
            r"git\s+clean\s+-fd",
            r"DROP\s+TABLE",
            r"DELETE\s+FROM",
            r"curl\s+.*\|\s*(sh|bash)",
            r"wget\s+.*\|\s*(sh|bash)",
        ];
        
        for pattern in RISKY_PATTERNS {
            classifier.custom_rules.push(ClassifierRule {
                pattern: regex::Regex::new(pattern).unwrap(),
                risk_level: CommandRiskLevel::PotentiallyRisky,
                description: "潜在风险命令".to_string(),
            });
        }
        
        classifier
    }
    
    pub fn classify(&self, command: &str) -> CommandRiskLevel {
        // 检查自定义规则
        for rule in &self.custom_rules {
            if rule.pattern.is_match(command) {
                return rule.risk_level;
            }
        }
        
        // 检查是否为只读命令
        if self.is_readonly_command(command) {
            return CommandRiskLevel::Safe;
        }
        
        CommandRiskLevel::Unknown
    }
    
    fn is_readonly_command(&self, command: &str) -> bool {
        const READONLY: &[&str] = &[
            r"^ls\b", r"^cat\b", r"^head\b", r"^tail\b",
            r"^grep\b", r"^find\b", r"^git\s+status\b",
            r"^git\s+log\b", r"^git\s+diff\b",
            r"^cargo\s+check\b", r"^cargo\s+clippy\b",
        ];
        
        READONLY.iter().any(|p| regex::Regex::new(p).unwrap().is_match(command))
    }
}
```

---

## 5. 配置文件设计

```toml
# ~/.config/yode/config.toml

[tools]
# 工具预算
budget_per_turn = 20      # 每轮最多 20 次工具调用
total_budget = 50         # 总会话最多 50 次

# 工具超时
[tools.timeouts]
default = 30              # 默认 30 秒
bash = 120                # Bash 2 分钟
web = 30                  # Web 30 秒
lsp = 60                  # LSP 1 分钟
agent = 300               # 代理 5 分钟

# Bash 分类器
[tools.bash_classifier]
enabled = true
# 自定义规则
[[tools.bash_classifier.rules]]
pattern = "npm run build"
risk_level = "safe"
description = "构建命令"

[[tools.bash_classifier.rules]]
pattern = "npm publish"
risk_level = "dangerous"
description = "发布命令需要确认"

# 进度追踪
[tools.progress]
enabled = true
show_in_status = true
```

---

## 6. Claude Code 工具系统架构深度分析

### 6.1 工具注册表与条件导入

```typescript
// src/tools.ts - 核心工具注册表

/**
 * 获取所有内置工具的穷举列表
 * 使用环境变量和 feature flag 进行条件导入（死代码消除）
 */
export function getAllBaseTools(): Tools {
  return [
    AgentTool,
    TaskOutputTool,
    BashTool,
    // 当有嵌入式搜索工具时，不启用独立的 Glob/Grep 工具
    ...(hasEmbeddedSearchTools() ? [] : [GlobTool, GrepTool]),
    ExitPlanModeV2Tool,
    FileReadTool,
    FileEditTool,
    FileWriteTool,
    NotebookEditTool,
    WebFetchTool,
    TodoWriteTool,
    WebSearchTool,
    TaskStopTool,
    AskUserQuestionTool,
    SkillTool,
    EnterPlanModeTool,
    // 仅限 ant 用户的工具
    ...(process.env.USER_TYPE === 'ant' ? [ConfigTool] : []),
    ...(process.env.USER_TYPE === 'ant' ? [TungstenTool] : []),
    // 条件导入：根据 feature flag 动态加载
    ...(SuggestBackgroundPRTool ? [SuggestBackgroundPRTool] : []),
    ...(WebBrowserTool ? [WebBrowserTool] : []),
    // Task v2 工具组
    ...(isTodoV2Enabled()
      ? [TaskCreateTool, TaskGetTool, TaskUpdateTool, TaskListTool]
      : []),
    // LSP 工具（需显式启用）
    ...(isEnvTruthy(process.env.ENABLE_LSP_TOOL) ? [LSPTool] : []),
    // 工作树模式工具
    ...(isWorktreeModeEnabled() ? [EnterWorktreeTool, ExitWorktreeTool] : []),
    // 延迟导入：懒加载以打破循环依赖
    getSendMessageTool(),
    ...(ListPeersTool ? [ListPeersTool] : []),
    // Agent Swarm 工具
    ...(isAgentSwarmsEnabled()
      ? [getTeamCreateTool(), getTeamDeleteTool()]
      : []),
    // REPL 工具（仅限 ant + REPL 模式）
    ...(process.env.USER_TYPE === 'ant' && REPLTool ? [REPLTool] : []),
    // Cron 工具（feature flag 控制）
    ...cronTools,
    // 通知工具
    ...(SendUserFileTool ? [SendUserFileTool] : []),
    ...(PushNotificationTool ? [PushNotificationTool] : []),
    BriefTool,
    ListMcpResourcesTool,
    ReadMcpResourceTool,
    // 工具搜索工具
    ...(isToolSearchEnabledOptimistic() ? [ToolSearchTool] : []),
  ]
}
```

**条件导入模式：**

| 模式 | 用途 | 示例 |
|------|------|------|
| `feature()` | Bundle-time 死代码消除 | `feature('PROACTIVE')` |
| `process.env` | 运行时环境检测 | `process.env.USER_TYPE === 'ant'` |
| `isEnvTruthy()` | 显式环境变量启用 | `isEnvTruthy(process.env.ENABLE_LSP_TOOL)` |
| 懒加载 | 打破循环依赖 | `getSendMessageTool()` |

### 6.2 工具池组装逻辑

```typescript
/**
 * 组装完整的工具池（内置工具 + MCP 工具）
 * 
 * 流程：
 * 1. 获取内置工具（已应用模式过滤）
 * 2. 过滤 MCP 工具（应用否认规则）
 * 3. 按名称去重（内置工具优先）
 * 4. 排序以保持提示缓存稳定性
 */
export function assembleToolPool(
  permissionContext: ToolPermissionContext,
  mcpTools: Tools,
): Tools {
  const builtInTools = getTools(permissionContext)
  
  // 过滤 MCP 工具
  const allowedMcpTools = filterToolsByDenyRules(mcpTools, permissionContext)
  
  // 排序并合并（内置工具优先，避免 MCP 工具插入导致缓存失效）
  const byName = (a: Tool, b: Tool) => a.name.localeCompare(b.name)
  return uniqBy(
    [...builtInTools].sort(byName).concat(allowedMcpTools.sort(byName)),
    'name',
  )
}
```

**工具过滤顺序：**

```
1. getAllBaseTools() - 获取所有内置工具
2. getTools() - 应用模式过滤（Simple 模式、REPL 模式）
3. filterToolsByDenyRules() - 应用用户/项目/CLI 否认规则
4. uniqBy() - MCP 工具合并时去重
```

### 6.3 权限否认规则过滤

```typescript
/**
 * 过滤被否认规则完全禁止的工具
 * 
 * 否认规则匹配逻辑：
 * - 精确匹配工具名（如 "Bash"）
 * - MCP 服务器前缀匹配（如 "mcp__server" 禁止该服务器所有工具）
 * - 通配符匹配
 * 
 * 规则来源优先级：
 * 1. 用户设置 (~/.config/yode/config.toml)
 * 2. 项目设置 (.yode/config.toml)
 * 3. 本地设置
 * 4. CLI 参数
 * 5. 会话级别
 */
export function filterToolsByDenyRules<
  T extends {
    name: string
    mcpInfo?: { serverName: string; toolName: string }
  },
>(tools: readonly T[], permissionContext: ToolPermissionContext): T[] {
  return tools.filter(tool => !getDenyRuleForTool(permissionContext, tool))
}
```

**否认规则示例：**

```toml
# ~/.config/yode/config.toml

# 完全禁止 Bash 工具
[[deny_rules]]
tool = "Bash"

# 禁止特定 MCP 服务器的所有工具
[[deny_rules]]
tool = "mcp__filesystem"  # 禁止所有 mcp__filesystem_* 工具

# 带条件的规则
[[deny_rules]]
tool = "Bash"
when = { pattern = "^rm.*" }  # 只禁止 rm 命令
```

### 6.4 REPL 模式工具隐藏

```typescript
// src/tools/REPLTool/constants.ts

/**
 * REPL 模式下仅允许的工具
 * 这些工具在 REPL 外部会被隐藏
 */
export const REPL_ONLY_TOOLS = new Set([
  'Bash',
  'FileRead',
  'FileEdit',
  'FileWrite',
  'Glob',
  'Grep',
])

/**
 * Simple 模式 + REPL：只返回 REPL 工具
 * REPL 内部通过 VM 上下文访问原始工具
 */
if (isReplModeEnabled() && REPLTool) {
  const replSimple: Tool[] = [REPLTool]
  if (feature('COORDINATOR_MODE') && coordinatorModeModule?.isCoordinatorMode()) {
    replSimple.push(TaskStopTool, getSendMessageTool())
  }
  return filterToolsByDenyRules(replSimple, permissionContext)
}
```

**REPL 模式工具可见性：**

| 工具类型 | REPL 外部 | REPL 内部 |
|---------|----------|----------|
| 原始工具 (Bash/Read/Edit) | 隐藏 | 通过 VM 访问 |
| REPL 工具 | 显示 | 显示 |
| 高级工具 (Agent/Skill) | 显示 | 显示 |

### 6.5 延迟工具激活

```typescript
// src/tools/registry.ts (简化)

class ToolRegistry {
  private tools: Map<string, Tool> = new Map()
  private deferredTools: Map<string, Tool> = new Map()
  
  // 注册延迟工具（已知但不发送给 LLM）
  registerDeferred(tool: Tool): void {
    this.deferredTools.set(tool.name, tool)
  }
  
  // 激活动画：将延迟工具移入活跃工具集
  activate(name: string): boolean {
    const tool = this.deferredTools.get(name)
    if (tool) {
      this.deferredTools.delete(name)
      this.tools.set(name, tool)
      return true
    }
    return false
  }
  
  // 获取工具定义（仅返回活跃工具）
  definitions(): ToolDefinition[] {
    return Array.from(this.tools.values()).map(tool => ({
      name: tool.name,
      description: tool.description,
      inputSchema: tool.inputSchema,
    }))
  }
}
```

**延迟激活场景：**

- 技能触发时动态激活相关工具
- 计划模式激活后激活特定工具
- MCP 服务器连接后激活其工具

---

## 7. 总结

### 7.1 Claude Code 工具系统特点

| 特性 | 实现方式 | 价值 |
|------|----------|------|
| **条件导入** | `feature()` + 环境变量 | 死代码消除，减小包体积 |
| **懒加载** | 函数式 getter | 打破循环依赖 |
| **工具池组装** | `assembleToolPool()` | 统一内置 + MCP 工具处理 |
| **否认规则过滤** | `filterToolsByDenyRules()` | 灵活权限控制 |
| **REPL 模式** | `REPL_ONLY_TOOLS` | 隐藏原始工具 |
| **延迟激活** | `deferredTools` 机制 | 动态工具发现 |
| **进度追踪** | `ToolProgressData` 类型 | 实时反馈 |
| **超时控制** | `tokio::time::timeout` | 防止无限等待 |
| **预算限制** | `ToolBudgetTracker` | 控制调用次数 |
| **遥测收集** | `ToolTelemetry` | 性能分析 |

### 7.2 Yode 工具系统优化优先级

**第一阶段（核心增强）：**
1. 工具进度追踪框架
2. Bash 命令分类器集成
3. 超时与重试机制

**第二阶段（权限与控制）：**
4. 预算控制追踪器
5. 否认规则扩展（支持 MCP 服务器前缀）
6. 延迟工具激活机制

**第三阶段（高级功能）：**
7. 执行遥测与报告
8. REPL 模式工具隐藏
9. 条件工具导入（feature flag）

### 7.3 关键设计模式

1. **工具池组装模式** - 统一内置与 MCP 工具处理
2. **条件导入模式** - 根据环境和 flag 动态加载
3. **懒加载模式** - 打破循环依赖
4. **否认规则模式** - 优先级链式规则匹配
5. **进度报告模式** - 异步通道实时推送
6. **预算追踪模式** - 原子计数器

---

## 8. 配置文件扩展示例

```toml
# ~/.config/yode/config.toml

[tools]
# 预算控制
budget_per_turn = 20
total_budget = 50

# 超时配置
[tools.timeouts]
default = 30
bash = 120
web = 30
lsp = 60
agent = 300

# 进度追踪
[tools.progress]
enabled = true
show_in_status = true

# Bash 分类器
[tools.bash_classifier]
enabled = true

[[tools.bash_classifier.rules]]
pattern = "npm run build"
risk_level = "safe"
description = "构建命令"

[[tools.bash_classifier.rules]]
pattern = "npm publish"
risk_level = "dangerous"
description = "发布命令"

# 否认规则
[[deny_rules]]
tool = "Bash"
when = { pattern = "^rm\\s+-rf" }

[[deny_rules]]
tool = "mcp__filesystem"  # 禁止所有 filesystem MCP 工具

# 延迟工具
[[tools.deferred]]
name = "LSP"
trigger = "rust"  # 当检测到 Rust 项目时激活
```
