# Yode Command System Redesign

**Date:** 2026-04-02  
**Status:** Approved  
**Scope:** Command layer (TUI) + Engine API

## Problem

Yode's current command system has all 21 slash commands defined in a single `commands.rs` file via a large match block, with completion limited to command-name prefix matching from a hardcoded array. There is no argument completion, no command aliases, no category grouping, and adding new commands requires modifying a monolithic function. This doesn't scale and falls short of the polish expected from a top-tier programming CLI.

## Goals

1. **Modular command architecture** — each command in its own file with a unified trait
2. **Argument completion** — commands declare their parameters; completions generated dynamically
3. **Command registry** — centralized registration, lookup by name/alias, category grouping
4. **New commands** — `/effort` (thinking depth) and `/permissions` (tool permission management)
5. **Engine API extensions** — support effort level, richer provider/model introspection

## Non-Goals

- Config file changes (no new TOML fields for command aliases or permissions persistence)
- Fuzzy search (prefix + substring matching is sufficient for now)
- Macro-based registration
- New UI rendering changes beyond existing completion popup

---

## Architecture

### Core Types

```rust
// crates/yode-tui/src/commands/mod.rs

/// How a command argument gets its completions.
/// CompletionContext is a read-only view (no &mut), safe to call on every keystroke.
pub enum ArgCompletionSource {
    /// No completions available
    None,
    /// Fixed list of valid values
    Static(Vec<String>),
    /// Generated at runtime from read-only app state
    Dynamic(fn(&CompletionContext) -> Vec<String>),
}

/// Defines one positional argument for a command
pub struct ArgDef {
    pub name: String,
    pub required: bool,
    pub hint: String,              // displayed in gray (e.g. "<model-name>")
    pub completions: ArgCompletionSource,
}

/// Command categories for /help grouping
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CommandCategory {
    Session,      // /clear, /compact, /sessions, /exit
    Model,        // /model, /provider, /providers, /effort
    Tools,        // /tools, /permissions
    Info,         // /help, /status, /cost, /version, /config, /context, /doctor
    Development,  // /diff, /bug
    Utility,      // /copy, /keys, /history, /time
}

/// Metadata every command must declare.
/// Stored as a field on each command struct, returned by reference to avoid allocation.
pub struct CommandMeta {
    pub name: &'static str,
    pub description: &'static str,
    pub aliases: &'static [&'static str],
    pub args: Vec<ArgDef>,
    pub category: CommandCategory,
    pub hidden: bool,
}

/// Result of command execution
pub type CommandResult = Result<CommandOutput, String>;

pub enum CommandOutput {
    Message(String),
    Messages(Vec<String>),
    Silent,
}

/// The trait every command implements
#[async_trait]
pub trait Command: Send + Sync {
    /// Returns metadata by reference. Each command stores its CommandMeta as a field,
    /// constructed once in new(), avoiding per-call allocations.
    fn meta(&self) -> &CommandMeta;
    async fn execute(&self, args: &str, ctx: &mut CommandContext) -> CommandResult;
}
```

### CommandContext

A single struct that gives commands access to everything they need. Since `AgentEngine` is behind
`Arc<Mutex<>>` (shared with the streaming task), `CommandContext` holds the Arc clone and commands
lock internally as needed. The context is constructed via `App::build_command_context()` which
destructures `self` to borrow fields independently.

```rust
// crates/yode-tui/src/commands/context.rs

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
}

/// Read-only context for completions. No &mut, safe on every keystroke.
/// Constructed cheaply from App state without mutable borrows.
pub struct CompletionContext<'a> {
    pub provider_models: &'a [String],
    pub all_provider_models: &'a HashMap<String, Vec<String>>,
    pub provider_name: &'a str,
    pub tools: &'a Arc<ToolRegistry>,
}
```

### CommandRegistry

```rust
// crates/yode-tui/src/commands/registry.rs

pub struct CommandRegistry {
    commands: Vec<Box<dyn Command>>,
    name_index: HashMap<String, usize>,  // name -> index
    // aliases also stored here, pointing to same index
}

impl CommandRegistry {
    pub fn new() -> Self;
    pub fn register(&mut self, cmd: Box<dyn Command>);
    pub fn find(&self, name: &str) -> Option<&dyn Command>;
    pub fn visible_commands(&self) -> Vec<&dyn Command>;
    pub fn by_category(&self) -> BTreeMap<CommandCategory, Vec<&dyn Command>>;

    /// Command name completion (prefix match, then substring fallback)
    pub fn complete_command(&self, prefix: &str) -> Vec<CommandSuggestion>;

    /// Argument completion. Parses args_so_far to determine position,
    /// then delegates to the correct ArgDef's completion source.
    pub fn complete_args(&self, cmd_name: &str, args_so_far: &[&str],
                         partial: &str, ctx: &CompletionContext) -> Vec<String>;

    /// Edit-distance suggestion for typos
    pub fn suggest_similar(&self, typo: &str) -> Option<String>;
}

pub struct CommandSuggestion {
    pub name: String,
    pub description: String,
    pub is_alias: bool,
}
```

---

## File Structure

```
crates/yode-tui/src/commands/
├── mod.rs              # Command trait, types, register_all()
├── registry.rs         # CommandRegistry
├── context.rs          # CommandContext
│
├── session/
│   ├── mod.rs
│   ├── clear.rs        # /clear [context]
│   ├── compact.rs      # /compact [keep_last]
│   ├── sessions.rs     # /sessions — list recent sessions
│   └── exit.rs         # /exit
│
├── model/
│   ├── mod.rs
│   ├── model.rs        # /model [name]
│   ├── provider.rs     # /provider <name>
│   ├── providers.rs    # /providers
│   └── effort.rs       # /effort [level]  (NEW)
│
├── tools/
│   ├── mod.rs
│   ├── tools.rs        # /tools
│   └── permissions.rs  # /permissions [tool] [allow|deny]  (NEW)
│
├── info/
│   ├── mod.rs
│   ├── help.rs         # /help
│   ├── status.rs       # /status
│   ├── cost.rs         # /cost
│   ├── version.rs      # /version
│   ├── config.rs       # /config
│   ├── context.rs      # /context
│   └── doctor.rs       # /doctor
│
├── dev/
│   ├── mod.rs
│   ├── diff.rs         # /diff
│   └── bug.rs          # /bug
│
└── utility/
    ├── mod.rs
    ├── copy.rs          # /copy
    ├── keys.rs          # /keys
    ├── history.rs       # /history [count]
    └── time.rs          # /time
```

Each command file is ~30-80 lines with a single struct implementing `Command`.

### Registration Pattern

```rust
// crates/yode-tui/src/commands/mod.rs

/// Register all built-in commands. Called once at App startup.
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
    // ... etc for all categories
}
```

Each command struct stores its `CommandMeta` as a field, built in `new()`:
```rust
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
```

---

## Completion System Changes

### Current Flow
1. `SLASH_COMMANDS` static array → prefix match on name → populate candidates
2. No argument completion

### New Flow
1. `CommandRegistry::complete_command(prefix)` → returns `Vec<CommandSuggestion>` (includes aliases)
2. When input has a space after command name → split completed args and partial → `CommandRegistry::complete_args(cmd, &args_so_far, partial, &completion_ctx)` → determines arg position from `args_so_far.len()`, delegates to correct `ArgDef`
3. `CommandCompletion` struct remains as UI-layer state manager but sources data from registry
4. `CompletionContext` (read-only) used for completions; `CommandContext` (mutable) only for execution

### Argument Completion Examples
| Command | Input | Completions |
|---------|-------|-------------|
| `/model ` | partial="" | `["gpt-4o", "claude-sonnet-4-20250514", ...]` (from provider_models) |
| `/model gpt` | partial="gpt" | `["gpt-4o", "gpt-4o-mini"]` |
| `/provider ` | partial="" | `["openai", "anthropic", "deepseek"]` |
| `/effort ` | partial="" | `["min", "low", "medium", "high", "max"]` |
| `/permissions ` | partial="" | `["bash", "write_file", "edit_file", ...]` (from tool registry) |
| `/permissions bash ` | partial="" | `["allow", "deny"]` |
| `/clear ` | partial="" | `["context"]` |

---

## New Commands

### /effort [level]

**Purpose:** Adjust AI thinking depth for the current session.

- No args → display current effort level
- With arg → set level, display confirmation
- Levels: `min`, `low`, `medium`, `high`, `max`
- Arg completion: Static list of levels
- Engine stores effort in `AgentContext`; passed to LLM as parameter or system prompt modifier

### /permissions [tool] [allow|deny|reset]

**Purpose:** View and manage tool execution permissions at runtime.

- No args → display current permission settings (which tools need confirmation)
- `/permissions bash allow` → add "bash" to auto-allow list (no confirmation needed)
- `/permissions bash deny` → add "bash" to deny list (always rejected)
- `/permissions reset` → restore defaults from config
- Arg 1 completion: Dynamic from tool registry names
- Arg 2 completion: Static `["allow", "deny"]`
- Modifies `PermissionManager` state in engine

---

## Engine API Extensions

```rust
// crates/yode-core/src/engine.rs

pub enum EffortLevel {
    Min,
    Low,
    Medium,  // default
    High,
    Max,
}

impl AgentEngine {
    // Existing (unchanged)
    pub fn set_model(&mut self, model: String);
    pub fn set_provider(&mut self, provider: Arc<dyn LlmProvider>, name: String);

    // New
    pub fn set_effort(&mut self, level: EffortLevel);
    pub fn effort(&self) -> EffortLevel;
    pub fn current_model(&self) -> &str;
    pub fn current_provider(&self) -> &str;
}
```

`EffortLevel` is stored in `AgentContext` and used during `run_turn_streaming` to adjust the request (e.g., mapped to `thinking` parameter for Anthropic, or temperature/max_tokens for OpenAI).

---

## Migration Plan

### What Gets Deleted
- `SLASH_COMMANDS` static array in `completion.rs`
- `handle_slash_command()` monolithic function in old `commands.rs`
- Typo suggestion logic in `app/mod.rs` moves to `CommandRegistry::suggest_similar()`

### What Gets Preserved
- `CommandCompletion` UI state struct (candidates, selected, cycle) — just changes data source
- `FileCompletion` — untouched
- Shell command (`!`) handling — stays in `app/mod.rs` (not a slash command, special prefix routing)
- File reference (`@`) handling — stays in `app/mod.rs` (same reason)
- `handle_shell_command()` stays in `app/mod.rs` as a standalone function (not part of CommandRegistry)
- Dynamic skill commands — registered into CommandRegistry at startup

### Backward Compatibility
- All 22 existing commands keep same names and behavior
- Completion UX stays the same (inline list below input)
- Shell (`!`) and file ref (`@`) syntax unchanged

---

## Testing Strategy

- Unit test each command's `execute()` with mock `CommandContext`
- Unit test `CommandRegistry` completion (prefix, substring, args)
- Integration test: verify all existing commands still work after migration
- Manual test: argument completion UX for `/model`, `/provider`, `/effort`, `/permissions`

---

## Success Criteria

1. All 22 existing commands work identically after refactor
2. `/model <tab>` completes with available model names
3. `/provider <tab>` completes with provider names
4. `/effort` and `/permissions` commands work as specified
5. `/help` displays commands grouped by category
6. Adding a new command requires only: create file, implement trait, register in `register_all()`
7. Typo suggestions still work (e.g., `/modle` → "Did you mean /model?")
