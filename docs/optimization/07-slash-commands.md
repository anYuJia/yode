# Slash Commands 系统深度分析与优化建议

## 1. Claude Code Slash Commands 架构

### 1.1 Commands 目录结构

Claude Code 有 85+ 个内置 commands：

```
src/commands/
├── add-dir/              # 添加工作目录
├── advisor.ts            # 顾问模式
├── agents/               # 代理管理
├── autofix-pr/           # 自动修复 PR
├── branch/               # 分支管理
├── brief.ts              # 生成简报
├── bughunter/            # Bug 狩猎
├── chrome/               # Chrome 集成
├── clear/                # 清屏
├── commit-push-pr.ts     # 提交并推送 PR
├── commit.ts             # Git 提交
├── compact/              # 压缩上下文
├── copy/                 # 复制到剪贴板
├── cost/                 # 成本查询
├── describe-image/       # 图片描述
├── diff/                 # Git diff
├── editor/               # 编辑器集成
├── extensions/           # 扩展管理
├── git/                  # Git 命令
├── help/                 # 帮助
├── history/              # 历史记录
├── init/                 # 初始化项目
├── integration-test/     # 集成测试
├── keybindings/          # 快捷键
├── login/                # 登录
├── logout/               # 登出
├── md/                   # Markdown 工具
├── model/                # 模型切换
├── notebook/             # Notebook 工具
├── permissions/          # 权限管理
├── plan/                 # 计划模式
├── projectbrief/         # 项目简报
├── providers/            # 提供商管理
├── resume/               # 恢复会话
├── review/               # 代码审查
├── skill/                # 技能管理
├── status/               # 状态显示
├── todos/                # Todo 管理
├── tools/                # 工具列表
├── tree/                 # 目录树
├── usage/                # 使用统计
├── version/              # 版本信息
└── ... (更多)
```

### 1.2 Command 类型定义

```typescript
// src/commands.ts

export type Command = {
  name: string;           // 命令名称（不含 /）
  description: string;    // 命令描述
  category: CommandCategory;
  
  // 命令实现
  action: (
    args: string[],
    context: CommandContext,
  ) => Promise<CommandResult>;
  
  // 自动完成
  completion?: (
    partialArg: string,
    context: CommandContext,
  ) => Promise<string[]>;
  
  // 参数定义（用于验证）
  args?: CommandArgDefinition[];
  
  // 是否显示在帮助中
  showInHelp?: boolean;
  
  // 别名
  aliases?: string[];
};

export type CommandCategory =
  | 'general'           // 通用命令
  | 'git'               // Git 相关
  | 'tools'             // 工具相关
  | 'session'           // 会话管理
  | 'settings'          // 设置相关
  | 'debug'             // 调试命令
  | 'extension';        // 扩展命令

export type CommandContext = {
  sessionId: string;
  workingDir: string;
  model: string;
  // ... 更多上下文
};

export type CommandResult = {
  output: string;       // 命令输出
  shouldSendToModel?: boolean;  // 是否发送给模型
};
```

### 1.3 命令注册与执行

```typescript
// src/commands.ts

class CommandRegistry {
  private commands: Map<string, Command> = new Map();
  
  register(command: Command): void {
    this.commands.set(command.name, command);
    
    // 注册别名
    if (command.aliases) {
      for (const alias of command.aliases) {
        this.commands.set(alias, command);
      }
    }
  }
  
  get(name: string): Command | undefined {
    return this.commands.get(name);
  }
  
  list(category?: CommandCategory): Command[] {
    const all = Array.from(this.commands.values());
    if (!category) return all;
    return all.filter(cmd => cmd.category === category);
  }
  
  async execute(
    name: string,
    args: string[],
    context: CommandContext,
  ): Promise<CommandResult> {
    const command = this.get(name);
    
    if (!command) {
      return {
        output: `Unknown command: /${name}\nUse /help for available commands.`,
      };
    }
    
    try {
      return await command.action(args, context);
    } catch (error) {
      return {
        output: `Error executing /${name}: ${error.message}`,
      };
    }
  }
}
```

### 1.4 常用命令实现示例

#### /help 命令

```typescript
// src/commands/help.ts

const helpCommand: Command = {
  name: 'help',
  description: 'Show available commands',
  category: 'general',
  
  action: async (args, context) => {
    const registry = getCommandRegistry();
    
    // 按类别分组
    const byCategory = registry.list().reduce((acc, cmd) => {
      if (!cmd.showInHelp) return acc;
      
      const cat = acc.get(cmd.category) || [];
      cat.push(cmd);
      acc.set(cmd.category, cat);
      return acc;
    }, new Map<CommandCategory, Command[]>());
    
    let output = 'Available commands:\n\n';
    
    for (const [category, commands] of byCategory) {
      output += `${formatCategoryName(category)}:\n`;
      
      for (const cmd of commands.sort((a, b) => 
        a.name.localeCompare(b.name)
      )) {
        output += `  /${cmd.name.padEnd(20)} ${cmd.description}\n`;
      }
      
      output += '\n';
    }
    
    output += 'Use /help <command> for detailed help on a specific command.';
    
    return { output };
  },
};
```

#### /cost 命令

```typescript
// src/commands/cost.ts

const costCommand: Command = {
  name: 'cost',
  description: 'Show token usage and estimated cost',
  category: 'session',
  
  action: async (args, context) => {
    const cost = getTotalCostUSD();
    const tokens = getTokenUsage();
    const modelUsage = getModelUsage();
    
    let output = `Total cost: ${formatCost(cost)}\n\n`;
    output += `Token usage:\n`;
    output += `  Input:  ${formatNumber(tokens.input)}\n`;
    output += `  Output: ${formatNumber(tokens.output)}\n`;
    
    if (Object.keys(modelUsage).length > 0) {
      output += '\nBy model:\n';
      
      for (const [model, usage] of Object.entries(modelUsage)) {
        output += `  ${model}:\n`;
        output += `    Cost: ${formatCost(usage.costUSD)}\n`;
        output += `    Input: ${formatNumber(usage.inputTokens)}\n`;
        output += `    Output: ${formatNumber(usage.outputTokens)}\n`;
      }
    }
    
    return { output };
  },
};
```

#### /compact 命令

```typescript
// src/commands/compact.ts

const compactCommand: Command = {
  name: 'compact',
  description: 'Compact chat history to save tokens',
  category: 'session',
  
  action: async (args, context) => {
    const messages = getMessages();
    
    if (messages.length < 10) {
      return {
        output: 'Not enough messages to compact.',
      };
    }
    
    // 保留系统消息和最近 10 条
    const systemMessage = messages[0];
    const recentMessages = messages.slice(-10);
    
    // 压缩中间部分
    const middleMessages = messages.slice(1, -10);
    const summary = await summarizeMessages(middleMessages);
    
    // 构建压缩后的消息
    const compressed = [
      systemMessage,
      {
        role: 'assistant',
        content: `[Previous conversation summary]\n\n${summary}`,
      },
      ...recentMessages,
    ];
    
    setMessages(compressed);
    
    return {
      output: `Compacted ${middleMessages.length} messages into a summary.`,
      shouldSendToModel: true,
    };
  },
};
```

#### /model 命令

```typescript
// src/commands/model.ts

const modelCommand: Command = {
  name: 'model',
  description: 'Show or change current model',
  category: 'settings',
  aliases: ['m'],
  
  action: async (args, context) => {
    if (args.length === 0) {
      // 显示当前模型
      return {
        output: `Current model: ${context.model}`,
      };
    }
    
    // 切换模型
    const newModel = args.join(' ');
    const availableModels = getAvailableModels();
    
    const model = availableModels.find(m => 
      m.name.toLowerCase() === newModel.toLowerCase()
    );
    
    if (!model) {
      return {
        output: `Unknown model: ${newModel}\nAvailable: ${availableModels.map(m => m.name).join(', ')}`,
      };
    }
    
    setModel(model.name);
    
    return {
      output: `Model changed to: ${model.name}`,
      shouldSendToModel: false,
    };
  },
  
  completion: async (partial) => {
    const models = getAvailableModels();
    return models
      .map(m => m.name)
      .filter(name => name.toLowerCase().includes(partial.toLowerCase()));
  },
};
```

---

## 2. Yode 当前 Slash Commands 分析

### 2.1 当前命令列表

Yode 当前支持的命令（根据 README.md）：

| 命令 | 描述 | 状态 |
|------|------|------|
| `/help` | 显示所有命令 | ✅ |
| `/keys` | 快捷键参考 | ✅ |
| `/clear` | 清屏 | ✅ |
| `/model` | 显示当前模型 | ✅ |
| `/provider` | 切换 LLM 提供商 | ✅ |
| `/providers` | 列出可用提供商 | ✅ |
| `/tools` | 列出工具 | ✅ |
| `/cost` | 显示 token 使用 | ❌ (未实现) |
| `/diff` | Git diff 统计 | ✅ |
| `/status` | 会话状态 | ✅ |
| `/context` | 上下文使用 | ✅ |
| `/compact` | 压缩历史 | ✅ |
| `/copy` | 复制回复 | ✅ |
| `/sessions` | 历史会话 | ❌ (未实现) |
| `/bug` | 生成 bug 报告 | ✅ |
| `/doctor` | 环境检查 | ✅ |
| `/config` | 显示配置 | ✅ |
| `/version` | 版本信息 | ✅ |

### 2.2 命令实现位置

```rust
// crates/yode-tui/src/commands/mod.rs

pub mod clear;
pub mod compact;
pub mod config;
pub mod context;
pub mod copy;
pub mod diff;
pub mod doctor;
pub mod help;
pub mod keys;
pub mod model;
pub mod provider;
pub mod status;
pub mod tools;
pub mod version;
```

---

## 3. 优化建议：新增命令

### 3.1 第一阶段：常用命令

#### 3.1.1 /cost 命令

```rust
// crates/yode-tui/src/commands/cost.rs

use yode_core::cost::CostTracker;

pub fn execute(tracker: &CostTracker) -> String {
    let total_cost = tracker.total_cost_usd();
    let (input, output) = tracker.total_usage();
    let model_usage = tracker.model_usage();
    
    let mut output = String::new();
    
    output.push_str(&format!("总成本：${:.4}\n\n", total_cost));
    output.push_str("Token 使用:\n");
    output.push_str(&format!("  输入：{} {}\n", format_number(input), format_token(input)));
    output.push_str(&format!("  输出：{} {}\n", format_number(output), format_token(output)));
    
    if !model_usage.is_empty() {
        output.push_str("\n按模型:\n");
        
        for (_, usage) in model_usage {
            output.push_str(&format!(
                "  {}:\n    成本：${:.4}\n    输入：{}\n    输出：{}\n",
                usage.model_name,
                usage.total_cost_usd,
                format_number(usage.total_input_tokens),
                format_number(usage.total_output_tokens),
            ));
        }
    }
    
    output
}

fn format_number(n: u64) -> String {
    let s = n.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result.chars().rev().collect()
}

fn format_token(n: u64) -> &'static str {
    if n >= 1_000_000 {
        "M"
    } else if n >= 1_000 {
        "K"
    } else {
        ""
    }
}
```

#### 3.1.2 /sessions 命令

```rust
// crates/yode-tui/src/commands/sessions.rs

use yode_core::session::{SessionSummary, Database};

pub fn execute(db: &Database) -> String {
    let summaries = match db.list_recent_sessions(10) {
        Ok(s) => s,
        Err(e) => return format!("错误：{}", e),
    };
    
    if summaries.is_empty() {
        return "没有历史会话".to_string();
    }
    
    let mut output = String::new();
    output.push_str("最近会话:\n\n");
    
    for (i, summary) in summaries.iter().enumerate() {
        let time_ago = format_duration_ago(&summary.updated_at);
        
        output.push_str(&format!(
            "{}. {}\n",
            i + 1,
            summary.id
        ));
        output.push_str(&format!("   时间：{}\n", time_ago));
        output.push_str(&format!("   消息：{} 条 | 模型：{}\n", 
            summary.message_count, summary.model));
        output.push_str(&format!("   预览：{}\n", 
            truncate(&summary.first_message_preview, 50)));
        output.push_str("\n");
    }
    
    output.push_str("使用 /resume <session-id> 恢复会话\n");
    
    output
}

fn format_duration_ago(dt: &chrono::DateTime<chrono::Utc>) -> String {
    let now = chrono::Utc::now();
    let duration = now.signed_duration_since(*dt);
    
    if duration.num_seconds() < 60 {
        format!("{}秒前", duration.num_seconds())
    } else if duration.num_minutes() < 60 {
        format!("{}分钟前", duration.num_minutes())
    } else if duration.num_hours() < 24 {
        format!("{}小时前", duration.num_hours())
    } else {
        format!("{}天前", duration.num_days())
    }
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() > max_len {
        format!("{}...", &s[..max_len])
    } else {
        s.to_string()
    }
}
```

#### 3.1.3 /resume 命令

```rust
// crates/yode-tui/src/commands/resume.rs

pub fn execute(session_id: &str) -> String {
    // 验证会话 ID 是否存在
    let db = get_database();
    
    match db.load_session(session_id) {
        Ok(Some(_session)) => {
            // 保存当前会话（如果有）
            save_current_session();
            
            // 切换到新会话
            switch_session(session_id);
            
            format!("已恢复到会话：{}", session_id)
        }
        Ok(None) => {
            format!("未找到会话：{}\n使用 /sessions 查看历史会话", session_id)
        }
        Err(e) => {
            format!("错误：{}", e)
        }
    }
}
```

### 3.2 第二阶段：Git 相关命令

#### 3.2.1 /branch 命令

```rust
// crates/yode-tui/src/commands/branch.rs

use std::process::Command;

pub fn execute() -> String {
    let git_cmd = if cfg!(windows) { "git.exe" } else { "git" };
    
    // 当前分支
    let current = Command::new(git_cmd)
        .args(["branch", "--show-current"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "(unknown)".to_string());
    
    // 所有本地分支
    let branches = Command::new(git_cmd)
        .args(["branch"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_default();
    
    let mut output = String::new();
    output.push_str(&format!("当前分支：{}\n\n", current));
    output.push_str("本地分支:\n");
    output.push_str(&branches);
    
    output
}
```

#### 3.2.2 /log 命令

```rust
// crates/yode-tui/src/commands/log.rs

use std::process::Command;

pub fn execute(limit: usize) -> String {
    let git_cmd = if cfg!(windows) { "git.exe" } else { "git" };
    
    let output = Command::new(git_cmd)
        .args(["log", "--oneline", &format!("-{}", limit)])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_else(|| "无法获取 git log".to_string());
    
    format!("最近 {} 条提交:\n\n{}", limit, output)
}
```

### 3.3 第三阶段：工具相关命令

#### 3.3.1 /mcp 命令

```rust
// crates/yode-tui/src/commands/mcp.rs

use yode_mcp::McpManager;

pub fn list_servers(manager: &McpManager) -> String {
    let servers = manager.list_servers();
    
    if servers.is_empty() {
        return "没有配置的 MCP 服务器".to_string();
    }
    
    let mut output = String::new();
    output.push_str("MCP 服务器:\n\n");
    
    for server in servers {
        let status = if manager.is_connected(&server.name) {
            "🟢 已连接"
        } else {
            "🔴 未连接"
        };
        
        output.push_str(&format!("{}: {}\n", server.name, status));
        output.push_str(&format!("  命令：{}\n", server.command));
        output.push_str(&format!("  工具：{} 个\n", server.tool_count));
        output.push_str("\n");
    }
    
    output
}

pub fn connect(manager: &McpManager, name: &str) -> String {
    match manager.connect(name) {
        Ok(_) => format!("已连接到 MCP 服务器：{}", name),
        Err(e) => format!("连接失败：{}", e),
    }
}

pub fn disconnect(manager: &McpManager, name: &str) -> String {
    match manager.disconnect(name) {
        Ok(_) => format!("已断开 MCP 服务器：{}", name),
        Err(e) => format!("断开失败：{}", e),
    }
}
```

#### 3.3.2 /tools 增强

```rust
// crates/yode-tui/src/commands/tools.rs

use yode_tools::registry::ToolRegistry;

pub fn execute(registry: &ToolRegistry, verbose: bool) -> String {
    let tools = registry.list();
    let deferred = registry.list_deferred();
    
    let mut output = String::new();
    
    // 活跃工具
    output.push_str(&format!("活跃工具 ({} 个):\n\n", tools.len()));
    
    for tool in tools {
        if verbose {
            output.push_str(&format!(
                "  /{} \n    {}\n    参数：{}\n\n",
                tool.name(),
                tool.description(),
                tool.parameters_schema(),
            ));
        } else {
            output.push_str(&format!("  {}\n", tool.name()));
        }
    }
    
    // 延迟工具
    if !deferred.is_empty() {
        output.push_str(&format!("\n延迟工具 ({} 个，使用 /tool_search 激活):\n\n", deferred.len()));
        
        for (name, _tool) in deferred {
            output.push_str(&format!("  {}\n", name));
        }
    }
    
    output
}
```

### 3.4 第四阶段：配置命令

#### 3.4.1 /set 命令

```rust
// crates/yode-tui/src/commands/set.rs

use yode_core::config::Config;

pub fn execute(key: &str, value: Option<&str>) -> String {
    let mut config = match Config::load() {
        Ok(c) => c,
        Err(e) => return format!("加载配置失败：{}", e),
    };
    
    if value.is_none() {
        // 读取配置
        return get_config_value(&config, key);
    }
    
    // 设置配置
    match set_config_value(&mut config, key, value.unwrap()) {
        Ok(_) => {
            if let Err(e) = config.save() {
                return format!("保存配置失败：{}", e);
            }
            format!("已设置 {} = {}", key, value.unwrap())
        }
        Err(e) => format!("错误：{}", e),
    }
}

fn get_config_value(config: &Config, key: &str) -> String {
    match key {
        "provider.default" => format!("provider.default = {}", config.llm.default_provider),
        "provider.model" => format!("provider.model = {}", config.llm.default_model),
        "permissions.mode" => format!("permissions.mode = {:?}", config.permissions.mode),
        _ => format!("未知配置项：{}", key),
    }
}

fn set_config_value(config: &mut Config, key: &str, value: &str) -> Result<(), String> {
    match key {
        "provider.default" => {
            config.llm.default_provider = value.to_string();
        }
        "provider.model" => {
            config.llm.default_model = value.to_string();
        }
        "permissions.mode" => {
            config.permissions.mode = parse_permission_mode(value)
                .ok_or_else(|| format!("无效的权限模式：{}", value))?;
        }
        _ => return Err(format!("未知配置项：{}", key)),
    }
    Ok(())
}
```

---

## 4. 命令自动完成系统

### 4.1 自动完成框架

```rust
// crates/yode-tui/src/completion.rs

use std::collections::HashMap;

pub type CompletionFn = Box<dyn Fn(&str) -> Vec<String> + Send + Sync>;

pub struct CompletionRegistry {
    completions: HashMap<String, CompletionFn>,
}

impl CompletionRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            completions: HashMap::new(),
        };
        
        registry.register_builtins();
        registry
    }
    
    pub fn register(&mut self, command: &str, completion: CompletionFn) {
        self.completions.insert(command.to_string(), completion);
    }
    
    pub fn complete(&self, command: &str, partial: &str) -> Vec<String> {
        if let Some(completion_fn) = self.completions.get(command) {
            completion_fn(partial)
        } else {
            Vec::new()
        }
    }
    
    fn register_builtins(&mut self) {
        // /model 自动完成
        self.register("model", Box::new(|partial| {
            get_available_models()
                .into_iter()
                .filter(|m| m.to_lowercase().contains(&partial.to_lowercase()))
                .collect()
        }));
        
        // /provider 自动完成
        self.register("provider", Box::new(|partial| {
            get_configured_providers()
                .into_iter()
                .filter(|p| p.to_lowercase().contains(&partial.to_lowercase()))
                .collect()
        }));
        
        // /resume 自动完成
        self.register("resume", Box::new(|partial| {
            get_recent_session_ids()
                .into_iter()
                .filter(|id| id.starts_with(partial))
                .collect()
        }));
    }
}
```

---

## 5. 完整命令列表建议

| 命令 | 描述 | 优先级 |
|------|------|--------|
| `/cost` | 显示 token 使用和成本 | 高 |
| `/sessions` | 列出历史会话 | 高 |
| `/resume` | 恢复会话 | 高 |
| `/branch` | 显示 git 分支 | 中 |
| `/log [n]` | 显示 git log | 中 |
| `/mcp` | MCP 服务器管理 | 中 |
| `/set <key> [value]` | 配置管理 | 中 |
| `/tree [dir]` | 显示目录树 | 低 |
| `/env` | 显示环境变量 | 低 |
| `/clear` | 清屏 | 已有 |
| `/compact` | 压缩上下文 | 已有 |
| `/copy` | 复制回复 | 已有 |

---

## 6. 总结

Claude Code 命令系统特点：

1. **丰富的命令集** - 85+ 个内置命令
2. **分类管理** - 按类别组织命令
3. **自动完成** - 命令参数智能提示
4. **可组合** - 命令可以组合使用

Yode 优化建议：
1. 实现 `/cost` 命令
2. 实现 `/sessions` 和 `/resume` 命令
3. 添加 Git 相关命令
4. 实现 `/set` 配置命令
5. 添加自动完成系统
