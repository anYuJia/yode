# 权限系统深度分析与优化建议

## 1. Claude Code 权限系统架构

### 1.1 权限模式 (Permission Modes)

Claude Code 实现了 6 种权限模式：

```typescript
// 外部可见的模式（用户可配置）
const EXTERNAL_PERMISSION_MODES = [
  'acceptEdits',      // 接受编辑，自动确认文件修改
  'bypassPermissions', // 绕过权限（危险操作）
  'default',          // 默认模式
  'dontAsk',          // 不再询问
  'plan',             // 仅生成计划，不执行
] as const

// 内部模式（包含自动模式）
type InternalPermissionMode = ExternalPermissionMode | 'auto' | 'bubble'
```

**模式详解：**

| 模式 | 行为 | 适用场景 |
|------|------|----------|
| `default` | 危险工具需确认 | 日常开发 |
| `plan` | 只生成计划，不执行任何工具 | 代码审查、方案设计 |
| `auto` | 基于分类器自动决策 | 高频重复任务 |
| `bypassPermissions` | 完全绕过权限检查 | 受信任环境 |
| `acceptEdits` | 自动确认文件编辑 | 重构任务 |
| `dontAsk` | 不再询问，全部自动执行 | 批处理任务 |

### 1.2 权限规则系统

Claude Code 的权限规则是分层级的：

```typescript
type PermissionRuleSource =
  | 'userSettings'    // 用户级配置 (~/.config/claude/settings.json)
  | 'projectSettings' // 项目级配置 (.claude/settings.json)
  | 'localSettings'   // 本地配置
  | 'flagSettings'    // 特性标志配置
  | 'policySettings'  // 策略配置
  | 'cliArg'          // 命令行参数
  | 'command'         // 命令来源
  | 'session'         // 会话级规则

type PermissionRule = {
  source: PermissionRuleSource
  ruleBehavior: 'allow' | 'deny' | 'ask'
  ruleValue: {
    toolName: string
    ruleContent?: string  // 可选的规则内容匹配
  }
}
```

**规则优先级：**
```
cliArg > command > session > localSettings > projectSettings > userSettings
```

### 1.3 权限决策流程

```typescript
// 简化的决策流程
async function checkPermission(toolName, input, context): Promise<PermissionResult> {
  // 1. 检查是否在 bypass 模式
  if (context.mode === 'bypassPermissions') {
    return { behavior: 'allow' }
  }
  
  // 2. 检查显式规则
  const rule = findMatchingRule(toolName, input, context)
  if (rule) {
    return { behavior: rule.ruleBehavior }
  }
  
  // 3. 检查分类器（auto 模式）
  if (context.mode === 'auto') {
    const classifierResult = await runClassifier(toolName, input)
    if (classifierResult.confidence > THRESHOLD) {
      return { behavior: classifierResult.decision }
    }
  }
  
  // 4. 检查 Hook
  const hookResult = await executePermissionHooks(toolName, input)
  if (hookResult.blocked) {
    return { behavior: 'deny', reason: hookResult.reason }
  }
  
  // 5. 默认需要确认
  return { behavior: 'ask' }
}
```

### 1.4 自动拒绝跟踪 (Denial Tracking)

Claude Code 实现了智能的拒绝跟踪机制：

```typescript
type DenialTrackingState = {
  toolName: string
  denialCount: number
  lastDenialTime: number
  fallbackToPrompting: boolean
}

const DENIAL_LIMITS = {
  DEFAULT: 3,           // 默认拒绝 3 次后改变策略
  BASH_DANGEROUS: 5,    // 危险命令拒绝 5 次
  FILE_WRITE: 3,        // 文件写入拒绝 3 次
}

// 当用户多次拒绝同一类请求时，系统会自动降级为询问模式
function shouldFallbackToPrompting(state: DenialTrackingState): boolean {
  return state.denialCount >= DENIAL_LIMITS[state.toolName] || DENIAL_LIMITS.DEFAULT
}
```

### 1.5 Bash 命令分类器

Claude Code 有两个专门的分类器：

**1. BASH_CLASSIFIER** - 实时命令分类
```typescript
// 分类类别
type BashCommandCategory =
  | 'safe'              // 安全命令 (ls, cat, grep)
  | 'potentially_risky' // 潜在风险 (git checkout, npm install)
  | 'dangerous'         // 危险命令 (rm -rf, git push --force)
  | 'destructive'       // 破坏性命令 (drop table, delete *)
```

**2. TRANSCRIPT_CLASSIFIER** - 基于上下文的决策
```typescript
// 考虑当前对话上下文进行分类
const classifierContext = {
  recentCommands: [],      // 最近执行的命令
  filesModified: [],       // 已修改的文件
  userDenials: [],         // 用户拒绝的历史
  conversationPhase: 'early' | 'middle' | 'late',
}
```

### 1.6 权限规则配置示例

```json
// ~/.config/claude/settings.json
{
  "defaultMode": "default",
  "permissionRules": {
    "alwaysAllow": [
      {
        "toolName": "Read",
        "ruleContent": "**/*.md"
      },
      {
        "toolName": "Bash",
        "ruleContent": "git status*"
      },
      {
        "toolName": "Bash",
        "ruleContent": "cargo check"
      }
    ],
    "alwaysDeny": [
      {
        "toolName": "Bash",
        "ruleContent": "rm -rf /*"
      },
      {
        "toolName": "Bash",
        "ruleContent": "*--force*push*"
      }
    ]
  }
}
```

---

## 2. Yode 当前权限系统分析

### 2.1 当前实现

Yode 当前的权限系统位于 `crates/yode-core/src/permission.rs`：

```rust
pub enum PermissionAction {
    Allow,      // 无需确认
    Confirm,    // 需要确认
    Deny,       // 禁止
}

pub struct PermissionManager {
    require_confirmation: HashSet<String>,
}
```

**功能清单：**

| 功能 | Yode 当前状态 | Claude Code |
|------|--------------|-------------|
| 基础权限检查 | ✅ | ✅ |
| 权限模式切换 | ⚠️ (仅 Plan 模式) | ✅ (6 种模式) |
| 规则优先级 | ❌ | ✅ (7 层来源) |
| 命令分类器 | ❌ | ✅ (2 个分类器) |
| 拒绝跟踪 | ❌ | ✅ |
| Hook 系统 | ❌ | ✅ |
| 内容匹配规则 | ❌ | ✅ (ruleContent) |

### 2.2 代码对比

**Yode 当前实现 (126 行)：**
```rust
pub fn check(&self, tool_name: &str) -> PermissionAction {
    if self.require_confirmation.contains(tool_name) {
        PermissionAction::Confirm
    } else {
        PermissionAction::Allow
    }
}
```

**Claude Code 实现 (2800+ 行)：**
- `permissions.ts` - 核心决策逻辑
- `permissionRuleParser.ts` - 规则解析
- `bashClassifier.ts` - Bash 命令分类
- `yoloClassifier.ts` - YOLO 模式分类
- `denialTracking.ts` - 拒绝跟踪
- `classifierDecision.ts` - 分类器决策

---

## 3. 优化建议

### 3.1 第一阶段：基础增强

#### 3.1.1 添加权限模式枚举

```rust
// crates/yode-core/src/permission.rs

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionMode {
    /// 默认模式：危险工具需确认
    Default,
    /// 计划模式：只生成计划，不执行
    Plan,
    /// 自动模式：基于简单规则自动决策
    Auto,
    /// 接受编辑：自动确认文件编辑
    AcceptEdits,
    /// 绕过权限：不询问直接执行
    Bypass,
}

impl Default for PermissionMode {
    fn default() -> Self => PermissionMode::Default
}
```

#### 3.1.2 添加规则来源枚举

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleSource {
    UserConfig,      // 用户配置
    ProjectConfig,   // 项目配置 (.yode/)
    CliArg,          // 命令行参数
    Session,         // 会话级规则
}

pub struct PermissionRule {
    pub source: RuleSource,
    pub behavior: PermissionBehavior,
    pub tool_name: String,
    pub pattern: Option<String>,  // 可选的命令模式匹配
}
```

#### 3.1.3 增强 PermissionManager

```rust
pub struct PermissionManager {
    /// 当前权限模式
    mode: PermissionMode,
    
    /// 按来源分组的规则
    rules: HashMap<RuleSource, Vec<PermissionRule>>,
    
    /// 会话级额外目录
    additional_directories: HashSet<PathBuf>,
    
    /// 拒绝跟踪
    denial_tracking: HashMap<String, DenialState>,
    
    /// 优先级缓存（避免重复计算）
    rule_cache: RwLock<Option<Vec<PermissionRule>>>,
}

struct DenialState {
    count: u32,
    last_denial_time: Instant,
}
```

### 3.2 第二阶段：Bash 命令分类器

#### 3.2.1 危险命令模式匹配

```rust
// crates/yode-tools/src/builtin/bash.rs

const DANGEROUS_PATTERNS: &[&str] = &[
    "rm -rf /",
    "rm -rf /*",
    "mkfs",
    "dd if=/dev/zero",
    ":(){:|:&};:",  // Fork bomb
    "> /dev/sda",
];

const POTENTIALLY_RISKY_PATTERNS: &[&str] = &[
    "git push --force",
    "git reset --hard",
    "git clean -fd",
    "DROP TABLE",
    "DELETE FROM",
    "npm install",  // 可能执行恶意脚本
    "curl * | sh",
    "wget * | sh",
];

pub fn classify_command(command: &str) -> CommandRiskLevel {
    let cmd_lower = command.to_lowercase();
    
    // 检查绝对危险命令
    if DANGEROUS_PATTERNS.iter().any(|p| cmd_lower.contains(p)) {
        return CommandRiskLevel::Destructive;
    }
    
    // 检查潜在危险命令
    if POTENTIALLY_RISKY_PATTERNS.iter().any(|p| cmd_lower.contains(p)) {
        return CommandRiskLevel::PotentiallyRisky;
    }
    
    // 检查是否为只读命令
    if is_readonly_command(&cmd_lower) {
        return CommandRiskLevel::Safe;
    }
    
    CommandRiskLevel::Unknown
}

fn is_readonly_command(cmd: &str) -> bool {
    const READONLY: &[&str] = &[
        "ls", "cat", "head", "tail", "grep", "find",
        "git status", "git log", "git diff",
        "cargo check", "cargo clippy", "cargo test",
    ];
    READONLY.iter().any(|c| cmd.starts_with(c))
}
```

#### 3.2.2 命令内容匹配

```rust
pub struct CommandClassifier {
    /// 自定义规则（来自配置）
    custom_rules: Vec<CustomRule>,
}

pub struct CustomRule {
    pub pattern: Regex,
    pub risk_level: CommandRiskLevel,
    pub description: String,
}

impl CommandClassifier {
    pub fn classify(&self, command: &str) -> ClassificationResult {
        // 1. 检查自定义规则
        for rule in &self.custom_rules {
            if rule.pattern.is_match(command) {
                return ClassificationResult {
                    level: rule.risk_level.clone(),
                    reason: rule.description.clone(),
                    source: "custom",
                };
            }
        }
        
        // 2. 检查内置模式
        classify_command(command)
    }
}
```

### 3.3 第三阶段：Hook 系统

```rust
// crates/yode-core/src/hooks.rs

use async_trait::async_trait;

#[async_trait]
pub trait PermissionHook: Send + Sync {
    /// Hook 名称
    fn name(&self) -> &str;
    
    /// 检查权限，返回是否需要确认
    async fn check_permission(
        &self,
        tool_name: &str,
        input: &serde_json::Value,
        context: &HookContext,
    ) -> HookResult;
}

pub struct HookContext {
    pub mode: PermissionMode,
    pub working_dir: PathBuf,
    pub recent_commands: Vec<String>,
    pub files_modified: Vec<PathBuf>,
}

pub struct HookResult {
    pub blocked: bool,
    pub require_confirm: bool,
    pub reason: Option<String>,
    pub suggestion: Option<String>,
}
```

**内置 Hook 示例：**

```rust
// 检查是否在受保护目录
pub struct ProtectedDirectoryHook {
    protected_dirs: Vec<PathBuf>,
}

#[async_trait]
impl PermissionHook for ProtectedDirectoryHook {
    async fn check_permission(
        &self,
        tool_name: &str,
        input: &serde_json::Value,
        context: &HookContext,
    ) -> HookResult {
        // 检查写入工具
        if !["write_file", "edit_file", "bash"].contains(&tool_name) {
            return HookResult::allowed();
        }
        
        // 提取目标路径
        let target_path = extract_target_path(input);
        
        // 检查是否在受保护目录
        if let Some(path) = target_path {
            for protected in &self.protected_dirs {
                if path.starts_with(protected) {
                    return HookResult {
                        blocked: true,
                        require_confirm: true,
                        reason: Some(format!(
                            "目标路径 {} 在受保护目录 {} 内",
                            path.display(),
                            protected.display()
                        )),
                        suggestion: None,
                    };
                }
            }
        }
        
        HookResult::allowed()
    }
}
```

### 3.4 第四阶段：拒绝跟踪

```rust
// crates/yode-core/src/permission.rs

use std::collections::HashMap;
use std::time::{Duration, Instant};

pub struct DenialTrackingState {
    tool_name: String,
    denial_count: u32,
    last_denial_time: Instant,
    consecutive_denials: u32,
}

pub struct DenialTracker {
    states: HashMap<String, DenialTrackingState>,
    /// 拒绝次数阈值
    thresholds: DenialThresholds,
    /// 拒绝过期时间
    expiry_duration: Duration,
}

pub struct DenialThresholds {
    default: u32,
    bash_dangerous: u32,
    file_write: u32,
}

impl Default for DenialThresholds {
    fn default() -> Self {
        Self {
            default: 3,
            bash_dangerous: 5,
            file_write: 3,
        }
    }
}

impl DenialTracker {
    pub fn new() -> Self {
        Self {
            states: HashMap::new(),
            thresholds: DenialThresholds::default(),
            expiry_duration: Duration::from_secs(30 * 60), // 30 分钟
        }
    }
    
    pub fn record_denial(&mut self, tool_name: &str, input: &serde_json::Value) {
        let state = self.states
            .entry(tool_name.to_string())
            .or_insert_with(|| DenialTrackingState {
                tool_name: tool_name.to_string(),
                denial_count: 0,
                last_denial_time: Instant::now(),
                consecutive_denials: 0,
            });
        
        state.denial_count += 1;
        state.consecutive_denials += 1;
        state.last_denial_time = Instant::now();
        
        // 清理过期状态
        self.cleanup_expired();
    }
    
    pub fn record_success(&mut self, tool_name: &str) {
        if let Some(state) = self.states.get_mut(tool_name) {
            state.consecutive_denials = 0;
        }
    }
    
    /// 检查是否应该降级为询问模式
    pub fn should_fallback_to_prompting(&self, tool_name: &str) -> bool {
        if let Some(state) = self.states.get(tool_name) {
            let threshold = self.get_threshold(tool_name);
            return state.consecutive_denials >= threshold;
        }
        false
    }
    
    fn get_threshold(&self, tool_name: &str) -> u32 {
        match tool_name {
            "bash" => self.thresholds.bash_dangerous,
            "write_file" | "edit_file" => self.thresholds.file_write,
            _ => self.thresholds.default,
        }
    }
    
    fn cleanup_expired(&mut self) {
        let now = Instant::now();
        self.states.retain(|_, state| {
            now.duration_since(state.last_denial_time) < self.expiry_duration
        });
    }
}
```

---

## 4. 配置文件设计

### 4.1 用户配置 (~/.config/yode/config.toml)

```toml
# 默认权限模式
[permissions]
default_mode = "default"

# 始终允许的工具/命令
[[permissions.always_allow]]
tool = "Read"
pattern = "**/*.md"

[[permissions.always_allow]]
tool = "Bash"
pattern = "git status*"

[[permissions.always_allow]]
tool = "Bash"
pattern = "cargo check"

# 始终拒绝的命令
[[permissions.always_deny]]
tool = "Bash"
pattern = "rm -rf /*"

[[permissions.always_deny]]
tool = "Bash"
pattern = "*--force*push*"

# 受保护目录
[[permissions.protected_directories]]
path = "/etc"
description = "系统配置目录"

[[permissions.protected_directories]]
path = "~/.ssh"
description = "SSH 密钥目录"

# 拒绝跟踪配置
[permissions.denial_tracking]
enabled = true
threshold_default = 3
threshold_bash_dangerous = 5
expiry_minutes = 30
```

### 4.2 项目配置 (.yode/config.toml)

```toml
# 项目级权限配置
[permissions]
# 项目特定的权限模式覆盖
default_mode = "plan"

# 项目特定的允许规则
[[permissions.always_allow]]
tool = "Bash"
pattern = "npm run build"

[[permissions.always_allow]]
tool = "Bash"
pattern = "cargo test"

# 项目特定的拒绝规则
[[permissions.always_deny]]
tool = "Bash"
pattern = "npm publish"
description = "禁止发布到 npm"
```

---

## 5. 实现路线图

| 阶段 | 内容 | 预计工作量 | 优先级 |
|------|------|-----------|--------|
| Phase 1 | 权限模式枚举 + 规则来源 | 2-3 天 | 高 |
| Phase 2 | Bash 命令分类器 | 3-4 天 | 高 |
| Phase 3 | Hook 系统框架 | 4-5 天 | 中 |
| Phase 4 | 拒绝跟踪 | 2-3 天 | 中 |
| Phase 5 | 配置系统增强 | 2-3 天 | 低 |

---

## 6. 测试用例

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_plan_mode_blocks_all_tools() {
        let mut pm = PermissionManager::new(PermissionMode::Plan);
        assert_eq!(pm.check("bash", &json!({"command": "ls"})), PermissionAction::Deny);
        assert_eq!(pm.check("read_file", &json!({"path": "test.rs"})), PermissionAction::Deny);
    }
    
    #[test]
    fn test_bash_classifier_dangerous_patterns() {
        let classifier = CommandClassifier::new();
        assert_eq!(
            classifier.classify("rm -rf /").level,
            CommandRiskLevel::Destructive
        );
        assert_eq!(
            classifier.classify("git push --force").level,
            CommandRiskLevel::PotentiallyRisky
        );
        assert_eq!(
            classifier.classify("git status").level,
            CommandRiskLevel::Safe
        );
    }
    
    #[test]
    fn test_denial_tracking_fallback() {
        let mut tracker = DenialTracker::new();
        
        // 连续拒绝 3 次
        for _ in 0..3 {
            tracker.record_denial("bash", &json!({"command": "rm -rf test"}));
        }
        
        assert!(tracker.should_fallback_to_prompting("bash"));
    }
    
    #[test]
    fn test_rule_priority() {
        let mut pm = PermissionManager::new(PermissionMode::Default);
        
        // 用户配置：允许
        pm.add_rule(PermissionRule {
            source: RuleSource::UserConfig,
            behavior: PermissionBehavior::Allow,
            tool_name: "Bash".to_string(),
            pattern: Some("cargo *".to_string()),
        });
        
        // CLI 参数：拒绝（优先级更高）
        pm.add_rule(PermissionRule {
            source: RuleSource::CliArg,
            behavior: PermissionBehavior::Deny,
            tool_name: "Bash".to_string(),
            pattern: Some("cargo *".to_string()),
        });
        
        // CLI 规则应该胜出
        assert_eq!(
            pm.check("bash", &json!({"command": "cargo build"})),
            PermissionAction::Deny
        );
    }
}
```

---

## 7. API 设计

### 7.1 对外暴露的 API

```rust
// crates/yode-core/src/permission.rs - 公开 API

impl PermissionManager {
    /// 创建新的权限管理器
    pub fn new(mode: PermissionMode) -> Self;
    
    /// 从配置加载规则
    pub fn load_from_config(config: &PermissionConfig) -> Result<Self>;
    
    /// 检查权限
    pub fn check(&self, tool_name: &str, input: &serde_json::Value) -> PermissionAction;
    
    /// 动态添加规则
    pub fn add_rule(&mut self, rule: PermissionRule);
    
    /// 切换权限模式
    pub fn set_mode(&mut self, mode: PermissionMode);
    
    /// 获取当前模式
    pub fn mode(&self) -> PermissionMode;
    
    /// 记录拒绝
    pub fn record_denial(&mut self, tool_name: &str, input: &serde_json::Value);
    
    /// 记录成功
    pub fn record_success(&mut self, tool_name: &str);
    
    /// 获取所有可确认的工具列表
    pub fn confirmable_tools(&self) -> Vec<&str>;
}

impl CommandClassifier {
    pub fn new() -> Self;
    pub fn classify(&self, command: &str) -> ClassificationResult;
    pub fn add_custom_rule(&mut self, rule: CustomRule);
}
```

### 7.2 TUI 集成

```rust
// crates/yode-tui/src/ui/permission_mode_selector.rs

/// 权限模式切换组件
pub struct PermissionModeSelector {
    current_mode: PermissionMode,
    modes: Vec<PermissionMode>,
    selected_index: usize,
}

impl PermissionModeSelector {
    pub fn cycle_mode(&mut self) -> PermissionMode {
        self.selected_index = (self.selected_index + 1) % self.modes.len();
        self.current_mode = self.modes[self.selected_index];
        self.current_mode
    }
    
    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let mode_str = match self.current_mode {
            PermissionMode::Default => "默认",
            PermissionMode::Plan => "计划",
            PermissionMode::Auto => "自动",
            PermissionMode::AcceptEdits => "编辑",
            PermissionMode::Bypass => "绕过",
        };
        
        // 渲染模式指示器
        let mode_text = format!("权限模式：{}", mode_str);
        // ...
    }
}
```

---

## 8. 与现有代码的集成

### 8.1 engine.rs 集成点

```rust
// crates/yode-core/src/engine.rs

impl AgentEngine {
    async fn execute_tool_call(
        &mut self,
        tool_call: &ToolCall,
    ) -> Result<ToolResult> {
        // 现有代码...
        
        // [新增] 检查权限
        let input = serde_json::from_str::<serde_json::Value>(&tool_call.arguments)?;
        
        // 1. 检查计划模式
        if self.permissions.mode() == PermissionMode::Plan {
            return Ok(ToolResult {
                content: format!("计划模式下工具 {} 不会执行", tool_call.name),
                is_error: false,
                ..Default::default()
            });
        }
        
        // 2. 检查 Bash 命令风险
        if tool_call.name == "bash" {
            let command = input.get("command")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            
            let classification = self.command_classifier.classify(command);
            
            match classification.level {
                CommandRiskLevel::Destructive => {
                    // 直接拒绝破坏性命令
                    return Ok(ToolResult {
                        content: format!("命令被拒绝：{}", classification.reason),
                        is_error: true,
                        error_type: Some(ToolErrorType::PermissionDenied),
                        ..Default::default()
                    });
                }
                CommandRiskLevel::PotentiallyRisky => {
                    // 潜在风险命令需要确认
                    // ...
                }
                _ => {}
            }
        }
        
        // 3. 检查权限
        match self.permissions.check(&tool_call.name, &input) {
            PermissionAction::Allow => {
                // 直接执行
            }
            PermissionAction::Confirm => {
                // 需要用户确认
                return Ok(ToolResult {
                    content: "等待用户确认...".to_string(),
                    is_error: false,
                    suggestion: Some("等待用户确认".to_string()),
                    ..Default::default()
                });
            }
            PermissionAction::Deny => {
                return Ok(ToolResult {
                    content: "操作被拒绝".to_string(),
                    is_error: true,
                    error_type: Some(ToolErrorType::PermissionDenied),
                    ..Default::default()
                });
            }
        }
        
        // 执行工具...
    }
}
```

---

## 9. 总结

Claude Code 的权限系统是一个多层次、可扩展的架构，核心特点：

1. **多种权限模式** - 适应不同使用场景
2. **规则优先级系统** - 灵活的配置继承
3. **智能分类器** - 自动识别危险命令
4. **拒绝跟踪** - 学习用户偏好
5. **Hook 系统** - 可扩展的权限检查

Yode 可以通过分阶段实现这些功能，从基础的权限模式开始，逐步增加分类器和 Hook 系统。
