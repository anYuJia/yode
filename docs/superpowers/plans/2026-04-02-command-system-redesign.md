# Command System Redesign — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace Yode's monolithic command system with a trait-based modular architecture, add argument completion, and introduce `/effort` and `/permissions` commands.

**Architecture:** Each slash command becomes a struct implementing a `Command` trait with metadata (name, aliases, args, category) and an `execute()` method. A `CommandRegistry` handles registration, lookup, and completion. `CommandContext` provides mutable access to app state; `CompletionContext` provides read-only access for keystroke-driven completions.

**Tech Stack:** Rust, tokio (Arc<Mutex<>>), ratatui TUI

**Key Design Decision:** `Command::execute()` is **synchronous** (not async), matching the current synchronous `handle_key_event` → `handle_enter` → `handle_slash_command` call chain. Commands access the engine via `engine.try_lock()` (non-blocking), consistent with the existing pattern. This avoids the need to make the entire key handler chain async.

**Spec:** `docs/superpowers/specs/2026-04-02-command-system-redesign-design.md`

---

## File Map

### New files to create:
| File | Responsibility |
|------|---------------|
| `crates/yode-tui/src/commands/mod.rs` | Command trait, types (CommandMeta, ArgDef, CommandCategory, CommandOutput, CommandResult), `register_all()` |
| `crates/yode-tui/src/commands/context.rs` | CommandContext, CompletionContext structs |
| `crates/yode-tui/src/commands/registry.rs` | CommandRegistry: register, find, complete_command, complete_args, suggest_similar |
| `crates/yode-tui/src/commands/session/mod.rs` | Re-exports for session commands |
| `crates/yode-tui/src/commands/session/clear.rs` | ClearCommand |
| `crates/yode-tui/src/commands/session/compact.rs` | CompactCommand |
| `crates/yode-tui/src/commands/session/sessions.rs` | SessionsCommand |
| `crates/yode-tui/src/commands/session/exit.rs` | ExitCommand |
| `crates/yode-tui/src/commands/model/mod.rs` | Re-exports for model commands |
| `crates/yode-tui/src/commands/model/model.rs` | ModelCommand (with dynamic arg completion) |
| `crates/yode-tui/src/commands/model/provider.rs` | ProviderCommand (with dynamic arg completion) |
| `crates/yode-tui/src/commands/model/providers.rs` | ProvidersCommand |
| `crates/yode-tui/src/commands/model/effort.rs` | EffortCommand (NEW) |
| `crates/yode-tui/src/commands/tools/mod.rs` | Re-exports for tools commands |
| `crates/yode-tui/src/commands/tools/tools.rs` | ToolsCommand |
| `crates/yode-tui/src/commands/tools/permissions.rs` | PermissionsCommand (NEW) |
| `crates/yode-tui/src/commands/info/mod.rs` | Re-exports for info commands |
| `crates/yode-tui/src/commands/info/help.rs` | HelpCommand (uses registry.by_category()) |
| `crates/yode-tui/src/commands/info/status.rs` | StatusCommand |
| `crates/yode-tui/src/commands/info/cost.rs` | CostCommand (includes estimate_cost helper) |
| `crates/yode-tui/src/commands/info/version.rs` | VersionCommand |
| `crates/yode-tui/src/commands/info/config.rs` | ConfigCommand |
| `crates/yode-tui/src/commands/info/context_cmd.rs` | ContextCommand (named context_cmd.rs to avoid clash with context.rs) |
| `crates/yode-tui/src/commands/info/doctor.rs` | DoctorCommand |
| `crates/yode-tui/src/commands/dev/mod.rs` | Re-exports for dev commands |
| `crates/yode-tui/src/commands/dev/diff.rs` | DiffCommand |
| `crates/yode-tui/src/commands/dev/bug.rs` | BugCommand |
| `crates/yode-tui/src/commands/utility/mod.rs` | Re-exports for utility commands |
| `crates/yode-tui/src/commands/utility/copy.rs` | CopyCommand |
| `crates/yode-tui/src/commands/utility/keys.rs` | KeysCommand |
| `crates/yode-tui/src/commands/utility/history.rs` | HistoryCommand |
| `crates/yode-tui/src/commands/utility/time.rs` | TimeCommand |

### Files to modify:
| File | Changes |
|------|---------|
| `crates/yode-core/src/engine.rs` | Add EffortLevel enum, effort field, set_effort/effort/current_model/current_provider methods |
| `crates/yode-core/src/context.rs` | Add `effort: EffortLevel` field to AgentContext |
| `crates/yode-core/src/permission.rs` | Add `allow()`, `deny()`, `reset()`, `list_confirmable()` methods to PermissionManager |
| `crates/yode-core/src/lib.rs` | Re-export EffortLevel |
| `crates/yode-tui/src/app/mod.rs` | Add `cmd_registry: CommandRegistry` field, replace handle_slash_command call, update completion update logic |
| `crates/yode-tui/src/app/completion.rs` | Remove SLASH_COMMANDS, update CommandCompletion to use CommandRegistry as data source |
| `crates/yode-tui/src/lib.rs` | Add `pub mod commands;` |

### Files to delete (after migration):
| File | Reason |
|------|--------|
| `crates/yode-tui/src/app/commands.rs` | Replaced by `commands/` module entirely |

---

## Task 1: Engine API Extensions (EffortLevel + PermissionManager)

**Files:**
- Modify: `crates/yode-core/src/engine.rs:104-131` (struct), `248-256` (methods)
- Modify: `crates/yode-core/src/context.rs:6-17`
- Modify: `crates/yode-core/src/permission.rs:14-53`
- Modify: `crates/yode-core/src/lib.rs`

- [ ] **Step 1: Add EffortLevel to context.rs**

In `crates/yode-core/src/context.rs`, add before `AgentContext`:

```rust
/// Controls AI thinking depth for the current session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffortLevel {
    Min,
    Low,
    Medium,
    High,
    Max,
}

impl std::fmt::Display for EffortLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Min => write!(f, "min"),
            Self::Low => write!(f, "low"),
            Self::Medium => write!(f, "medium"),
            Self::High => write!(f, "high"),
            Self::Max => write!(f, "max"),
        }
    }
}

impl std::str::FromStr for EffortLevel {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "min" => Ok(Self::Min),
            "low" => Ok(Self::Low),
            "medium" => Ok(Self::Medium),
            "high" => Ok(Self::High),
            "max" => Ok(Self::Max),
            _ => Err(format!("Unknown effort level: {s}. Valid: min, low, medium, high, max")),
        }
    }
}
```

Add `effort` field to `AgentContext`:
```rust
pub struct AgentContext {
    pub session_id: String,
    pub working_dir: PathBuf,
    pub model: String,
    pub provider: String,
    pub is_resumed: bool,
    pub effort: EffortLevel,  // NEW
}
```

Update any `AgentContext` construction sites (search for `AgentContext {`) to include `effort: EffortLevel::Medium`.

- [ ] **Step 2: Add engine methods in engine.rs**

In `crates/yode-core/src/engine.rs`, after the existing `set_provider` method (line 256), add:

```rust
pub fn set_effort(&mut self, level: EffortLevel) {
    self.context.effort = level;
}

pub fn effort(&self) -> EffortLevel {
    self.context.effort
}

pub fn current_model(&self) -> &str {
    &self.context.model
}

pub fn current_provider(&self) -> &str {
    &self.context.provider
}

pub fn permissions(&self) -> &PermissionManager {
    &self.permissions
}

pub fn permissions_mut(&mut self) -> &mut PermissionManager {
    &mut self.permissions
}
```

- [ ] **Step 3: Extend PermissionManager in permission.rs**

In `crates/yode-core/src/permission.rs`, add these methods to `PermissionManager` impl block (before the closing `}`):

```rust
/// Allow a tool to run without confirmation.
pub fn allow(&mut self, tool_name: &str) {
    self.require_confirmation.remove(tool_name);
}

/// Require confirmation for a tool (add to confirmation list).
pub fn deny(&mut self, tool_name: &str) {
    self.require_confirmation.insert(tool_name.to_string());
}

/// Reset to given defaults.
pub fn reset(&mut self, defaults: Vec<String>) {
    self.require_confirmation = defaults.into_iter().collect();
}

/// List all tools that currently require confirmation.
pub fn confirmable_tools(&self) -> Vec<&str> {
    let mut tools: Vec<&str> = self.require_confirmation.iter().map(|s| s.as_str()).collect();
    tools.sort();
    tools
}
```

- [ ] **Step 4: Re-export EffortLevel from lib.rs**

In `crates/yode-core/src/lib.rs`, ensure `EffortLevel` is accessible. Add if needed:

```rust
pub use context::EffortLevel;
```

- [ ] **Step 5: Add permission tests**

In `crates/yode-core/src/permission.rs`, add to the existing `#[cfg(test)] mod tests` block:

```rust
#[test]
fn test_allow_removes_confirmation() {
    let mut pm = PermissionManager::strict();
    assert_eq!(pm.check("bash"), PermissionAction::Confirm);
    pm.allow("bash");
    assert_eq!(pm.check("bash"), PermissionAction::Allow);
}

#[test]
fn test_deny_adds_confirmation() {
    let mut pm = PermissionManager::permissive();
    assert_eq!(pm.check("bash"), PermissionAction::Allow);
    pm.deny("bash");
    assert_eq!(pm.check("bash"), PermissionAction::Confirm);
}

#[test]
fn test_reset_restores_defaults() {
    let mut pm = PermissionManager::strict();
    pm.allow("bash");
    pm.reset(vec!["bash".into(), "write_file".into(), "edit_file".into()]);
    assert_eq!(pm.check("bash"), PermissionAction::Confirm);
}

#[test]
fn test_confirmable_tools_sorted() {
    let pm = PermissionManager::strict();
    let tools = pm.confirmable_tools();
    assert!(tools.contains(&"bash"));
    assert!(tools.contains(&"edit_file"));
    // Verify sorted
    let mut sorted = tools.clone();
    sorted.sort();
    assert_eq!(tools, sorted);
}
```

- [ ] **Step 6: Run tests**

Run: `cargo test -p yode-core`
Expected: All existing tests + 4 new tests pass.

- [ ] **Step 7: Commit**

```bash
git add crates/yode-core/src/context.rs crates/yode-core/src/engine.rs crates/yode-core/src/permission.rs crates/yode-core/src/lib.rs
git commit -m "feat(core): add EffortLevel, engine accessors, and PermissionManager methods"
```

---

## Task 2: Command Trait & Core Types

**Files:**
- Create: `crates/yode-tui/src/commands/mod.rs`
- Create: `crates/yode-tui/src/commands/context.rs`
- Modify: `crates/yode-tui/src/lib.rs` (add `pub mod commands;`)

- [ ] **Step 1: Create commands/mod.rs with core types**

Create `crates/yode-tui/src/commands/mod.rs`:

```rust
pub mod context;
pub mod registry;

pub mod session;
pub mod model;
pub mod tools;
pub mod info;
pub mod dev;
pub mod utility;

use context::{CommandContext, CompletionContext};

/// How a command argument gets its completions.
pub enum ArgCompletionSource {
    /// No completions available
    None,
    /// Fixed list of valid values
    Static(Vec<String>),
    /// Generated at runtime from read-only app state
    Dynamic(fn(&CompletionContext) -> Vec<String>),
}

/// Defines one positional argument for a command.
pub struct ArgDef {
    pub name: String,
    pub required: bool,
    pub hint: String,
    pub completions: ArgCompletionSource,
}

/// Command categories for /help grouping.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CommandCategory {
    Session,
    Model,
    Tools,
    Info,
    Development,
    Utility,
}

impl CommandCategory {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Session => "Session",
            Self::Model => "Model & Provider",
            Self::Tools => "Tools",
            Self::Info => "Information",
            Self::Development => "Development",
            Self::Utility => "Utility",
        }
    }
}

/// Metadata every command must declare.
pub struct CommandMeta {
    pub name: &'static str,
    pub description: &'static str,
    pub aliases: &'static [&'static str],
    pub args: Vec<ArgDef>,
    pub category: CommandCategory,
    pub hidden: bool,
}

/// Successful command output.
pub enum CommandOutput {
    Message(String),
    Messages(Vec<String>),
    Silent,
}

/// Result of command execution.
pub type CommandResult = Result<CommandOutput, String>;

/// The trait every slash command implements.
/// execute() is synchronous — commands access engine via try_lock() like existing code.
pub trait Command: Send + Sync {
    fn meta(&self) -> &CommandMeta;
    fn execute(&self, args: &str, ctx: &mut CommandContext) -> CommandResult;
}

use registry::CommandRegistry;

/// Register all built-in commands.
pub fn register_all(registry: &mut CommandRegistry) {
    // Session
    registry.register(Box::new(session::ClearCommand::new()));
    registry.register(Box::new(session::CompactCommand::new()));
    registry.register(Box::new(session::SessionsCommand::new()));
    registry.register(Box::new(session::ExitCommand::new()));
    // Model
    registry.register(Box::new(model::ModelCommand::new()));
    registry.register(Box::new(model::ProviderCommand::new()));
    registry.register(Box::new(model::ProvidersCommand::new()));
    registry.register(Box::new(model::EffortCommand::new()));
    // Tools
    registry.register(Box::new(tools::ToolsCommand::new()));
    registry.register(Box::new(tools::PermissionsCommand::new()));
    // Info
    registry.register(Box::new(info::HelpCommand::new()));
    registry.register(Box::new(info::StatusCommand::new()));
    registry.register(Box::new(info::CostCommand::new()));
    registry.register(Box::new(info::VersionCommand::new()));
    registry.register(Box::new(info::ConfigCommand::new()));
    registry.register(Box::new(info::ContextCommand::new()));
    registry.register(Box::new(info::DoctorCommand::new()));
    // Dev
    registry.register(Box::new(dev::DiffCommand::new()));
    registry.register(Box::new(dev::BugCommand::new()));
    // Utility
    registry.register(Box::new(utility::CopyCommand::new()));
    registry.register(Box::new(utility::KeysCommand::new()));
    registry.register(Box::new(utility::HistoryCommand::new()));
    registry.register(Box::new(utility::TimeCommand::new()));
}
```

- [ ] **Step 2: Create commands/context.rs**

Create `crates/yode-tui/src/commands/context.rs`:

```rust
use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::Mutex;
use yode_core::engine::AgentEngine;
use yode_llm::registry::ProviderRegistry;
use yode_tools::registry::ToolRegistry;

use crate::app::{ChatEntry, SessionState};
use crate::terminal_caps::TerminalCaps;

/// Mutable context for command execution.
/// Engine is Arc<Mutex<>> because it's shared with the async streaming task.
pub struct CommandContext<'a> {
    pub engine: Arc<Mutex<AgentEngine>>,
    pub provider_registry: &'a Arc<ProviderRegistry>,
    pub provider_name: &'a mut String,
    pub provider_models: &'a mut Vec<String>,
    pub all_provider_models: &'a HashMap<String, Vec<String>>,
    pub chat_entries: &'a mut Vec<ChatEntry>,
    pub tools: &'a Arc<ToolRegistry>,
    pub session: &'a SessionState,
    pub terminal_caps: &'a TerminalCaps,
    pub input_history: &'a [String],
    pub should_quit: &'a mut bool,
}

/// Read-only context for completions. No &mut, safe on every keystroke.
pub struct CompletionContext<'a> {
    pub provider_models: &'a [String],
    pub all_provider_models: &'a HashMap<String, Vec<String>>,
    pub provider_name: &'a str,
    pub tools: &'a Arc<ToolRegistry>,
}
```

- [ ] **Step 3: Add `pub mod commands;` to lib.rs**

In `crates/yode-tui/src/lib.rs`, add:

```rust
pub mod commands;
```

- [ ] **Step 4: Create stub modules for all categories**

Create empty stub files so the project compiles incrementally. Each will be filled in Tasks 4-9.

```bash
mkdir -p crates/yode-tui/src/commands/{session,model,tools,info,dev,utility}
```

Create each stub `mod.rs` with just enough to compile. For example:

`crates/yode-tui/src/commands/session/mod.rs`:
```rust
// Stub — will be populated in Task 4
```

Do the same for `model/mod.rs`, `tools/mod.rs`, `info/mod.rs`, `dev/mod.rs`, `utility/mod.rs`.

Also, temporarily comment out the `register_all()` body (keep the function signature) since the command types don't exist yet.

- [ ] **Step 5: Verify it compiles**

Run: `cargo check -p yode-tui`
Expected: Clean compilation (stubs + registry + context all compile).

- [ ] **Step 6: Commit**

```bash
git add crates/yode-tui/src/commands/ crates/yode-tui/src/lib.rs
git commit -m "feat(tui): add Command trait, CommandContext, and commands module skeleton"
```

---

## Task 3: CommandRegistry

**Files:**
- Create: `crates/yode-tui/src/commands/registry.rs`

- [ ] **Step 1: Create registry.rs**

Create `crates/yode-tui/src/commands/registry.rs`:

```rust
use std::collections::{BTreeMap, HashMap};

use super::context::CompletionContext;
use super::{ArgCompletionSource, Command, CommandCategory, CommandMeta};

pub struct CommandSuggestion {
    pub name: String,
    pub description: String,
    pub is_alias: bool,
}

pub struct CommandRegistry {
    commands: Vec<Box<dyn Command>>,
    name_index: HashMap<String, usize>,
}

impl CommandRegistry {
    pub fn new() -> Self {
        Self {
            commands: Vec::new(),
            name_index: HashMap::new(),
        }
    }

    pub fn register(&mut self, cmd: Box<dyn Command>) {
        let idx = self.commands.len();
        let meta = cmd.meta();
        self.name_index.insert(meta.name.to_string(), idx);
        for alias in meta.aliases {
            self.name_index.insert(alias.to_string(), idx);
        }
        self.commands.push(cmd);
    }

    /// Register a dynamic command (e.g., from skills).
    pub fn register_dynamic(&mut self, cmd: Box<dyn Command>) {
        self.register(cmd);
    }

    pub fn find(&self, name: &str) -> Option<&dyn Command> {
        self.name_index.get(name).map(|&idx| self.commands[idx].as_ref())
    }

    pub fn visible_commands(&self) -> Vec<&dyn Command> {
        self.commands.iter()
            .map(|c| c.as_ref())
            .filter(|c| !c.meta().hidden)
            .collect()
    }

    pub fn by_category(&self) -> BTreeMap<CommandCategory, Vec<&dyn Command>> {
        let mut map = BTreeMap::new();
        for cmd in &self.commands {
            if cmd.meta().hidden {
                continue;
            }
            map.entry(cmd.meta().category)
                .or_insert_with(Vec::new)
                .push(cmd.as_ref());
        }
        map
    }

    /// Command name completion: prefix match, then substring fallback.
    pub fn complete_command(&self, prefix: &str) -> Vec<CommandSuggestion> {
        let prefix_lower = prefix.to_lowercase();
        let mut results: Vec<CommandSuggestion> = Vec::new();

        // Phase 1: prefix match
        for cmd in &self.commands {
            let meta = cmd.meta();
            if meta.hidden {
                continue;
            }
            if meta.name.starts_with(&prefix_lower) {
                results.push(CommandSuggestion {
                    name: meta.name.to_string(),
                    description: meta.description.to_string(),
                    is_alias: false,
                });
            }
            for alias in meta.aliases {
                if alias.starts_with(&prefix_lower) {
                    results.push(CommandSuggestion {
                        name: alias.to_string(),
                        description: meta.description.to_string(),
                        is_alias: true,
                    });
                }
            }
        }

        // Phase 2: if no prefix matches, try substring
        if results.is_empty() {
            for cmd in &self.commands {
                let meta = cmd.meta();
                if meta.hidden {
                    continue;
                }
                if meta.name.contains(&prefix_lower) {
                    results.push(CommandSuggestion {
                        name: meta.name.to_string(),
                        description: meta.description.to_string(),
                        is_alias: false,
                    });
                }
            }
        }

        // Sort by name length (shorter first, like current behavior)
        results.sort_by_key(|s| s.name.len());
        results
    }

    /// Argument completion: determine position from args_so_far, delegate to ArgDef.
    pub fn complete_args(
        &self,
        cmd_name: &str,
        args_so_far: &[&str],
        partial: &str,
        ctx: &CompletionContext,
    ) -> Vec<String> {
        let cmd = match self.find(cmd_name) {
            Some(c) => c,
            None => return Vec::new(),
        };

        let meta = cmd.meta();
        let arg_index = args_so_far.len();

        if arg_index >= meta.args.len() {
            return Vec::new();
        }

        let arg_def = &meta.args[arg_index];
        let all_values = match &arg_def.completions {
            ArgCompletionSource::None => return Vec::new(),
            ArgCompletionSource::Static(vals) => vals.clone(),
            ArgCompletionSource::Dynamic(f) => f(ctx),
        };

        if partial.is_empty() {
            all_values
        } else {
            let partial_lower = partial.to_lowercase();
            all_values
                .into_iter()
                .filter(|v| v.to_lowercase().starts_with(&partial_lower))
                .collect()
        }
    }

    /// Get args hint string for a command (for display when user types "/cmd ").
    pub fn args_hint(&self, cmd_name: &str) -> Option<String> {
        let cmd = self.find(cmd_name)?;
        let meta = cmd.meta();
        if meta.args.is_empty() {
            return None;
        }
        let hints: Vec<&str> = meta.args.iter().map(|a| a.hint.as_str()).collect();
        Some(hints.join(" "))
    }

    /// Edit-distance suggestion for typos.
    pub fn suggest_similar(&self, typo: &str) -> Option<String> {
        let mut best: Option<(usize, String)> = None;
        for cmd in &self.commands {
            let name = cmd.meta().name;
            let dist = levenshtein(typo, name);
            let threshold = name.len() / 2 + 1;
            if dist <= threshold {
                if best.is_none() || dist < best.as_ref().unwrap().0 {
                    best = Some((dist, name.to_string()));
                }
            }
        }
        best.map(|(_, name)| name)
    }
}

fn levenshtein(a: &str, b: &str) -> usize {
    let a_bytes = a.as_bytes();
    let b_bytes = b.as_bytes();
    let mut dp = vec![vec![0usize; b_bytes.len() + 1]; a_bytes.len() + 1];
    for i in 0..=a_bytes.len() {
        dp[i][0] = i;
    }
    for j in 0..=b_bytes.len() {
        dp[0][j] = j;
    }
    for i in 1..=a_bytes.len() {
        for j in 1..=b_bytes.len() {
            let cost = if a_bytes[i - 1] == b_bytes[j - 1] { 0 } else { 1 };
            dp[i][j] = (dp[i - 1][j] + 1)
                .min(dp[i][j - 1] + 1)
                .min(dp[i - 1][j - 1] + cost);
        }
    }
    dp[a_bytes.len()][b_bytes.len()]
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::*;
    use context::CommandContext;
    

    struct DummyCommand {
        meta: CommandMeta,
    }

    impl DummyCommand {
        fn new(name: &'static str, desc: &'static str, aliases: &'static [&'static str], category: CommandCategory) -> Self {
            Self {
                meta: CommandMeta {
                    name,
                    description: desc,
                    aliases,
                    args: vec![],
                    category,
                    hidden: false,
                },
            }
        }
    }

#[async_trait]
    impl Command for DummyCommand {
        fn meta(&self) -> &CommandMeta { &self.meta }
        fn execute(&self, _args: &str, _ctx: &mut CommandContext) -> CommandResult {
            Ok(CommandOutput::Silent)
        }
    }

    #[test]
    fn test_register_and_find() {
        let mut reg = CommandRegistry::new();
        reg.register(Box::new(DummyCommand::new("model", "Switch model", &["m"], CommandCategory::Model)));
        assert!(reg.find("model").is_some());
        assert!(reg.find("m").is_some());
        assert!(reg.find("xyz").is_none());
    }

    #[test]
    fn test_complete_command_prefix() {
        let mut reg = CommandRegistry::new();
        reg.register(Box::new(DummyCommand::new("model", "Switch model", &[], CommandCategory::Model)));
        reg.register(Box::new(DummyCommand::new("memory", "Memory", &[], CommandCategory::Utility)));
        let results = reg.complete_command("mo");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "model");
    }

    #[test]
    fn test_complete_command_substring_fallback() {
        let mut reg = CommandRegistry::new();
        reg.register(Box::new(DummyCommand::new("compact", "Compress", &[], CommandCategory::Session)));
        let results = reg.complete_command("pac");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "compact");
    }

    #[test]
    fn test_suggest_similar_typo() {
        let mut reg = CommandRegistry::new();
        reg.register(Box::new(DummyCommand::new("model", "Switch model", &[], CommandCategory::Model)));
        assert_eq!(reg.suggest_similar("modle"), Some("model".to_string()));
        assert_eq!(reg.suggest_similar("zzzzz"), None);
    }

    #[test]
    fn test_by_category() {
        let mut reg = CommandRegistry::new();
        reg.register(Box::new(DummyCommand::new("model", "M", &[], CommandCategory::Model)));
        reg.register(Box::new(DummyCommand::new("clear", "C", &[], CommandCategory::Session)));
        let cats = reg.by_category();
        assert!(cats.contains_key(&CommandCategory::Model));
        assert!(cats.contains_key(&CommandCategory::Session));
    }

    #[test]
    fn test_levenshtein() {
        assert_eq!(levenshtein("kitten", "sitting"), 3);
        assert_eq!(levenshtein("", "abc"), 3);
        assert_eq!(levenshtein("abc", "abc"), 0);
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p yode-tui -- commands::registry`
Expected: All 5 registry tests pass.

- [ ] **Step 3: Commit**

```bash
git add crates/yode-tui/src/commands/registry.rs
git commit -m "feat(tui): add CommandRegistry with completion, typo suggestion, and tests"
```

---

## Task 4: Migrate Session Commands (clear, compact, sessions, exit)

**Files:**
- Create: `crates/yode-tui/src/commands/session/mod.rs`
- Create: `crates/yode-tui/src/commands/session/clear.rs`
- Create: `crates/yode-tui/src/commands/session/compact.rs`
- Create: `crates/yode-tui/src/commands/session/sessions.rs`
- Create: `crates/yode-tui/src/commands/session/exit.rs`
- Reference: `crates/yode-tui/src/app/commands.rs:112-118` (clear), `212-218` (compact), `282-324` (sessions), `116-118` (exit)

- [ ] **Step 1: Create session/mod.rs**

```rust
mod clear;
mod compact;
mod sessions;
mod exit;

pub use clear::ClearCommand;
pub use compact::CompactCommand;
pub use sessions::SessionsCommand;
pub use exit::ExitCommand;
```

- [ ] **Step 2: Create session/clear.rs**

Port from `commands.rs:112-115`. The command clears chat_entries.

```rust
use crate::commands::{Command, CommandMeta, CommandCategory, CommandOutput, CommandResult, ArgDef, ArgCompletionSource};
use crate::commands::context::CommandContext;

pub struct ClearCommand {
    meta: CommandMeta,
}

impl ClearCommand {
    pub fn new() -> Self {
        Self {
            meta: CommandMeta {
                name: "clear",
                description: "Clear chat history",
                aliases: &[],
                args: vec![ArgDef {
                    name: "scope".into(),
                    required: false,
                    hint: "[context]".into(),
                    completions: ArgCompletionSource::Static(vec!["context".into()]),
                }],
                category: CommandCategory::Session,
                hidden: false,
            },
        }
    }
}
impl Command for ClearCommand {
    fn meta(&self) -> &CommandMeta { &self.meta }

    fn execute(&self, args: &str, ctx: &mut CommandContext<'_>) -> CommandResult {
        let args = args.trim();
        if args == "context" {
            // Clear context only (keep system messages)
            ctx.chat_entries.retain(|e| matches!(e.role, crate::app::ChatRole::System));
            Ok(CommandOutput::Message("Context cleared.".into()))
        } else {
            ctx.chat_entries.clear();
            Ok(CommandOutput::Message("Chat history cleared.".into()))
        }
    }
}
```

- [ ] **Step 3: Create session/compact.rs**

Port from `commands.rs:212-218`.

```rust

use crate::commands::{Command, CommandMeta, CommandCategory, CommandOutput, CommandResult, ArgDef, ArgCompletionSource};
use crate::commands::context::CommandContext;

pub struct CompactCommand {
    meta: CommandMeta,
}

impl CompactCommand {
    pub fn new() -> Self {
        Self {
            meta: CommandMeta {
                name: "compact",
                description: "Compress conversation history",
                aliases: &[],
                args: vec![ArgDef {
                    name: "keep_last".into(),
                    required: false,
                    hint: "[keep_last=20]".into(),
                    completions: ArgCompletionSource::None,
                }],
                category: CommandCategory::Session,
                hidden: false,
            },
        }
    }
}
impl Command for CompactCommand {
    fn meta(&self) -> &CommandMeta { &self.meta }

    fn execute(&self, args: &str, ctx: &mut CommandContext<'_>) -> CommandResult {
        let keep = args.trim().parse::<usize>().unwrap_or(20);
        let total = ctx.chat_entries.len();
        if total <= keep {
            return Ok(CommandOutput::Message(format!("History has only {total} entries, nothing to compact.")));
        }
        let removed = total - keep;
        ctx.chat_entries.drain(..removed);
        Ok(CommandOutput::Message(format!("Compacted: removed {removed} entries, kept last {keep}.")))
    }
}
```

- [ ] **Step 4: Create session/sessions.rs**

Port from `commands.rs:282-324`. This opens the DB and lists recent sessions.

```rust

use crate::commands::{Command, CommandMeta, CommandCategory, CommandOutput, CommandResult, ArgDef};
use crate::commands::context::CommandContext;

pub struct SessionsCommand {
    meta: CommandMeta,
}

impl SessionsCommand {
    pub fn new() -> Self {
        Self {
            meta: CommandMeta {
                name: "sessions",
                description: "List recent sessions",
                aliases: &[],
                args: vec![],
                category: CommandCategory::Session,
                hidden: false,
            },
        }
    }
}
impl Command for SessionsCommand {
    fn meta(&self) -> &CommandMeta { &self.meta }

    fn execute(&self, _args: &str, _ctx: &mut CommandContext<'_>) -> CommandResult {
        // Port the exact logic from commands.rs:282-324
        // Uses dirs::home_dir() to find ~/.yode/sessions.db
        // Opens Database, calls list_sessions(10)
        // Displays session id (first 8 chars), model, age, message count
        let db_path = dirs::home_dir()
            .map(|h| h.join(".yode").join("sessions.db"))
            .unwrap_or_default();

        if !db_path.exists() {
            return Ok(CommandOutput::Message("No session database found.".into()));
        }

        match yode_core::db::Database::open(&db_path) {
            Ok(db) => {
                match db.list_sessions(10) {
                    Ok(sessions) => {
                        if sessions.is_empty() {
                            return Ok(CommandOutput::Message("No sessions found.".into()));
                        }
                        let mut lines = vec!["Recent sessions:".to_string()];
                        for s in sessions {
                            lines.push(format!(
                                "  {} | {} | {} msgs",
                                &s.id[..8.min(s.id.len())],
                                s.model,
                                s.message_count
                            ));
                        }
                        Ok(CommandOutput::Messages(lines))
                    }
                    Err(e) => Err(format!("Failed to load sessions: {e}")),
                }
            }
            Err(e) => Err(format!("Failed to open database: {e}")),
        }
    }
}
```

Note: Read the actual `commands.rs:282-324` during implementation and port the exact field names, formatting, and API calls. The above is a guide — the real Database API may differ.

- [ ] **Step 5: Create session/exit.rs**

Port from `commands.rs:116-118`.

```rust

use crate::commands::{Command, CommandMeta, CommandCategory, CommandOutput, CommandResult};
use crate::commands::context::CommandContext;

pub struct ExitCommand {
    meta: CommandMeta,
}

impl ExitCommand {
    pub fn new() -> Self {
        Self {
            meta: CommandMeta {
                name: "exit",
                description: "Quit application",
                aliases: &["quit", "q"],
                args: vec![],
                category: CommandCategory::Session,
                hidden: false,
            },
        }
    }
}
impl Command for ExitCommand {
    fn meta(&self) -> &CommandMeta { &self.meta }

    fn execute(&self, _args: &str, ctx: &mut CommandContext<'_>) -> CommandResult {
        *ctx.should_quit = true;
        Ok(CommandOutput::Silent)
    }
}
```

- [ ] **Step 6: Verify compilation**

Run: `cargo check -p yode-tui 2>&1 | head -30`
Expected: May have errors for other missing modules (model, tools, etc.) but session module should compile.

- [ ] **Step 7: Commit**

```bash
git add crates/yode-tui/src/commands/session/
git commit -m "feat(tui): migrate session commands (clear, compact, sessions, exit)"
```

---

## Task 5: Migrate Model Commands (model, provider, providers) + New /effort

**Files:**
- Create: `crates/yode-tui/src/commands/model/mod.rs`
- Create: `crates/yode-tui/src/commands/model/model.rs`
- Create: `crates/yode-tui/src/commands/model/provider.rs`
- Create: `crates/yode-tui/src/commands/model/providers.rs`
- Create: `crates/yode-tui/src/commands/model/effort.rs`
- Reference: `crates/yode-tui/src/app/commands.rs:119-199`

- [ ] **Step 1: Create model/mod.rs**

```rust
mod model;
mod provider;
mod providers;
mod effort;

pub use model::ModelCommand;
pub use provider::ProviderCommand;
pub use providers::ProvidersCommand;
pub use effort::EffortCommand;
```

- [ ] **Step 2: Create model/model.rs**

Port from `commands.rs:119-157`. Key: dynamic arg completion from provider_models.

```rust

use crate::commands::{Command, CommandMeta, CommandCategory, CommandOutput, CommandResult, ArgDef, ArgCompletionSource};
use crate::commands::context::CommandContext;

pub struct ModelCommand {
    meta: CommandMeta,
}

impl ModelCommand {
    pub fn new() -> Self {
        Self {
            meta: CommandMeta {
                name: "model",
                description: "Show or switch the current model",
                aliases: &["m"],
                args: vec![ArgDef {
                    name: "name".into(),
                    required: false,
                    hint: "<model-name>".into(),
                    completions: ArgCompletionSource::Dynamic(|ctx| {
                        ctx.provider_models.to_vec()
                    }),
                }],
                category: CommandCategory::Model,
                hidden: false,
            },
        }
    }
}
impl Command for ModelCommand {
    fn meta(&self) -> &CommandMeta { &self.meta }

    fn execute(&self, args: &str, ctx: &mut CommandContext<'_>) -> CommandResult {
        let args = args.trim();

        if args.is_empty() {
            // Show current model + available models (read from app state, no lock needed)
            let models_list = if ctx.provider_models.is_empty() {
                "  (unrestricted)".to_string()
            } else {
                ctx.provider_models.iter()
                    .map(|m| format!("    {m}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            };
            return Ok(CommandOutput::Message(format!(
                "Current model: {}\nProvider: {}\nAvailable models:\n{}",
                ctx.session.model, ctx.provider_name, models_list
            )));
        }

        // Validate model exists in current provider's model list
        if !ctx.provider_models.is_empty() && !ctx.provider_models.iter().any(|m| m == args) {
            let available = ctx.provider_models.join(", ");
            return Err(format!("Model '{args}' not available. Available: {available}"));
        }

        if let Ok(mut eng) = ctx.engine.try_lock() {
            eng.set_model(args.to_string());
        }

        Ok(CommandOutput::Message(format!("Switched to model: {args}")))
    }
}
```

- [ ] **Step 3: Create model/provider.rs**

Port from `commands.rs:158-186`. Key: dynamic arg completion from all_provider_models keys.

```rust

use crate::commands::{Command, CommandMeta, CommandCategory, CommandOutput, CommandResult, ArgDef, ArgCompletionSource};
use crate::commands::context::CommandContext;

pub struct ProviderCommand {
    meta: CommandMeta,
}

impl ProviderCommand {
    pub fn new() -> Self {
        Self {
            meta: CommandMeta {
                name: "provider",
                description: "Switch LLM provider",
                aliases: &[],
                args: vec![ArgDef {
                    name: "name".into(),
                    required: true,
                    hint: "<provider-name>".into(),
                    completions: ArgCompletionSource::Dynamic(|ctx| {
                        let mut keys: Vec<String> = ctx.all_provider_models.keys().cloned().collect();
                        keys.sort();
                        keys
                    }),
                }],
                category: CommandCategory::Model,
                hidden: false,
            },
        }
    }
}
impl Command for ProviderCommand {
    fn meta(&self) -> &CommandMeta { &self.meta }

    fn execute(&self, args: &str, ctx: &mut CommandContext<'_>) -> CommandResult {
        let name = args.trim();
        if name.is_empty() {
            return Ok(CommandOutput::Message(format!(
                "Current provider: {}\nUse /provider <name> to switch, /providers to list all.",
                ctx.provider_name
            )));
        }

        let provider = ctx.provider_registry
            .get(name)
            .ok_or_else(|| format!("Unknown provider: {name}. Use /providers to list."))?;

        let models = ctx.all_provider_models
            .get(name)
            .cloned()
            .unwrap_or_default();

        let first_model = models.first().cloned().unwrap_or_default();

        if let Ok(mut eng) = ctx.engine.try_lock() {
            eng.set_provider(provider, name.to_string());
            if !first_model.is_empty() {
                eng.set_model(first_model.clone());
            }
        }

        *ctx.provider_name = name.to_string();
        *ctx.provider_models = models;

        Ok(CommandOutput::Message(format!("Switched to provider: {name} (model: {first_model})")))
    }
}
```

- [ ] **Step 4: Create model/providers.rs**

Port from `commands.rs:187-199`.

```rust

use crate::commands::{Command, CommandMeta, CommandCategory, CommandOutput, CommandResult};
use crate::commands::context::CommandContext;

pub struct ProvidersCommand {
    meta: CommandMeta,
}

impl ProvidersCommand {
    pub fn new() -> Self {
        Self {
            meta: CommandMeta {
                name: "providers",
                description: "List available providers and their models",
                aliases: &[],
                args: vec![],
                category: CommandCategory::Model,
                hidden: false,
            },
        }
    }
}
impl Command for ProvidersCommand {
    fn meta(&self) -> &CommandMeta { &self.meta }

    fn execute(&self, _args: &str, ctx: &mut CommandContext<'_>) -> CommandResult {
        let mut lines = vec!["Available providers:".to_string()];
        for (name, models) in ctx.all_provider_models.iter() {
            let marker = if name == ctx.provider_name.as_str() { " (active)" } else { "" };
            lines.push(format!("  {name}{marker}"));
            for model in models {
                lines.push(format!("    - {model}"));
            }
        }
        Ok(CommandOutput::Messages(lines))
    }
}
```

- [ ] **Step 5: Create model/effort.rs (NEW command)**

```rust

use yode_core::context::EffortLevel;
use crate::commands::{Command, CommandMeta, CommandCategory, CommandOutput, CommandResult, ArgDef, ArgCompletionSource};
use crate::commands::context::CommandContext;

pub struct EffortCommand {
    meta: CommandMeta,
}

impl EffortCommand {
    pub fn new() -> Self {
        Self {
            meta: CommandMeta {
                name: "effort",
                description: "Show or set AI thinking effort level",
                aliases: &[],
                args: vec![ArgDef {
                    name: "level".into(),
                    required: false,
                    hint: "<min|low|medium|high|max>".into(),
                    completions: ArgCompletionSource::Static(
                        vec!["min".into(), "low".into(), "medium".into(), "high".into(), "max".into()]
                    ),
                }],
                category: CommandCategory::Model,
                hidden: false,
            },
        }
    }
}
impl Command for EffortCommand {
    fn meta(&self) -> &CommandMeta { &self.meta }

    fn execute(&self, args: &str, ctx: &mut CommandContext<'_>) -> CommandResult {
        let args = args.trim();

        if args.is_empty() {
            if let Ok(eng) = ctx.engine.try_lock() {
                let level = eng.effort();
                return Ok(CommandOutput::Message(format!("Current effort level: {level}")));
            }
            return Ok(CommandOutput::Message("Current effort level: unknown (engine busy)".into()));
        }

        let level: EffortLevel = args.parse().map_err(|e: String| e)?;
        if let Ok(mut eng) = ctx.engine.try_lock() {
            eng.set_effort(level);
        }
        Ok(CommandOutput::Message(format!("Effort level set to: {level}")))
    }
}
```

- [ ] **Step 6: Verify compilation**

Run: `cargo check -p yode-tui 2>&1 | head -30`

- [ ] **Step 7: Commit**

```bash
git add crates/yode-tui/src/commands/model/
git commit -m "feat(tui): migrate model/provider commands + add /effort command"
```

---

## Task 6: Migrate Tools Commands + New /permissions

**Files:**
- Create: `crates/yode-tui/src/commands/tools/mod.rs`
- Create: `crates/yode-tui/src/commands/tools/tools.rs`
- Create: `crates/yode-tui/src/commands/tools/permissions.rs`
- Reference: `crates/yode-tui/src/app/commands.rs:200-211`

- [ ] **Step 1: Create tools/mod.rs**

```rust
mod tools;
mod permissions;

pub use tools::ToolsCommand;
pub use permissions::PermissionsCommand;
```

- [ ] **Step 2: Create tools/tools.rs**

Port from `commands.rs:200-211`.

```rust

use crate::commands::{Command, CommandMeta, CommandCategory, CommandOutput, CommandResult};
use crate::commands::context::CommandContext;

pub struct ToolsCommand {
    meta: CommandMeta,
}

impl ToolsCommand {
    pub fn new() -> Self {
        Self {
            meta: CommandMeta {
                name: "tools",
                description: "List registered tools",
                aliases: &[],
                args: vec![],
                category: CommandCategory::Tools,
                hidden: false,
            },
        }
    }
}
impl Command for ToolsCommand {
    fn meta(&self) -> &CommandMeta { &self.meta }

    fn execute(&self, _args: &str, ctx: &mut CommandContext<'_>) -> CommandResult {
        let defs = ctx.tools.definitions();
        let mut lines = vec![format!("Registered tools ({}):", defs.len())];
        for d in &defs {
            lines.push(format!("  {} — {}", d.name, d.description));
        }
        Ok(CommandOutput::Messages(lines))
    }
}
```

- [ ] **Step 3: Create tools/permissions.rs (NEW command)**

```rust

use crate::commands::{Command, CommandMeta, CommandCategory, CommandOutput, CommandResult, ArgDef, ArgCompletionSource};
use crate::commands::context::CommandContext;

pub struct PermissionsCommand {
    meta: CommandMeta,
}

impl PermissionsCommand {
    pub fn new() -> Self {
        Self {
            meta: CommandMeta {
                name: "permissions",
                description: "View or modify tool execution permissions",
                aliases: &["perms"],
                args: vec![
                    ArgDef {
                        name: "tool".into(),
                        required: false,
                        hint: "<tool-name|reset>".into(),
                        completions: ArgCompletionSource::Dynamic(|ctx| {
                            let mut names: Vec<String> = ctx.tools.definitions()
                                .iter()
                                .map(|d| d.name.clone())
                                .collect();
                            names.push("reset".into());
                            names.sort();
                            names
                        }),
                    },
                    ArgDef {
                        name: "action".into(),
                        required: false,
                        hint: "<allow|deny>".into(),
                        completions: ArgCompletionSource::Static(vec!["allow".into(), "deny".into()]),
                    },
                ],
                category: CommandCategory::Tools,
                hidden: false,
            },
        }
    }
}
impl Command for PermissionsCommand {
    fn meta(&self) -> &CommandMeta { &self.meta }

    fn execute(&self, args: &str, ctx: &mut CommandContext<'_>) -> CommandResult {
        let parts: Vec<&str> = args.trim().split_whitespace().collect();

        let Ok(mut engine) = ctx.engine.try_lock() else {
            return Err("Engine is busy, try again.".into());
        };

        match parts.as_slice() {
            // No args: show current permissions
            [] => {
                let tools = engine.permissions().confirmable_tools();
                if tools.is_empty() {
                    Ok(CommandOutput::Message("All tools are auto-allowed (no confirmations required).".into()))
                } else {
                    let mut lines = vec!["Tools requiring confirmation:".to_string()];
                    for t in tools {
                        lines.push(format!("  {t}"));
                    }
                    Ok(CommandOutput::Messages(lines))
                }
            }
            // Reset
            ["reset"] => {
                engine.permissions_mut().reset(vec![
                    "bash".into(),
                    "write_file".into(),
                    "edit_file".into(),
                ]);
                Ok(CommandOutput::Message("Permissions reset to defaults.".into()))
            }
            // /permissions <tool> allow
            [tool, "allow"] => {
                engine.permissions_mut().allow(tool);
                Ok(CommandOutput::Message(format!("Tool '{tool}' set to auto-allow.")))
            }
            // /permissions <tool> deny
            [tool, "deny"] => {
                engine.permissions_mut().deny(tool);
                Ok(CommandOutput::Message(format!("Tool '{tool}' now requires confirmation.")))
            }
            _ => Err("Usage: /permissions [tool] [allow|deny] or /permissions reset".into()),
        }
    }
}
```

Note: This requires `permissions()` and `permissions_mut()` methods on `AgentEngine`. Add to engine.rs:

```rust
pub fn permissions(&self) -> &PermissionManager { &self.permissions }
pub fn permissions_mut(&mut self) -> &mut PermissionManager { &mut self.permissions }
```

- [ ] **Step 4: Verify compilation**

Run: `cargo check -p yode-tui 2>&1 | head -30`

- [ ] **Step 5: Commit**

```bash
git add crates/yode-tui/src/commands/tools/
git commit -m "feat(tui): migrate tools command + add /permissions command"
```

---

## Task 7: Migrate Info Commands (help, status, cost, version, config, context, doctor)

**Files:**
- Create: `crates/yode-tui/src/commands/info/mod.rs`
- Create: `crates/yode-tui/src/commands/info/help.rs`
- Create: `crates/yode-tui/src/commands/info/status.rs`
- Create: `crates/yode-tui/src/commands/info/cost.rs`
- Create: `crates/yode-tui/src/commands/info/version.rs`
- Create: `crates/yode-tui/src/commands/info/config.rs`
- Create: `crates/yode-tui/src/commands/info/context_cmd.rs`
- Create: `crates/yode-tui/src/commands/info/doctor.rs`
- Reference: `crates/yode-tui/src/app/commands.rs:71-78` (help), `261-281` (status), `219-226` (cost), `462-471` (version), `443-461` (config), `248-260` (context), `391-442` (doctor)

- [ ] **Step 1: Create info/mod.rs**

```rust
mod help;
mod status;
mod cost;
mod version;
mod config;
mod context_cmd;
mod doctor;

pub use help::HelpCommand;
pub use status::StatusCommand;
pub use cost::CostCommand;
pub use version::VersionCommand;
pub use config::ConfigCommand;
pub use context_cmd::ContextCommand;
pub use doctor::DoctorCommand;
```

- [ ] **Step 2: Create each info command file**

Port each command from the corresponding line ranges in `commands.rs`. Each follows the same pattern:
- Struct with `meta: CommandMeta`
- `new()` constructor
- `Command` trait impl

**IMPORTANT:** For `/help`, the new implementation should use `CommandRegistry::by_category()` to display grouped help. This means `/help` needs access to the registry. Add `cmd_registry: &'a CommandRegistry` to `CommandContext`.

For each file, read the corresponding lines from `commands.rs` and port the logic precisely. Key specifics:

- **help.rs**: Iterate `ctx.cmd_registry.by_category()` and format with category headers
- **status.rs**: Port `commands.rs:261-281` — session info, duration, tokens, tool calls
- **cost.rs**: Port `commands.rs:219-226` + `estimate_cost()` helper (lines 633-653)
- **version.rs**: Port `commands.rs:462-471` — display version string
- **config.rs**: Port `commands.rs:443-461` — display config summary
- **context_cmd.rs**: Port `commands.rs:248-260` — context window usage
- **doctor.rs**: Port `commands.rs:391-442` — environment health check

- [ ] **Step 3: Update CommandContext to include cmd_registry**

In `crates/yode-tui/src/commands/context.rs`, add to `CommandContext`:

```rust
pub cmd_registry: &'a super::registry::CommandRegistry,
```

- [ ] **Step 4: Verify compilation**

Run: `cargo check -p yode-tui 2>&1 | head -30`

- [ ] **Step 5: Commit**

```bash
git add crates/yode-tui/src/commands/info/ crates/yode-tui/src/commands/context.rs
git commit -m "feat(tui): migrate info commands (help, status, cost, version, config, context, doctor)"
```

---

## Task 8: Migrate Dev Commands (diff, bug)

**Files:**
- Create: `crates/yode-tui/src/commands/dev/mod.rs`
- Create: `crates/yode-tui/src/commands/dev/diff.rs`
- Create: `crates/yode-tui/src/commands/dev/bug.rs`
- Reference: `crates/yode-tui/src/app/commands.rs:227-247` (diff), `365-390` (bug)

- [ ] **Step 1: Create dev/mod.rs**

```rust
mod diff;
mod bug;

pub use diff::DiffCommand;
pub use bug::BugCommand;
```

- [ ] **Step 2: Create dev/diff.rs**

Port from `commands.rs:227-247`. Runs `git diff --stat`.

- [ ] **Step 3: Create dev/bug.rs**

Port from `commands.rs:365-390`. Generates bug report.

- [ ] **Step 4: Verify compilation**

Run: `cargo check -p yode-tui 2>&1 | head -30`

- [ ] **Step 5: Commit**

```bash
git add crates/yode-tui/src/commands/dev/
git commit -m "feat(tui): migrate dev commands (diff, bug)"
```

---

## Task 9: Migrate Utility Commands (copy, keys, history, time)

**Files:**
- Create: `crates/yode-tui/src/commands/utility/mod.rs`
- Create: `crates/yode-tui/src/commands/utility/copy.rs`
- Create: `crates/yode-tui/src/commands/utility/keys.rs`
- Create: `crates/yode-tui/src/commands/utility/history.rs`
- Create: `crates/yode-tui/src/commands/utility/time.rs`
- Reference: `crates/yode-tui/src/app/commands.rs:325-364` (copy), `79-111` (keys), `472-487` (history), `488-506` (time)

- [ ] **Step 1: Create utility/mod.rs**

```rust
mod copy;
mod keys;
mod history;
mod time;

pub use copy::CopyCommand;
pub use keys::KeysCommand;
pub use history::HistoryCommand;
pub use time::TimeCommand;
```

- [ ] **Step 2: Create each utility command file**

Port from the corresponding line ranges. Key notes:
- **copy.rs**: Uses `arboard::Clipboard`, port from lines 325-364
- **keys.rs**: Static text display, port from lines 79-111
- **history.rs**: Has `[count]` arg with no completion, port from lines 472-487
- **time.rs**: Session timing, port from lines 488-506

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p yode-tui 2>&1 | head -30`

- [ ] **Step 4: Commit**

```bash
git add crates/yode-tui/src/commands/utility/
git commit -m "feat(tui): migrate utility commands (copy, keys, history, time)"
```

---

## Task 10: Wire Up CommandRegistry in App + Update Completion

**Files:**
- Modify: `crates/yode-tui/src/app/mod.rs` (App struct, run_app, handle_enter, completion updates)
- Modify: `crates/yode-tui/src/app/completion.rs` (remove SLASH_COMMANDS, update CommandCompletion)

This is the integration task that connects everything.

- [ ] **Step 1: Add cmd_registry field to App struct**

In `crates/yode-tui/src/app/mod.rs`, add to App struct (around line 199):

```rust
pub cmd_registry: crate::commands::registry::CommandRegistry,
```

- [ ] **Step 2: Initialize registry in App::new() or run()**

In the `run()` function (around line 370-400), after app creation, add:

```rust
let mut cmd_registry = crate::commands::registry::CommandRegistry::new();
crate::commands::register_all(&mut cmd_registry);
// Register skill commands as dynamic commands
for (name, desc) in &skill_commands {
    // Create a simple dynamic SkillCommand for each
    // (or register them differently — see step 3)
}
app.cmd_registry = cmd_registry;
```

- [ ] **Step 3: Create a DynamicSkillCommand helper**

In `crates/yode-tui/src/commands/mod.rs`, add a simple wrapper for skill commands:

```rust
/// A dynamically-registered skill command.
pub struct SkillCommand {
    meta: CommandMeta,
    skill_name: String,
}

impl SkillCommand {
    pub fn new(name: String, description: String) -> Self {
        let meta = CommandMeta {
            name: Box::leak(name.clone().into_boxed_str()),
            description: Box::leak(description.into_boxed_str()),
            aliases: &[],
            args: vec![],
            category: CommandCategory::Utility,
            hidden: false,
        };
        Self { meta, skill_name: name }
    }
}
impl Command for SkillCommand {
    fn meta(&self) -> &CommandMeta { &self.meta }
    fn execute(&self, args: &str, ctx: &mut CommandContext) -> CommandResult {
        // Skill execution is handled by the existing skill system
        // This just provides metadata for completion
        Ok(CommandOutput::Message(format!("Skill '{}' invoked.", self.skill_name)))
    }
}
```

Note: `Box::leak` is used for `&'static str` from dynamic strings. This is acceptable since skill commands live for the entire app lifetime.

- [ ] **Step 4: Replace handle_slash_command call in handle_enter**

In `crates/yode-tui/src/app/mod.rs`, around line 903 where `handle_slash_command` is called, replace with:

```rust
if raw_typed.starts_with('/') {
    let trimmed = raw_typed.trim();
    let (cmd_name, cmd_args) = match trimmed.find(' ') {
        Some(pos) => (&trimmed[1..pos], trimmed[pos + 1..].trim()),
        None => (&trimmed[1..], ""),
    };

    // IMPORTANT: Use execute_command helper method on App to avoid borrow checker
    // conflict between app.cmd_registry (immutable) and app.chat_entries (mutable).
    // The helper takes &mut self and uses an index-based approach.
    if app.cmd_registry.find(cmd_name).is_some() {
        // Build CommandContext by destructuring app fields.
        // This works because Rust allows borrowing disjoint struct fields.
        let result = {
            let cmd = app.cmd_registry.find(cmd_name).unwrap();
            let mut ctx = crate::commands::context::CommandContext {
                engine: engine.clone(),
                provider_registry: &app.provider_registry,
                provider_name: &mut app.provider_name,
                provider_models: &mut app.provider_models,
                all_provider_models: &app.all_provider_models,
                chat_entries: &mut app.chat_entries,
                tools,
                session: &app.session,
                terminal_caps: &app.terminal_caps,
                input_history: &app.history.entries(),
                should_quit: &mut app.should_quit,
                cmd_registry: &app.cmd_registry,
            };
            cmd.execute(cmd_args, &mut ctx)
        };

        match result {
            Ok(crate::commands::CommandOutput::Message(msg)) => {
                app.chat_entries.push(ChatEntry::system(msg));
            }
            Ok(crate::commands::CommandOutput::Messages(msgs)) => {
                for msg in msgs {
                    app.chat_entries.push(ChatEntry::system(msg));
                }
            }
            Ok(crate::commands::CommandOutput::Silent) => {}
            Err(e) => {
                app.chat_entries.push(ChatEntry::error(e));
            }
        }
        return;
    } else {
        // Typo suggestion
        if let Some(suggestion) = app.cmd_registry.suggest_similar(cmd_name) {
            app.chat_entries.push(ChatEntry::system(
                format!("Unknown command: /{cmd_name}. Did you mean /{suggestion}?")
            ));
        } else {
            app.chat_entries.push(ChatEntry::system(
                format!("Unknown command: /{cmd_name}. Type /help for available commands.")
            ));
        }
        return;
    }
}
```

- [ ] **Step 5: Update CommandCompletion to use CommandRegistry**

In `crates/yode-tui/src/app/completion.rs`:

1. Remove the `SLASH_COMMANDS` const array and `SlashCommand` struct.
2. Update `CommandCompletion::update()` to accept a reference to `CommandRegistry`:

```rust
pub fn update(&mut self, input: &str, single_line: bool, registry: &CommandRegistry, completion_ctx: &CompletionContext) {
    // ... same logic but source from registry instead of SLASH_COMMANDS
}
```

3. Update all call sites of `cmd_completion.update()` in `app/mod.rs` to pass `&app.cmd_registry` and a `CompletionContext`.

- [ ] **Step 6: Build CompletionContext at update sites**

At each place in `app/mod.rs` where `cmd_completion.update()` is called (lines 826, 945, 1022), construct:

```rust
let completion_ctx = crate::commands::context::CompletionContext {
    provider_models: &app.provider_models,
    all_provider_models: &app.all_provider_models,
    provider_name: &app.provider_name,
    tools: tools,
};
app.cmd_completion.update(&app.input.lines[0], !app.input.is_multiline(), &app.cmd_registry, &completion_ctx);
```

- [ ] **Step 7: Verify it compiles**

Run: `cargo check -p yode-tui`
Expected: Clean compilation.

- [ ] **Step 8: Commit**

```bash
git add crates/yode-tui/src/app/mod.rs crates/yode-tui/src/app/completion.rs crates/yode-tui/src/commands/
git commit -m "feat(tui): wire CommandRegistry into App, replace monolithic dispatch"
```

---

## Task 11: Delete Old commands.rs + Final Cleanup

**Files:**
- Delete: `crates/yode-tui/src/app/commands.rs`
- Modify: `crates/yode-tui/src/app/mod.rs` (remove `mod commands;` from app module if present)

- [ ] **Step 1: Move shell command and file reference handling**

Move `handle_shell_command()` (lines 527-589), `process_file_references()` (lines 592-624), `DANGEROUS_PATTERNS` (lines 15-40), and `is_dangerous_command()` (lines 43-51) from the old `commands.rs` into `app/mod.rs` as private functions, or into a new small `app/shell.rs` file.

- [ ] **Step 2: Delete old commands.rs**

```bash
git rm crates/yode-tui/src/app/commands.rs
```

- [ ] **Step 3: Remove old module reference**

If `app/mod.rs` has `mod commands;` referring to the old file, remove it. The new commands module is at `crates/yode-tui/src/commands/` (sibling to `app/`), referenced via `lib.rs`.

- [ ] **Step 4: Full build + test**

Run: `cargo build -p yode-tui && cargo test -p yode-core`
Expected: Clean build, all tests pass.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "refactor(tui): remove old monolithic commands.rs, migration complete"
```

---

## Task 12: End-to-End Verification

- [ ] **Step 1: Run the app**

```bash
cargo run
```

- [ ] **Step 2: Test all existing commands**

Manually verify each command still works:
- `/help` — should show grouped output by category
- `/model` — should show current model
- `/model <tab>` — should show model name completions
- `/provider <tab>` — should show provider name completions
- `/providers` — should list all providers
- `/effort` — should show current effort level
- `/effort high` — should set effort
- `/effort <tab>` — should show min/low/medium/high/max
- `/permissions` — should show current permission list
- `/permissions bash allow` — should modify permissions
- `/permissions <tab>` — should show tool names
- `/permissions bash <tab>` — should show allow/deny
- `/clear`, `/exit`, `/tools`, `/compact`, `/cost`, `/diff`, `/context`, `/status`, `/sessions`, `/copy`, `/bug`, `/doctor`, `/config`, `/version`, `/history`, `/time`, `/keys`
- Aliases: `/m` should work like `/model`, `/q` like `/exit`, `/perms` like `/permissions`
- Typo: `/modle` should suggest `/model`

- [ ] **Step 3: Test completion edge cases**

- Type `/` alone — should show all commands
- Type `/mo` — should show `/model`
- Type `/pro` — should show `/provider`, `/providers`
- Type `/xyz` — no completions
- Type `/model gpt` — should filter model completions to gpt-prefixed

- [ ] **Step 4: Commit final state if any fixes needed**

```bash
git add -A
git commit -m "fix(tui): post-migration fixes from e2e testing"
```
