# 权限系统深度分析与优化

## 1. Claude Code 权限系统架构

### 1.1 权限决策类型

```typescript
// src/utils/permissions/PermissionResult.ts

export type PermissionBehavior = 'allow' | 'deny' | 'ask'

export type PermissionDecision =
  | PermissionAllowDecision
  | PermissionDenyDecision
  | PermissionAskDecision

export type PermissionAllowDecision = {
  behavior: 'allow'
  source: PermissionRuleSource
  reason?: PermissionDecisionReason
}

export type PermissionDenyDecision = {
  behavior: 'deny'
  source: PermissionRuleSource
  reason?: PermissionDecisionReason
}

export type PermissionAskDecision = {
  behavior: 'ask'
  reason?: PermissionDecisionReason
}
```

### 1.2 规则来源优先级

```typescript
// src/utils/permissions/permissions.ts

const PERMISSION_RULE_SOURCES = [
  ...SETTING_SOURCES,  // user, project, local, flag, policy
  'cliArg',           // CLI 参数
  'command',          // 命令
  'session',         // 会话
] as const

// 优先级显示名称
function permissionRuleSourceDisplayString(
  source: PermissionRuleSource,
): string {
  return getSettingSourceDisplayNameLowercase(source)
}

// 规则来源显示顺序（从高到低）
// 1. user - 用户设置 (~/.config/yode/config.toml)
// 2. project - 项目设置 (.yode/config.toml)
// 3. local - 本地设置
// 4. flag - Flag 设置
// 5. policy - Policy 设置
// 6. cliArg - CLI 参数
// 7. command - 命令
// 8. session - 会话
```

### 1.3 权限决策流程

```typescript
// src/utils/permissions/permissions.ts

/**
 * 获取权限决策的核心流程
 */
export async function getPermissionDecision(
  toolName: string,
  input: unknown,
  context: ToolPermissionContext,
  assistantMessage: AssistantMessage,
): Promise<PermissionDecision> {
  // ========== 步骤 1: Bypass 模式检查 ==========
  if (context.mode === 'bypass') {
    return { behavior: 'allow', source: 'mode' }
  }
  
  // ========== 步骤 2: 显式允许规则检查 ==========
  const allowRule = toolAlwaysAllowedRule(context, tool)
  if (allowRule) {
    return { behavior: 'allow', source: allowRule.source }
  }
  
  // ========== 步骤 3: 显式拒绝规则检查 ==========
  const denyRule = getDenyRuleForTool(context, tool)
  if (denyRule) {
    return { behavior: 'deny', source: denyRule.source }
  }
  
  // ========== 步骤 4: 显式询问规则检查 ==========
  const askRule = getAskRuleForTool(context, tool)
  if (askRule) {
    return { 
      behavior: 'ask', 
      reason: { type: 'rule', rule: askRule } 
    }
  }
  
  // ========== 步骤 5: 分类器检查（如果启用） ==========
  if (feature('BASH_CLASSIFIER') || feature('TRANSCRIPT_CLASSIFIER')) {
    const classifierDecision = await checkClassifier(toolName, input)
    if (classifierDecision) {
      return classifierDecision
    }
  }
  
  // ========== 步骤 6: Hook 检查 ==========
  const hookDecision = await executePermissionRequestHooks(
    toolName,
    input,
    context,
    assistantMessage,
  )
  if (hookDecision) {
    return hookDecision
  }
  
  // ========== 步骤 7: 默认行为（根据模式） ==========
  switch (context.mode) {
    case 'auto':
      return { behavior: 'deny' }  // 自动拒绝
    case 'plan':
      return { behavior: 'deny' }  // 计划模式拒绝
    case 'acceptEdits':
      if (toolName === 'FileEdit') {
        return { behavior: 'allow', source: 'mode' }
      }
      break
    case 'dontAsk':
      return { behavior: 'deny' }
  }
  
  // ========== 步骤 8: 默认询问 ==========
  return { behavior: 'ask' }
}
```

---

## 2. 规则匹配逻辑

### 2.1 工具与规则匹配

```typescript
/**
 * 检查工具是否与规则匹配
 * 支持 MCP 服务器前缀匹配
 */
function toolMatchesRule(
  tool: Pick<Tool, 'name' | 'mcpInfo'>,
  rule: PermissionRule,
): boolean {
  // 规则必须没有内容才能匹配整个工具
  if (rule.ruleValue.ruleContent !== undefined) {
    return false
  }
  
  // 获取工具名称（MCP 工具使用完整限定名）
  const nameForRuleMatch = getToolNameForPermissionCheck(tool)
  
  // 直接工具名匹配
  if (rule.ruleValue.toolName === nameForRuleMatch) {
    return true
  }
  
  // MCP 服务器级别权限匹配
  // 规则 "mcp__server1" 匹配工具 "mcp__server1__tool1"
  // 通配符：规则 "mcp__server1__*" 匹配 server1 的所有工具
  const ruleInfo = mcpInfoFromString(rule.ruleValue.toolName)
  const toolInfo = mcpInfoFromString(nameForRuleMatch)
  
  return (
    ruleInfo !== null &&
    toolInfo !== null &&
    (ruleInfo.toolName === undefined || ruleInfo.toolName === '*') &&
    ruleInfo.serverName === toolInfo.serverName
  )
}
```

### 2.2 通配符模式匹配

```typescript
// src/utils/permissions/shellRuleMatching.ts

/**
 * 通配符模式匹配
 * 支持 * (任意字符) 和 ? (单个字符)
 */
export function matchWildcardPattern(
  pattern: string,
  text: string,
): boolean {
  // 转换为正则表达式
  const regexPattern = pattern
    .replace(/[.+^${}()|[\]\\]/g, '\\$&')  // 转义特殊字符
    .replace(/\*/g, '.*')                  // * -> .*
    .replace(/\?/g, '.')                   // ? -> .
  
  const regex = new RegExp(`^${regexPattern}$`, 'i')
  return regex.test(text)
}

// 示例
matchWildcardPattern('Bash', 'Bash')           // true
matchWildcardPattern('Bash*', 'Bash(prefix:rm)') // true
matchWildcardPattern('mcp__*', 'mcp__filesystem__read') // true
matchWildcardPattern('*.py', 'script.py')      // true
```

### 2.3 基于内容的规则匹配

```typescript
// src/utils/permissions/permissions.ts

/**
 * 按内容获取工具的规则映射
 * 例如：Bash(prefix:*) -> 规则对象
 */
export function getRuleByContentsForTool(
  context: ToolPermissionContext,
  tool: Tool,
  behavior: PermissionBehavior,
): Map<string, PermissionRule> {
  const ruleByContents = new Map<string, PermissionRule>()
  let rules: PermissionRule[] = []
  
  switch (behavior) {
    case 'allow':
      rules = getAllowRules(context)
      break
    case 'deny':
      rules = getDenyRules(context)
      break
    case 'ask':
      rules = getAskRules(context)
      break
  }
  
  for (const rule of rules) {
    if (
      rule.ruleValue.toolName === tool.name &&
      rule.ruleValue.ruleContent !== undefined &&
      rule.ruleBehavior === behavior
    ) {
      ruleByContents.set(rule.ruleValue.ruleContent, rule)
    }
  }
  
  return ruleByContents
}

// 使用示例
const denyRules = getRuleByContentsForTool(context, BashTool, 'deny')
if (denyRules.has('prefix:rm')) {
  // rm 命令被拒绝
}
```

---

## 3. 决策原因类型

### 3.1 原因类型定义

```typescript
// src/utils/permissions/PermissionResult.ts

export type PermissionDecisionReason =
  | { type: 'classifier'; classifier: string; reason: string }
  | { type: 'hook'; hookName: string; reason?: string }
  | { type: 'rule'; rule: PermissionRule }
  | { type: 'subcommandResults'; reasons: Array<[string, PermissionDecision]> }
  | { type: 'permissionPromptTool'; permissionPromptToolName: string }
  | { type: 'sandboxOverride' }
  | { type: 'workingDir'; reason: string }
  | { type: 'safetyCheck' }
  | { type: 'other'; reason: string }
  | { type: 'mode'; mode: PermissionMode }
  | { type: 'asyncAgent'; reason: string }
```

### 3.2 权限请求消息生成

```typescript
// src/utils/permissions/permissions.ts

export function createPermissionRequestMessage(
  toolName: string,
  decisionReason?: PermissionDecisionReason,
): string {
  if (decisionReason) {
    switch (decisionReason.type) {
      // ========== 分类器原因 ==========
      case 'classifier':
        return `Classifier '${decisionReason.classifier}' requires approval ` +
               `for this ${toolName} command: ${decisionReason.reason}`
      
      // ========== Hook 原因 ==========
      case 'hook':
        const hookMessage = decisionReason.reason
          ? `Hook '${decisionReason.hookName}' blocked this action: ${decisionReason.reason}`
          : `Hook '${decisionReason.hookName}' requires approval`
        return hookMessage
      
      // ========== 规则原因 ==========
      case 'rule':
        const ruleString = permissionRuleValueToString(decisionReason.rule.ruleValue)
        const sourceString = permissionRuleSourceDisplayString(decisionReason.rule.source)
        return `Permission rule '${ruleString}' from ${sourceString} ` +
               `requires approval for this ${toolName} command`
      
      // ========== 子命令结果 ==========
      case 'subcommandResults':
        const needsApproval: string[] = []
        for (const [cmd, result] of decisionReason.reasons) {
          if (result.behavior === 'ask' || result.behavior === 'passthrough') {
            // Bash 工具：移除输出重定向显示
            if (toolName === 'Bash') {
              const { commandWithoutRedirections, redirections } = 
                extractOutputRedirections(cmd)
              const displayCmd = redirections.length > 0 
                ? commandWithoutRedirections 
                : cmd
              needsApproval.push(displayCmd)
            } else {
              needsApproval.push(cmd)
            }
          }
        }
        if (needsApproval.length > 0) {
          const n = needsApproval.length
          return `This ${toolName} command contains ${n} ` +
                 `part${n > 1 ? 's' : ''} that require approval: ` +
                 `${needsApproval.join(', ')}`
        }
        return `This ${toolName} command contains operations that require approval`
      
      // ========== 模式原因 ==========
      case 'mode':
        const modeTitle = permissionModeTitle(decisionReason.mode)
        return `Current permission mode (${modeTitle}) ` +
               `requires approval for this ${toolName} command`
      
      // ========== Sandbox 覆盖 ==========
      case 'sandboxOverride':
        return 'Run outside of the sandbox'
      
      // ========== 其他 ==========
      case 'other':
      case 'safetyCheck':
      case 'workingDir':
        return decisionReason.reason
    }
  }
  
  // 默认消息
  return `Claude requested permissions to use ${toolName}, ` +
         `but you haven't granted it yet.`
}
```

---

## 4. 拒绝跟踪机制

### 4.1 拒绝跟踪状态

```typescript
// src/utils/permissions/denialTracking.ts

// 拒绝限制配置
export const DENIAL_LIMITS = {
  // 每轮拒绝次数上限
  perTurn: 3,
  // 总拒绝次数上限
  total: 5,
  // 冷却时间（毫秒）
  cooldownMs: 60000, // 1 分钟
}

// 拒绝跟踪状态
export interface DenialTrackingState {
  // 每轮拒绝计数
  turnDenials: number
  // 总拒绝计数
  totalDenials: number
  // 最后拒绝时间戳
  lastDenialTime: number
}

// 创建拒绝跟踪状态
export function createDenialTrackingState(): DenialTrackingState {
  return {
    turnDenials: 0,
    totalDenials: 0,
    lastDenialTime: 0,
  }
}

// 记录拒绝
export function recordDenial(state: DenialTrackingState): void {
  state.turnDenials++
  state.totalDenials++
  state.lastDenialTime = Date.now()
}

// 检查是否应回退到提示
export function shouldFallbackToPrompting(
  state: DenialTrackingState,
): boolean {
  // 检查冷却时间是否已过
  const cooldownExpired = 
    Date.now() - state.lastDenialTime > DENIAL_LIMITS.cooldownMs
  
  if (cooldownExpired) {
    return true
  }
  
  // 检查是否超过限制
  return (
    state.turnDenials >= DENIAL_LIMITS.perTurn ||
    state.totalDenials >= DENIAL_LIMITS.total
  )
}
```

### 4.2 Yolo 分类器集成

```typescript
// src/utils/permissions/yoloClassifier.ts

/**
 * Yolo 模式分类器
 * 自动允许/拒绝常见操作
 */
export function classifyYoloAction(
  toolName: string,
  input: unknown,
): 'allow' | 'deny' | 'unknown' {
  // 只读命令 -> 允许
  if (isReadonlyCommand(toolName, input)) {
    return 'allow'
  }
  
  // 危险命令 -> 拒绝
  if (isDangerousCommand(toolName, input)) {
    return 'deny'
  }
  
  return 'unknown'
}

// 生成 Yolo 拒绝消息
export function buildYoloRejectionMessage(
  toolName: string,
  input: unknown,
): string {
  const classification = classifyYoloAction(toolName, input)
  
  if (classification === 'deny') {
    return `Auto-denied potentially dangerous ${toolName} command.`
  }
  
  return ''
}
```

---

## 5. 自动模式拒绝

### 5.1 自动模式状态管理

```typescript
// src/utils/permissions/autoModeState.ts

interface AutoModeState {
  // 当前轮拒绝计数
  turnDenials: number
  // 总拒绝计数
  totalDenials: number
  // 最后拒绝时间
  lastDenialTime: number
  // 分类器检查状态
  classifierChecking: boolean
}

// 检查分类器状态
export function isClassifierChecking(): boolean {
  return autoModeState.classifierChecking
}

// 设置分类器检查状态
export function setClassifierChecking(checking: boolean): void {
  autoModeState.classifierChecking = checking
}

// 清除分类器检查状态
export function clearClassifierChecking(): void {
  autoModeState.classifierChecking = false
}
```

### 5.2 自动拒绝消息

```typescript
// src/utils/messages.ts

export const AUTO_REJECT_MESSAGE = 
  'Auto-rejected: Too many permission denials in this turn.'

export const DONT_ASK_REJECT_MESSAGE = 
  'Rejected: Current permission mode does not allow this action.'

// 生成分类器不可用消息
export function buildClassifierUnavailableMessage(): string {
  return 'Classifier unavailable: Please manually approve this action.'
}
```

---

## 6. Hook 权限请求

### 6.1 Hook 执行流程

```typescript
// src/utils/hooks.ts

/**
 * 执行权限请求 Hook
 * 适用于无头/异步 agent
 */
export async function executePermissionRequestHooks(
  toolName: string,
  input: unknown,
  context: ToolPermissionContext,
  assistantMessage: AssistantMessage,
): Promise<PermissionDecision | null> {
  const hooks = getRegisteredHooks('permission_request')
  
  for (const hook of hooks) {
    try {
      const decision = await hook.callback({
        toolName,
        input,
        context,
        assistantMessage,
      })
      
      if (decision) {
        return decision
      }
    } catch (error) {
      logError('Hook error:', error)
      // 继续执行下一个 Hook
    }
  }
  
  return null  // 无 Hook 提供决策
}
```

---

## 7. Yode 权限系统优化建议

### 7.1 第一阶段：权限决策框架

```rust
// crates/yode-core/src/permissions/decision.rs

/// 权限行为
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PermissionBehavior {
    Allow,
    Deny,
    Ask,
}

/// 规则来源
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RuleSource {
    User,
    Project,
    Local,
    CliArg,
    Command,
    Session,
}

/// 权限决策
#[derive(Debug, Clone)]
pub struct PermissionDecision {
    pub behavior: PermissionBehavior,
    pub source: Option<RuleSource>,
    pub reason: Option<String>,
}

/// 获取权限决策
pub async fn get_permission_decision(
    tool_name: &str,
    input: &serde_json::Value,
    context: &PermissionContext,
) -> PermissionDecision {
    // 1. Bypass 模式检查
    if context.mode == PermissionMode::Bypass {
        return PermissionDecision {
            behavior: PermissionBehavior::Allow,
            source: None,
            reason: Some("Bypass mode".to_string()),
        };
    }
    
    // 2. 显式允许规则
    if let Some(rule) = find_allow_rule(tool_name, input, context) {
        return PermissionDecision {
            behavior: PermissionBehavior::Allow,
            source: Some(rule.source),
            reason: None,
        };
    }
    
    // 3. 显式拒绝规则
    if let Some(rule) = find_deny_rule(tool_name, input, context) {
        return PermissionDecision {
            behavior: PermissionBehavior::Deny,
            source: Some(rule.source),
            reason: None,
        };
    }
    
    // 4. 显式询问规则
    if let Some(rule) = find_ask_rule(tool_name, input, context) {
        return PermissionDecision {
            behavior: PermissionBehavior::Ask,
            source: Some(rule.source),
            reason: None,
        };
    }
    
    // 5. 默认行为
    match context.mode {
        PermissionMode::Auto => PermissionDecision {
            behavior: PermissionBehavior::Deny,
            source: None,
            reason: Some("Auto mode".to_string()),
        },
        _ => PermissionDecision {
            behavior: PermissionBehavior::Ask,
            source: None,
            reason: None,
        },
    }
}
```

### 7.2 第二阶段：规则匹配

```rust
// crates/yode-core/src/permissions/rule_matching.rs

use regex::Regex;

/// 通配符模式匹配
pub fn match_wildcard_pattern(pattern: &str, text: &str) -> bool {
    // 转换为正则表达式
    let regex_pattern = regex::escape(pattern)
        .replace('*', ".*")
        .replace('?', ".");
    
    let regex = Regex::new(&format!("^{}$", regex_pattern)).unwrap();
    regex.is_match(text)
}

/// 工具与规则匹配
pub fn tool_matches_rule(
    tool_name: &str,
    rule_content: Option<&str>,
    rule_pattern: &str,
) -> bool {
    // 有内容时使用内容匹配
    if let Some(content) = rule_content {
        return match_wildcard_pattern(content, tool_name);
    }
    
    // 否则使用工具名匹配
    match_wildcard_pattern(rule_pattern, tool_name)
}

/// MCP 服务器前缀匹配
pub fn mcp_server_matches(
    rule_server: &str,
    tool_server: &str,
    tool_name: &str,
) -> bool {
    // 完全匹配服务器名
    if rule_server == tool_server {
        return true;
    }
    
    // 通配符匹配
    if rule_server.ends_with('*') {
        let prefix = &rule_server[..rule_server.len() - 1];
        return tool_server.starts_with(prefix);
    }
    
    false
}
```

### 7.3 第三阶段：拒绝跟踪

```rust
// crates/yode-core/src/permissions/denial_tracking.rs

use std::time::{Duration, Instant};

/// 拒绝限制配置
pub struct DenialLimits {
    pub per_turn: u32,
    pub total: u32,
    pub cooldown: Duration,
}

impl Default for DenialLimits {
    fn default() -> Self {
        Self {
            per_turn: 3,
            total: 5,
            cooldown: Duration::from_secs(60),
        }
    }
}

/// 拒绝跟踪状态
pub struct DenialTracking {
    pub turn_denials: u32,
    pub total_denials: u32,
    pub last_denial_time: Option<Instant>,
}

impl DenialTracking {
    pub fn new() -> Self {
        Self {
            turn_denials: 0,
            total_denials: 0,
            last_denial_time: None,
        }
    }
    
    /// 记录拒绝
    pub fn record_denial(&mut self) {
        self.turn_denials += 1;
        self.total_denials += 1;
        self.last_denial_time = Some(Instant::now());
    }
    
    /// 重置轮计数
    pub fn reset_turn(&mut self) {
        self.turn_denials = 0;
    }
    
    /// 检查是否应回退到提示
    pub fn should_fallback_to_prompting(&self, limits: &DenialLimits) -> bool {
        // 检查冷却时间
        if let Some(last) = self.last_denial_time {
            if last.elapsed() > limits.cooldown {
                return true;
            }
        }
        
        // 检查限制
        self.turn_denials >= limits.per_turn || 
        self.total_denials >= limits.total
    }
}
```

### 7.4 第四阶段：命令分类器

```rust
// crates/yode-core/src/permissions/classifier.rs

/// 命令分类器
pub struct CommandClassifier {
    readonly_patterns: Vec<Regex>,
    dangerous_patterns: Vec<Regex>,
}

impl CommandClassifier {
    pub fn new() -> Self {
        let readonly = vec![
            r"^ls\b", r"^cat\b", r"^head\b", r"^tail\b",
            r"^grep\b", r"^find\b", r"^git\s+status\b",
        ];
        
        let dangerous = vec![
            r"^rm\s+-rf\b", r"^mkfs\b", r"^dd\b",
            r"^curl.*\|\s*(sh|bash)\b",
        ];
        
        Self {
            readonly_patterns: readonly.iter()
                .map(|p| Regex::new(p).unwrap()).collect(),
            dangerous_patterns: dangerous.iter()
                .map(|p| Regex::new(p).unwrap()).collect(),
        }
    }
    
    /// 分类命令
    pub fn classify(&self, command: &str) -> Classification {
        // 检查危险命令
        for pattern in &self.dangerous_patterns {
            if pattern.is_match(command) {
                return Classification::Dangerous;
            }
        }
        
        // 检查只读命令
        for pattern in &self.readonly_patterns {
            if pattern.is_match(command) {
                return Classification::Readonly;
            }
        }
        
        Classification::Unknown
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Classification {
    Readonly,
    Dangerous,
    Unknown,
}
```

---

## 8. 配置文件示例

```toml
# ~/.config/yode/config.toml

# 权限规则
[[always_allow]]
tool = "Bash"
pattern = "prefix:ls *"

[[always_allow]]
tool = "Bash"
pattern = "prefix:cat *"

[[always_allow]]
tool = "FileRead"

[[always_deny]]
tool = "Bash"
pattern = "prefix:rm -rf *"

[[always_deny]]
tool = "Bash"
pattern = "prefix:mkfs *"

[[always_ask]]
tool = "FileEdit"
pattern = "*"

# 拒绝限制
[permissions.denial_limits]
per_turn = 3
total = 5
cooldown_seconds = 60

# 自动模式
[permissions.auto_mode]
enabled = true
use_classifier = true
fallback_to_ask = true
```

---

## 9. 总结

Claude Code 权限系统的核心特点：

1. **多来源规则** - 用户/项目/本地/CLI/命令/会话
2. **优先级链** - 规则来源有明确优先级
3. **通配符匹配** - 支持 * 和 ? 通配符
4. **MCP 前缀匹配** - 服务器级别权限控制
5. **内容匹配** - 基于命令内容的规则
6. **决策原因追踪** - 完整的决策原因类型
7. **拒绝跟踪** - 防止无限拒绝循环
8. **Yolo 分类器** - 自动允许/拒绝常见操作
9. **Hook 集成** - 可扩展的权限 Hook
10. **模式支持** - bypass/auto/plan/acceptEdits/dontAsk

Yode 优化优先级：
1. 权限决策框架
2. 规则匹配引擎（通配符）
3. 拒绝跟踪机制
4. 命令分类器
5. Hook 权限请求
6. MCP 前缀匹配
