# 成本追踪系统深度分析与优化建议

## 1. Claude Code 成本追踪架构

### 1.1 核心数据结构

Claude Code 的成本追踪系统位于 `src/cost-tracker.ts` (约 450 行)，核心数据结构：

```typescript
// 模型成本配置
type ModelCosts = {
  inputTokens: number;           // 输入 token 价格 (每百万 token)
  outputTokens: number;          // 输出 token 价格
  promptCacheWriteTokens: number; // 缓存写入价格
  promptCacheReadTokens: number;  // 缓存读取价格
  webSearchRequests: number;      // Web 搜索请求价格
}

// 使用示例
const COST_TIER_3_15 = {
  inputTokens: 3,
  outputTokens: 15,
  promptCacheWriteTokens: 3.75,
  promptCacheReadTokens: 0.3,
  webSearchRequests: 0.01,
} as const satisfies ModelCosts
```

### 1.2 模型成本配置

Claude Code 为每个模型定义了成本配置：

```typescript
// src/utils/model/configs.ts

// Sonnet 4.6 配置
export const CLAUDE_SONNET_4_6_CONFIG = {
  maxOutputTokens: 64000,
  contextWindow: 200000,
  cost: COST_TIER_3_15,  // $3 输入 / $15 输出
}

// Opus 4.6 配置 (Fast Mode)
export const CLAUDE_OPUS_4_6_CONFIG = {
  maxOutputTokens: 64000,
  contextWindow: 200000,
  cost: COST_TIER_30_150,  // $30 输入 / $150 输出 (Fast Mode)
}

// Haiku 4.5 配置
export const CLAUDE_HAIKU_4_5_CONFIG = {
  maxOutputTokens: 64000,
  contextWindow: 200000,
  cost: COST_HAIKU_45,  // $1 输入 / $5 输出
}
```

### 1.3 成本计算核心逻辑

```typescript
// src/utils/modelCost.ts

/**
 * 根据 token 使用量计算 USD 成本
 */
export function calculateUSDCost(
  usage: Usage,
  model: string,
  fastMode: boolean,
): number {
  const costs = getModelCosts(model, fastMode);
  
  const inputCost = (usage.inputTokens / 1_000_000) * costs.inputTokens;
  const outputCost = (usage.outputTokens / 1_000_000) * costs.outputTokens;
  const cacheWriteCost = (usage.cacheCreationInputTokens / 1_000_000) * costs.promptCacheWriteTokens;
  const cacheReadCost = (usage.cacheReadInputTokens / 1_000_000) * costs.promptCacheReadTokens;
  const webSearchCost = (usage.webSearchRequests || 0) * costs.webSearchRequests;
  
  return inputCost + outputCost + cacheWriteCost + cacheReadCost + webSearchCost;
}

/**
 * 获取模型的成本配置
 */
export function getModelCosts(model: string, fastMode: boolean): ModelCosts {
  const canonicalName = getCanonicalName(model);
  
  switch (canonicalName) {
    case 'claude-opus-4-6':
      return getOpus46CostTier(fastMode);  // Fast Mode 价格更高
    case 'claude-sonnet-4-6':
      return COST_TIER_3_15;
    case 'claude-haiku-4-5':
      return COST_HAIKU_45;
    default:
      return DEFAULT_UNKNOWN_MODEL_COST;
  }
}
```

### 1.4 全局状态管理

Claude Code 使用全局状态来追踪成本：

```typescript
// src/bootstrap/state.ts

// 全局成本状态
let totalCostUSD = 0;
let totalInputTokens = 0;
let totalOutputTokens = 0;
let totalCacheReadInputTokens = 0;
let totalCacheCreationInputTokens = 0;
let totalWebSearchRequests = 0;
let totalLinesAdded = 0;
let totalLinesRemoved = 0;

// 按模型分类的使用量
let modelUsage: { [modelName: string]: ModelUsage } = {};

/**
 * 添加使用量到全局状态
 */
export function addToTotalCostState(usage: Usage, model: string): void {
  const cost = calculateUSDCost(usage, model, isFastModeEnabled());
  
  totalCostUSD += cost;
  totalInputTokens += usage.inputTokens || 0;
  totalOutputTokens += usage.outputTokens || 0;
  totalCacheReadInputTokens += usage.cacheReadInputTokens || 0;
  totalCacheCreationInputTokens += usage.cacheCreationInputTokens || 0;
  totalWebSearchRequests += usage.webSearchRequests || 0;
  
  // 更新模型使用量
  if (!modelUsage[model]) {
    modelUsage[model] = {
      inputTokens: 0,
      outputTokens: 0,
      cacheReadInputTokens: 0,
      cacheCreationInputTokens: 0,
      webSearchRequests: 0,
      costUSD: 0,
      contextWindow: getContextWindowForModel(model),
      maxOutputTokens: getModelMaxOutputTokens(model).default,
    };
  }
  
  modelUsage[model].inputTokens += usage.inputTokens || 0;
  modelUsage[model].outputTokens += usage.outputTokens || 0;
  modelUsage[model].costUSD += cost;
}
```

### 1.5 会话持久化

```typescript
// src/cost-tracker.ts

/**
 * 保存当前会话成本到项目配置
 */
export function saveCurrentSessionCosts(fpsMetrics?: FpsMetrics): void {
  saveCurrentProjectConfig(current => ({
    ...current,
    lastCost: getTotalCostUSD(),
    lastAPIDuration: getTotalAPIDuration(),
    lastToolDuration: getTotalToolDuration(),
    lastLinesAdded: getTotalLinesAdded(),
    lastLinesRemoved: getTotalLinesRemoved(),
    lastTotalInputTokens: getTotalInputTokens(),
    lastTotalOutputTokens: getTotalOutputTokens(),
    lastModelUsage: Object.fromEntries(
      Object.entries(getModelUsage()).map(([model, usage]) => [
        model,
        {
          inputTokens: usage.inputTokens,
          outputTokens: usage.outputTokens,
          cacheReadInputTokens: usage.cacheReadInputTokens,
          cacheCreationInputTokens: usage.cacheCreationInputTokens,
          webSearchRequests: usage.webSearchRequests,
          costUSD: usage.costUSD,
        },
      ]),
    ),
    lastSessionId: getSessionId(),
  }));
}

/**
 * 恢复会话成本
 */
export function restoreCostStateForSession(sessionId: string): boolean {
  const data = getStoredSessionCosts(sessionId);
  if (!data) return false;
  
  setCostStateForRestore(data);
  return true;
}
```

### 1.6 成本显示格式化

```typescript
// src/cost-tracker.ts

function formatCost(cost: number, maxDecimalPlaces: number = 4): string {
  return `$${cost > 0.5 ? round(cost, 100).toFixed(2) : cost.toFixed(maxDecimalPlaces)}`;
}

function formatModelUsage(): string {
  const modelUsageMap = getModelUsage();
  
  if (Object.keys(modelUsageMap).length === 0) {
    return 'Usage: 0 input, 0 output, 0 cache read, 0 cache write';
  }
  
  return Object.entries(modelUsageMap)
    .map(([model, usage]) => {
      const cost = calculateUSDCost(
        {
          inputTokens: usage.inputTokens,
          outputTokens: usage.outputTokens,
          cacheReadInputTokens: usage.cacheReadInputTokens,
          cacheCreationInputTokens: usage.cacheCreationInputTokens,
          webSearchRequests: usage.webSearchRequests,
        },
        model,
        false
      );
      
      return `${model}: $${formatCost(cost)} (${usage.inputTokens.toLocaleString()} in, ${usage.outputTokens.toLocaleString()} out)`;
    })
    .join('\n');
}

// 输出示例：
// Total cost: $0.4523
// Usage: 125,432 input, 45,234 output, 0 cache read, 0 cache write
// 
// Model breakdown:
//   claude-sonnet-4-20250514: $0.3521 (98,234 in, 32,123 out)
//   claude-haiku-4-5: $0.1002 (27,198 in, 13,111 out)
```

### 1.7 成本阈值告警

```typescript
// src/components/CostThresholdDialog.tsx

const COST_THRESHOLDS = {
  WARNING: 5,      // $5 警告
  CRITICAL: 10,    // $10 严重警告
  MAX: 20,         // $20 最大阈值
};

function checkCostThreshold(currentCost: number): ThresholdAlert | null {
  if (currentCost >= COST_THRESHOLDS.MAX) {
    return {
      level: 'critical',
      message: `已达到最大成本阈值 ($${COST_THRESHOLDS.MAX})`,
      suggestion: '会话已停止，请开始新会话',
    };
  }
  
  if (currentCost >= COST_THRESHOLDS.CRITICAL) {
    return {
      level: 'warning',
      message: `成本已超过 $${COST_THRESHOLDS.CRITICAL}`,
      suggestion: '考虑切换到更便宜的模型',
    };
  }
  
  if (currentCost >= COST_THRESHOLDS.WARNING) {
    return {
      level: 'info',
      message: `成本已超过 $${COST_THRESHOLDS.WARNING}`,
      suggestion: null,
    };
  }
  
  return null;
}
```

---

## 2. Yode 当前成本追踪分析

### 2.1 当前状态

**Yode 目前没有成本追踪功能。**

需要从零开始实现完整的成本追踪系统。

### 2.2 现有相关代码

Yode 现有的 LLM 类型定义中已有 token 计数字段：

```rust
// crates/yode-llm/src/types.rs

pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

pub struct ChatResponse {
    pub content: String,
    pub usage: Option<Usage>,
    pub model: String,
    // ...
}
```

---

## 3. Yode 成本追踪系统设计

### 3.1 模块结构

```
crates/
└── yode-core/
    └── src/
        ├── cost.rs           # 新增：成本追踪核心
        └── config.rs         # 修改：添加成本配置
```

### 3.2 核心数据结构

```rust
// crates/yode-core/src/cost.rs

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

/// 模型成本配置（每百万 token 的 USD 价格）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCostConfig {
    /// 输入 token 价格
    pub input_cost_per_million: f64,
    /// 输出 token 价格
    pub output_cost_per_million: f64,
    /// 缓存写入价格（如果有）
    pub cache_write_cost_per_million: Option<f64>,
    /// 缓存读取价格（如果有）
    pub cache_read_cost_per_million: Option<f64>,
}

impl ModelCostConfig {
    /// 计算单次请求的成本
    pub fn calculate_cost(&self, input_tokens: u32, output_tokens: u32) -> f64 {
        let input_cost = (input_tokens as f64 / 1_000_000.0) * self.input_cost_per_million;
        let output_cost = (output_tokens as f64 / 1_000_000.0) * self.output_cost_per_million;
        input_cost + output_cost
    }
}

/// 已知模型的成本配置
pub mod model_costs {
    use super::ModelCostConfig;
    
    /// Claude 3.5 Sonnet: $3 输入 / $15 输出
    pub const CLAUDE_SONNET_3_5: ModelCostConfig = ModelCostConfig {
        input_cost_per_million: 3.0,
        output_cost_per_million: 15.0,
        cache_write_cost_per_million: Some(3.75),
        cache_read_cost_per_million: Some(0.3),
    };
    
    /// Claude 3.5 Haiku: $0.80 输入 / $4 输出
    pub const CLAUDE_HAIKU_3_5: ModelCostConfig = ModelCostConfig {
        input_cost_per_million: 0.8,
        output_cost_per_million: 4.0,
        cache_write_cost_per_million: Some(1.0),
        cache_read_cost_per_million: Some(0.08),
    };
    
    /// GPT-4o: $5 输入 / $15 输出
    pub const GPT_4O: ModelCostConfig = ModelCostConfig {
        input_cost_per_million: 5.0,
        output_cost_per_million: 15.0,
        cache_write_cost_per_million: None,
        cache_read_cost_per_million: None,
    };
    
    /// GPT-4o-mini: $0.15 输入 / $0.60 输出
    pub const GPT_4O_MINI: ModelCostConfig = ModelCostConfig {
        input_cost_per_million: 0.15,
        output_cost_per_million: 0.60,
        cache_write_cost_per_million: None,
        cache_read_cost_per_million: None,
    };
}

/// 单次请求的使用量
#[derive(Debug, Clone, Default)]
pub struct TokenUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub cache_read_tokens: Option<u32>,
    pub cache_write_tokens: Option<u32>,
}

impl TokenUsage {
    pub fn total_tokens(&self) -> u32 {
        self.input_tokens + self.output_tokens
    }
}

/// 按模型分类的使用量统计
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModelUsage {
    pub model_name: String,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_cache_read_tokens: Option<u64>,
    pub total_cache_write_tokens: Option<u64>,
    pub total_cost_usd: f64,
    pub request_count: u64,
}

impl ModelUsage {
    pub fn add_usage(&mut self, usage: &TokenUsage, cost: f64) {
        self.total_input_tokens += usage.input_tokens as u64;
        self.total_output_tokens += usage.output_tokens as u64;
        self.total_cache_read_tokens = 
            Some(self.total_cache_read_tokens.unwrap_or(0) + usage.cache_read_tokens.unwrap_or(0) as u64);
        self.total_cache_write_tokens = 
            Some(self.total_cache_write_tokens.unwrap_or(0) + usage.cache_write_tokens.unwrap_or(0) as u64);
        self.total_cost_usd += cost;
        self.request_count += 1;
    }
}

/// 全局成本追踪器
pub struct CostTracker {
    /// 总会话成本
    total_cost_usd: AtomicU64,  // 存储为美分，避免浮点误差
    
    /// 总计 token 使用
    total_input_tokens: AtomicU64,
    total_output_tokens: AtomicU64,
    total_cache_read_tokens: AtomicU64,
    total_cache_write_tokens: AtomicU64,
    
    /// 按模型分类的使用量
    model_usage: parking_lot::RwLock<HashMap<String, ModelUsage>>,
    
    /// 会话开始时间
    session_start: std::time::Instant,
    
    /// 成本阈值配置
    thresholds: CostThresholds,
}

/// 成本阈值配置
#[derive(Debug, Clone)]
pub struct CostThresholds {
    /// 警告阈值
    pub warning: f64,
    /// 严重警告阈值
    pub critical: f64,
    /// 最大阈值（超过后停止）
    pub max: f64,
}

impl Default for CostThresholds {
    fn default() -> Self {
        Self {
            warning: 5.0,
            critical: 10.0,
            max: 20.0,
        }
    }
}
```

### 3.3 成本追踪器实现

```rust
// crates/yode-core/src/cost.rs

impl CostTracker {
    pub fn new() -> Self {
        Self {
            total_cost_usd: AtomicU64::new(0),
            total_input_tokens: AtomicU64::new(0),
            total_output_tokens: AtomicU64::new(0),
            total_cache_read_tokens: AtomicU64::new(0),
            total_cache_write_tokens: AtomicU64::new(0),
            model_usage: parking_lot::RwLock::new(HashMap::new()),
            session_start: std::time::Instant::now(),
            thresholds: CostThresholds::default(),
        }
    }
    
    /// 记录一次 API 调用的使用量
    pub fn record_usage(&self, model: &str, usage: &TokenUsage, cost_config: &ModelCostConfig) {
        let cost = cost_config.calculate_cost(usage.input_tokens, usage.output_tokens);
        let cost_cents = (cost * 100.0) as u64;
        
        // 更新全局计数
        self.total_cost_usd.fetch_add(cost_cents, Ordering::Relaxed);
        self.total_input_tokens.fetch_add(usage.input_tokens as u64, Ordering::Relaxed);
        self.total_output_tokens.fetch_add(usage.output_tokens as u64, Ordering::Relaxed);
        self.total_cache_read_tokens.fetch_add(usage.cache_read_tokens.unwrap_or(0) as u64, Ordering::Relaxed);
        self.total_cache_write_tokens.fetch_add(usage.cache_write_tokens.unwrap_or(0) as u64, Ordering::Relaxed);
        
        // 更新模型使用量
        let mut model_usage = self.model_usage.write();
        let entry = model_usage.entry(model.to_string()).or_insert_with(|| ModelUsage {
            model_name: model.to_string(),
            ..Default::default()
        });
        entry.add_usage(usage, cost);
    }
    
    /// 获取总成本（USD）
    pub fn total_cost_usd(&self) -> f64 {
        self.total_cost_usd.load(Ordering::Relaxed) as f64 / 100.0
    }
    
    /// 获取总 token 使用
    pub fn total_usage(&self) -> (u64, u64) {
        (
            self.total_input_tokens.load(Ordering::Relaxed),
            self.total_output_tokens.load(Ordering::Relaxed),
        )
    }
    
    /// 获取模型使用量
    pub fn model_usage(&self) -> HashMap<String, ModelUsage> {
        self.model_usage.read().clone()
    }
    
    /// 检查是否超过阈值
    pub fn check_threshold(&self) -> Option<ThresholdAlert> {
        let cost = self.total_cost_usd();
        
        if cost >= self.thresholds.max {
            Some(ThresholdAlert {
                level: AlertLevel::Critical,
                message: format!("已达到最大成本阈值 (${})", self.thresholds.max),
                suggestion: "会话已停止，请开始新会话".to_string(),
            })
        } else if cost >= self.thresholds.critical {
            Some(ThresholdAlert {
                level: AlertLevel::Warning,
                message: format!("成本已超过 ${}", self.thresholds.critical),
                suggestion: "考虑切换到更便宜的模型".to_string(),
            })
        } else if cost >= self.thresholds.warning {
            Some(ThresholdAlert {
                level: AlertLevel::Info,
                message: format!("成本已超过 ${}", self.thresholds.warning),
                suggestion: String::new(),
            })
        } else {
            None
        }
    }
    
    /// 获取会话运行时长
    pub fn session_duration(&self) -> std::time::Duration {
        self.session_start.elapsed()
    }
    
    /// 重置计数器（用于新会话）
    pub fn reset(&self) {
        self.total_cost_usd.store(0, Ordering::Relaxed);
        self.total_input_tokens.store(0, Ordering::Relaxed);
        self.total_output_tokens.store(0, Ordering::Relaxed);
        self.total_cache_read_tokens.store(0, Ordering::Relaxed);
        self.total_cache_write_tokens.store(0, Ordering::Relaxed);
        self.model_usage.write().clear();
    }
}

/// 成本告警
#[derive(Debug, Clone)]
pub struct ThresholdAlert {
    pub level: AlertLevel,
    pub message: String,
    pub suggestion: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AlertLevel {
    Info,
    Warning,
    Critical,
}
```

### 3.4 成本格式化输出

```rust
// crates/yode-core/src/cost.rs

impl CostTracker {
    /// 格式化成本报告
    pub fn format_report(&self) -> String {
        let total_cost = self.total_cost_usd();
        let (input, output) = self.total_usage();
        let duration = self.session_duration();
        
        let mut report = String::new();
        
        report.push_str(&format!("总成本：${:.4}\n", total_cost));
        report.push_str(&format!("Token 使用：{} 输入，{} 输出\n", 
            format_number(input), format_number(output)));
        report.push_str(&format!("会话时长：{}\n", format_duration(duration)));
        
        let model_usage = self.model_usage();
        if !model_usage.is_empty() {
            report.push_str("\n模型使用详情:\n");
            for (_, usage) in model_usage {
                report.push_str(&format!(
                    "  {}: ${:.4} ({} 输入，{} 输出，{} 次请求)\n",
                    usage.model_name,
                    usage.total_cost_usd,
                    format_number(usage.total_input_tokens),
                    format_number(usage.total_output_tokens),
                    usage.request_count
                ));
            }
        }
        
        report
    }
}

fn format_number(n: u64) -> String {
    let n_str = n.to_string();
    let mut formatted = String::new();
    for (i, c) in n_str.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            formatted.push(',');
        }
        formatted.push(c);
    }
    formatted.chars().rev().collect()
}

fn format_duration(duration: std::time::Duration) -> String {
    let secs = duration.as_secs();
    if secs < 60 {
        format!("{}s", secs)
    } else if secs < 3600 {
        format!("{}m {}s", secs / 60, secs % 60)
    } else {
        format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
    }
}
```

### 3.5 与 LLM Provider 集成

```rust
// crates/yode-llm/src/provider.rs

use yode_core::cost::{TokenUsage, ModelCostConfig};

#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse>;
    
    /// 获取模型名称
    fn model_name(&self) -> &str;
    
    /// 获取成本配置
    fn cost_config(&self) -> Option<&ModelCostConfig> {
        None  // 默认未知
    }
}

// crates/yode-llm/src/providers/anthropic.rs

impl LlmProvider for AnthropicProvider {
    fn model_name(&self) -> &str {
        &self.model
    }
    
    fn cost_config(&self) -> Option<&ModelCostConfig> {
        Some(match self.model.as_str() {
            "claude-sonnet-4-20250514" => &CLAUDE_SONNET_3_5,
            "claude-haiku-4-5" => &CLAUDE_HAIKU_3_5,
            _ => &CLAUDE_SONNET_3_5,  // 默认
        })
    }
}

// crates/yode-llm/src/providers/openai.rs

impl LlmProvider for OpenAiProvider {
    fn model_name(&self) -> &str {
        &self.model
    }
    
    fn cost_config(&self) -> Option<&ModelCostConfig> {
        Some(match self.model.as_str() {
            "gpt-4o" => &GPT_4O,
            "gpt-4o-mini" => &GPT_4O_MINI,
            "gpt-4-turbo" => &GPT_4_TURBO,
            _ => &GPT_4O_MINI,  // 默认使用最便宜的
        })
    }
}
```

### 3.6 Engine 集成

```rust
// crates/yode-core/src/engine.rs

use crate::cost::{CostTracker, TokenUsage};

pub struct AgentEngine {
    provider: Arc<dyn LlmProvider>,
    // ... 其他字段
    cost_tracker: Arc<CostTracker>,
}

impl AgentEngine {
    pub fn new(
        provider: Arc<dyn LlmProvider>,
        // ... 其他参数
        cost_tracker: Arc<CostTracker>,
    ) -> Self {
        Self {
            provider,
            cost_tracker,
            // ...
        }
    }
    
    async fn run_agent_loop(&mut self) -> Result<()> {
        // ... 现有代码
        
        // 调用 LLM
        let response = self.provider.chat(request).await?;
        
        // 记录使用量
        if let Some(usage) = &response.usage {
            let token_usage = TokenUsage {
                input_tokens: usage.prompt_tokens,
                output_tokens: usage.completion_tokens,
                cache_read_tokens: None,  // 根据 provider 实现
                cache_write_tokens: None,
            };
            
            if let Some(cost_config) = self.provider.cost_config() {
                self.cost_tracker.record_usage(
                    self.provider.model_name(),
                    &token_usage,
                    cost_config,
                );
            }
            
            // 检查阈值
            if let Some(alert) = self.cost_tracker.check_threshold() {
                self.emit_event(EngineEvent::CostAlert {
                    level: alert.level,
                    message: alert.message,
                    suggestion: alert.suggestion,
                });
            }
        }
        
        // ...
    }
}
```

### 3.7 配置支持

```rust
// crates/yode-core/src/config.rs

use crate::cost::CostThresholds;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostConfig {
    /// 是否启用成本追踪
    pub enabled: bool,
    
    /// 成本阈值
    pub thresholds: CostThresholds,
    
    /// 是否在每次响应后显示成本
    pub show_after_response: bool,
}

impl Default for CostConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            thresholds: CostThresholds::default(),
            show_after_response: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    // ... 现有字段
    pub cost: CostConfig,
}
```

---

## 4. TUI 集成

### 4.1 状态栏成本显示

```rust
// crates/yode-tui/src/ui/status_bar.rs

use yode_core::cost::CostTracker;

pub struct StatusBar {
    // ... 现有字段
    cost_tracker: Option<Arc<CostTracker>>,
    show_cost: bool,
}

impl StatusBar {
    pub fn render(&self, frame: &mut Frame, area: Rect) {
        // ... 现有代码
        
        if self.show_cost {
            if let Some(tracker) = &self.cost_tracker {
                let cost_text = format!("  成本：${:.4}", tracker.total_cost_usd());
                // 渲染到状态栏右侧
            }
        }
    }
}
```

### 4.2 /cost 命令

```rust
// crates/yode-tui/src/commands/cost.rs

use yode_core::cost::CostTracker;

pub fn show_cost_report(tracker: &CostTracker) -> String {
    tracker.format_report()
}

// 示例输出：
// 总成本：$0.4523
// Token 使用：125,432 输入，45,234 输出
// 会话时长：15m 32s
//
// 模型使用详情:
//   claude-sonnet-4-20250514: $0.3521 (98,234 输入，32,123 输出，12 次请求)
//   claude-haiku-4-5: $0.1002 (27,198 输入，13,111 输出，5 次请求)
```

---

## 5. 配置文件设计

```toml
# ~/.config/yode/config.toml

[cost]
# 启用成本追踪
enabled = true

# 成本阈值
[cost.thresholds]
warning = 5.0    # $5 警告
critical = 10.0  # $10 严重警告
max = 20.0       # $20 最大阈值

# 是否在每次响应后显示成本
show_after_response = false

# 自定义模型成本（可选）
[cost.custom_models]
"my-custom-model" = { input_cost = 2.0, output_cost = 8.0 }
```

---

## 6. 总结

Claude Code 的成本追踪系统特点：

1. **精确到模型** - 每个模型有独立的成本配置
2. **实时更新** - 每次 API 调用后立即更新
3. **会话持久化** - 成本保存到项目配置
4. **阈值告警** - 多级阈值提醒
5. **详细报告** - 按模型分类的使用详情

Yode 需要从零实现，但可以借鉴这个设计，核心是：
- 模型成本配置表
- 全局使用量追踪
- 成本计算和格式化
- 阈值告警机制
