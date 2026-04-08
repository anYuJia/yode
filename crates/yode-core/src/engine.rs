use regex::Regex;
use std::pin::Pin;
use std::sync::Arc;

use anyhow::{Context as _, Result};
use tokio::sync::{mpsc, Mutex};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

use yode_llm::provider::LlmProvider;
use yode_llm::types::{
    ChatRequest, ChatResponse, Message, Role, StreamEvent, ToolCall,
    ToolDefinition as LlmToolDefinition,
};
use yode_tools::registry::ToolRegistry;
use yode_tools::state::TaskStore;
use yode_tools::tool::{SubAgentRunner, ToolContext, ToolErrorType, ToolResult, UserQuery, SubAgentOptions};
use yode_tools::validation;

use crate::context::{AgentContext, EffortLevel};
use crate::context_manager::ContextManager;
use crate::cost_tracker::CostTracker;
use crate::db::Database;
use crate::hooks::{HookContext, HookEvent, HookManager};
use crate::permission::{CommandClassifier, CommandRiskLevel, PermissionAction, PermissionManager};

/// Maximum size for tool results (50KB)
const MAX_TOOL_RESULT_SIZE: usize = 50 * 1024;

/// Maximum total size for all tool results in a single turn (200KB)
const MAX_TOTAL_TOOL_RESULTS_SIZE: usize = 200 * 1024;

/// LLM call timeout in seconds
const LLM_TIMEOUT_SECS: u64 = 120;

/// Maximum retry count for retryable errors
const MAX_RETRIES: u32 = 5;

/// Maximum retry count for rate-limit (429) errors
const MAX_RATE_LIMIT_RETRIES: u32 = 10;

/// Classify an error to determine retry strategy.
#[derive(Debug, Clone, Copy, PartialEq)]
enum ErrorKind {
    /// 429 Too Many Requests — retry with long backoff
    RateLimit,
    /// 500/502/503/504, timeout, network — retry with standard backoff
    Transient,
    /// 400/401/403/404 etc. — do not retry
    Fatal,
}

fn classify_error(err: &anyhow::Error) -> ErrorKind {
    let msg = format!("{:#}", err);
    if msg.contains("429") || msg.contains("rate_limit") || msg.contains("Too Many Requests") {
        ErrorKind::RateLimit
    } else if msg.contains("500") || msg.contains("502") || msg.contains("503") || msg.contains("504")
        || msg.contains("timeout") || msg.contains("超时") || msg.contains("timed out")
        || msg.contains("connection") || msg.contains("Connection")
        || msg.contains("ECONNRESET") || msg.contains("ECONNREFUSED")
        || msg.contains("Broken pipe") || msg.contains("reset by peer")
        || msg.contains("Failed to send") || msg.contains("failed to send")
        || msg.contains("dns error") || msg.contains("DNS error")
        || msg.contains("hyper") || msg.contains("reqwest")
        || msg.contains("network") || msg.contains("Network")
        || msg.contains("temporarily unavailable")
        || msg.contains("connect error") || msg.contains("Connect error")
    {
        ErrorKind::Transient
    } else {
        ErrorKind::Fatal
    }
}

/// Compute retry delay based on error kind and attempt number.
fn retry_delay(kind: ErrorKind, attempt: u32) -> std::time::Duration {
    match kind {
        // 429: 5s, 10s, 15s, 20s, 30s, 30s, …
        ErrorKind::RateLimit => {
            let secs = match attempt {
                0 => 5,
                1 => 10,
                2 => 15,
                3 => 20,
                _ => 30,
            };
            std::time::Duration::from_secs(secs)
        }
        // Transient: exponential backoff 2s, 4s, 8s, 16s, 16s with jitter
        ErrorKind::Transient => {
            let base_secs = 2u64.pow(attempt.min(4) + 1);
            let jitter = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() % 1000;
            std::time::Duration::from_millis((base_secs * 1000) + jitter as u64)
        }
        ErrorKind::Fatal => std::time::Duration::from_secs(0),
    }
}

/// Max retries for a given error kind.
fn max_retries_for(kind: ErrorKind) -> u32 {
    match kind {
        ErrorKind::RateLimit => MAX_RATE_LIMIT_RETRIES,
        ErrorKind::Transient => MAX_RETRIES,
        ErrorKind::Fatal => 0,
    }
}

/// Per-tool timeout for parallel execution (30 seconds)
const PARALLEL_TOOL_TIMEOUT_SECS: u64 = 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProjectKind {
    Rust,
    Node,
    Python,
    Mixed,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RecoveryState {
    Normal,
    ReanchorRequired,
    SingleStepMode,
    NeedUserGuidance,
}

/// Events emitted by the engine for the UI to consume.
#[derive(Debug, Clone)]
pub enum EngineEvent {
    /// LLM is thinking (stream started)
    Thinking,
    /// Real-time usage update (e.g. prompt tokens known at start)
    UsageUpdate(yode_llm::types::Usage),
    /// Incremental text from LLM
    TextDelta(String),
    /// Incremental reasoning/thought from LLM
    ReasoningDelta(String),
    /// LLM produced a complete text response
    TextComplete(String),
    /// LLM finished reasoning
    ReasoningComplete(String),
    /// Tool call initiated
    ToolCallStart {
        id: String,
        name: String,
        arguments: String,
    },
    /// Tool requires user confirmation
    ToolConfirmRequired {
        id: String,
        name: String,
        arguments: String,
    },
    /// Progress update from a tool
    ToolProgress {
        id: String,
        name: String,
        progress: yode_tools::tool::ToolProgress,
    },
    /// Tool execution result
    ToolResult {
        id: String,
        name: String,
        result: ToolResult,
    },
    /// Complete response from one LLM turn
    TurnComplete(ChatResponse),
    /// Error occurred
    Error(String),
    /// Retrying after a transient/rate-limit error
    Retrying {
        error_message: String,
        attempt: u32,
        max_attempts: u32,
        delay_secs: u64,
    },
    /// Tool is asking user a question
    AskUser {
        id: String,
        question: String,
    },
    /// Agent loop finished (no more tool calls)
    Done,
    /// Sub-agent started
    SubAgentStart { description: String },
    /// Sub-agent completed
    SubAgentComplete { result: String },
    /// Plan mode entered
    PlanModeEntered,
    /// Plan mode requires user approval
    PlanApprovalRequired { plan_content: String },
    /// Plan mode exited
    PlanModeExited,
    /// Context window was compressed to fit within limits
    ContextCompressed { removed: usize },
    /// Cost update after API call
    CostUpdate {
        estimated_cost: f64,
        input_tokens: u64,
        output_tokens: u64,
    },
    /// Budget limit exceeded
    BudgetExceeded { cost: f64, limit: f64 },
    /// Suggestion generated (async LLM call completed)
    SuggestionReady { suggestion: String },
    /// App update is available
    UpdateAvailable(String),
    /// App update is downloading
    UpdateDownloading,
    /// App update is downloaded
    UpdateDownloaded(String),
}

/// Response to a confirmation request.
#[derive(Debug, Clone)]
pub enum ConfirmResponse {
    Allow,
    Deny,
}

/// Tool call budget thresholds for analysis guidance.
const TOOL_BUDGET_NOTICE: u32 = 15;
const TOOL_BUDGET_WARNING: u32 = 25;
/// Self-reflection injection point.
const TOOL_REFLECT_INTERVAL: u32 = 10;
/// Goal reminder injection point.
const TOOL_GOAL_REMINDER: u32 = 5;

/// The core agent engine that drives the conversation loop.
pub struct AgentEngine {
    provider: Arc<dyn LlmProvider>,
    tools: Arc<ToolRegistry>,
    permissions: PermissionManager,
    context: AgentContext,
    messages: Vec<Message>,
    #[allow(dead_code)]
    system_prompt: String,
    db: Option<Database>,
    /// Shared task store for the todo tool.
    task_store: Arc<Mutex<TaskStore>>,
    /// Channel for ask_user questions (engine → TUI).
    ask_user_tx: Option<mpsc::UnboundedSender<UserQuery>>,
    /// Channel for ask_user answers (TUI → engine).
    ask_user_rx: Option<Arc<Mutex<mpsc::UnboundedReceiver<String>>>>,
    /// Tool call counter for the current turn (budget tracking).
    tool_call_count: u32,
    /// Recent tool call signatures for dedup detection (name+args hash).
    recent_tool_calls: Vec<(String, String)>,
    /// Consecutive tool call failure counter.
    consecutive_failures: u32,
    /// Total bytes of tool results in the current turn.
    total_tool_results_bytes: usize,
    /// Counter for protocol violation retries.
    violation_retries: u32,
    /// Context window manager for automatic compression.
    context_manager: ContextManager,
    /// Cost tracker for token usage and estimated cost.
    cost_tracker: CostTracker,
    /// Hook manager for pre/post tool use hooks.
    hook_manager: Option<HookManager>,
    /// Files the agent has already read in this turn (path → line count).
    files_read: std::collections::HashMap<String, usize>,
    /// Files the agent has modified in this turn.
    files_modified: Vec<String>,
    /// Error buckets for state-machine recovery (Type -> Count).
    error_buckets: std::collections::HashMap<ToolErrorType, u32>,
    /// Last failed path/command to detect identical retry loops.
    last_failed_signature: Option<String>,
    /// Whether the engine is in plan mode.
    plan_mode: Arc<Mutex<bool>>,
    /// Detected project kind for current session root.
    project_kind: ProjectKind,
    /// Unified recovery state for tool-call orchestration.
    recovery_state: RecoveryState,
}

/// Convert yode-tools ToolDefinition to yode-llm ToolDefinition.
fn convert_tool_definitions(registry: &ToolRegistry) -> Vec<LlmToolDefinition> {
    registry
        .definitions()
        .into_iter()
        .map(|td| LlmToolDefinition {
            name: td.name,
            description: td.description,
            parameters: td.parameters,
        })
        .collect()
}

/// Truncate tool result if it exceeds the size limit.
/// Preserves the beginning and end of the result for better context.
fn truncate_tool_result(result: ToolResult) -> ToolResult {
    if result.content.len() > MAX_TOOL_RESULT_SIZE {
        let head_size = MAX_TOOL_RESULT_SIZE * 3 / 4; // 75% from start
        let tail_size = MAX_TOOL_RESULT_SIZE / 4;       // 25% from end
        let head: String = result.content.chars().take(head_size).collect();
        let tail: String = result.content.chars().rev().take(tail_size).collect::<String>().chars().rev().collect();
        ToolResult {
            content: format!(
                "{}\n\n... [截断: 原始 {} 字节，使用 read_file 的 offset/limit 查看完整内容] ...\n\n{}",
                head,
                result.content.len(),
                tail
            ),
            is_error: result.is_error,
            error_type: result.error_type,
            recoverable: result.recoverable,
            suggestion: result.suggestion,
            metadata: result.metadata,
        }
    } else {
        result
    }
}

impl AgentEngine {
    fn detect_project_kind(root: &std::path::Path) -> ProjectKind {
        let has_cargo = root.join("Cargo.toml").exists();
        let has_node = root.join("package.json").exists();
        let has_python = root.join("pyproject.toml").exists() || root.join("requirements.txt").exists();

        match (has_cargo, has_node, has_python) {
            (true, false, false) => ProjectKind::Rust,
            (false, true, false) => ProjectKind::Node,
            (false, false, true) => ProjectKind::Python,
            (false, false, false) => ProjectKind::Unknown,
            _ => ProjectKind::Mixed,
        }
    }

    fn update_recovery_state(&mut self) {
        let not_found = *self.error_buckets.get(&ToolErrorType::NotFound).unwrap_or(&0);
        let validation = *self.error_buckets.get(&ToolErrorType::Validation).unwrap_or(&0);
        let timeout = *self.error_buckets.get(&ToolErrorType::Timeout).unwrap_or(&0);

        self.recovery_state = if self.consecutive_failures >= 3 {
            RecoveryState::NeedUserGuidance
        } else if validation >= 2 || timeout >= 2 || self.consecutive_failures >= 2 {
            RecoveryState::SingleStepMode
        } else if not_found >= 2 {
            RecoveryState::ReanchorRequired
        } else {
            RecoveryState::Normal
        };
    }

    fn language_command_mismatch(&self, tool_name: &str, params: &serde_json::Value) -> Option<String> {
        if tool_name != "bash" {
            return None;
        }

        let cmd = params
            .get("command")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_lowercase();

        if cmd.is_empty() {
            return None;
        }

        let starts_with_cargo = cmd.starts_with("cargo ");
        let starts_with_npm = cmd.starts_with("npm ") || cmd.starts_with("pnpm ") || cmd.starts_with("yarn ") || cmd.starts_with("bun ");

        match self.project_kind {
            ProjectKind::Node if starts_with_cargo => Some("Project appears to be Node/TypeScript. Avoid cargo commands until Rust root is verified.".to_string()),
            ProjectKind::Rust if starts_with_npm => Some("Project appears to be Rust. Avoid npm/pnpm/yarn commands until Node root is verified.".to_string()),
            _ => None,
        }
    }

    pub fn new(
        provider: Arc<dyn LlmProvider>,
        tools: Arc<ToolRegistry>,
        permissions: PermissionManager,
        context: AgentContext,
    ) -> Self {
        let mut system_prompt = include_str!("../../../prompts/system.md").to_string();

        // Inject runtime environment info
        system_prompt.push_str("\n\n# Environment\n\n");
        
        // Block to safely get initial runtime state
        let (cwd, project_root) = {
            // We use a sync-friendly way here for initial prompt or just use the root
            // Since rt is new, we know it matches the root
            (context.working_dir_compat(), context.working_dir_compat())
        };

        system_prompt.push_str(&format!(
            "- Working directory: {}\n- Project root: {}\n- Platform: {} ({})\n- Date: {}\n- Model: {}\n- Provider: {}\n",
            cwd.display(),
            project_root.display(),
            std::env::consts::OS,
            std::env::consts::ARCH,
            chrono::Local::now().format("%Y-%m-%d"),
            context.model,
            context.provider,
        ));

        // Inject git status if in a git repo
        if cwd.join(".git").exists() {
            system_prompt.push_str("- Git repo: yes\n");
            if let Ok(output) = std::process::Command::new("git")
                .args(["branch", "--show-current"])
                .current_dir(&cwd)
                .output()
            {
                let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !branch.is_empty() {
                    system_prompt.push_str(&format!("- Branch: {}\n", branch));
                }
            }
        }

        let cwd = context.working_dir_compat();
        // Try to load project-level documentation (YODE.md, CLAUDE.md, etc.)
        system_prompt.push_str(&AgentEngine::get_project_docs_static(&cwd));

        // Inject memory files if they exist
        if let Some(memory_content) = AgentEngine::get_memory_files_static(&cwd) {
            system_prompt.push_str("\n\n");
            system_prompt.push_str(&memory_content);
        }

        // Inject output style instructions if not default
        if context.output_style != "default" {
            system_prompt.push_str("\n\n# Output Style\n\n");
            match context.output_style.as_str() {
                "explanatory" => {
                    system_prompt.push_str("You are in **Explanatory Mode**. Before and after writing code, provide brief educational insights about implementation choices.\n");
                    system_prompt.push_str("Include 2-3 key educational points explaining WHY you chose this approach.\n");
                    system_prompt.push_str("These insights should be in the conversation, not in the codebase.\n");
                }
                "learning" => {
                    system_prompt.push_str("You are in **Learning Mode**. Help the user learn through hands-on practice.\n");
                    system_prompt.push_str("- Request user input for meaningful design decisions\n");
                    system_prompt.push_str("- Ask the user to write small code pieces (2-10 lines) for key decisions\n");
                    system_prompt.push_str("- Frame contributions as valuable design decisions, not busy work\n");
                    system_prompt.push_str("- Wait for user implementation before proceeding\n");
                }
                _ => {}
            }
        }

        let mut messages = Vec::new();
        messages.push(Message::system(&system_prompt));

        let context_manager = ContextManager::new(&context.model);
        let cost_tracker = CostTracker::new(&context.model);

        let detected_project_kind = Self::detect_project_kind(&context.working_dir_compat());

        Self {
            provider,
            tools,
            permissions,
            context,
            messages,
            system_prompt,
            db: None,
            task_store: Arc::new(Mutex::new(TaskStore::new())),
            ask_user_tx: None,
            ask_user_rx: None,
            tool_call_count: 0,
            recent_tool_calls: Vec::new(),
            consecutive_failures: 0,
            total_tool_results_bytes: 0,
            violation_retries: 0,
            context_manager,
            cost_tracker,
            hook_manager: None,
            files_read: std::collections::HashMap::new(),
            files_modified: Vec::new(),
            error_buckets: std::collections::HashMap::new(),
            last_failed_signature: None,
            plan_mode: Arc::new(Mutex::new(false)),
            project_kind: detected_project_kind,
            recovery_state: RecoveryState::Normal,
        }
    }

    /// Set the database for session persistence.
    pub fn set_database(&mut self, db: Database) {
        self.db = Some(db);
    }

    /// Switch the model at runtime.
    pub fn set_model(&mut self, model: String) {
        self.context.model = model;
        self.context_manager = ContextManager::new(&self.context.model);
        self.rebuild_system_prompt();
    }

    /// Switch the provider at runtime.
    pub fn set_provider(&mut self, provider: Arc<dyn LlmProvider>, name: String) {
        self.provider = provider;
        self.context.provider = name;
        self.rebuild_system_prompt();
    }

    /// Rebuild the system prompt with current context (model, provider, etc.)
    /// and update the first message in the conversation history.
    fn rebuild_system_prompt(&mut self) {
        let mut system_prompt = include_str!("../../../prompts/system.md").to_string();
        let cwd = self.context.working_dir_compat();

        system_prompt.push_str("\n\n# Environment\n\n");
        system_prompt.push_str(&format!(
            "- Working directory: {}\n- Project root: {}\n- Platform: {} ({})\n- Date: {}\n- Model: {}\n- Provider: {}\n",
            cwd.display(),
            cwd.display(), // For now project root is same as initial cwd
            std::env::consts::OS,
            std::env::consts::ARCH,
            chrono::Local::now().format("%Y-%m-%d"),
            self.context.model,
            self.context.provider,
        ));

        if cwd.join(".git").exists() {
            system_prompt.push_str("- Git repo: yes\n");
            if let Ok(output) = std::process::Command::new("git")
                .args(["branch", "--show-current"])
                .current_dir(&cwd)
                .output()
            {
                let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !branch.is_empty() {
                    system_prompt.push_str(&format!("- Branch: {}\n", branch));
                }
            }
        }

        let yode_md = cwd.join("YODE.md");
        if yode_md.exists() {
            if let Ok(override_prompt) = std::fs::read_to_string(&yode_md) {
                system_prompt.push_str("\n\n# Project-specific instructions\n\n");
                system_prompt.push_str(&override_prompt);
            }
        }

        // Inject project-level documentation (YODE.md, CLAUDE.md, etc.)
        system_prompt.push_str(&AgentEngine::get_project_docs_static(&cwd));

        // Inject memory files if they exist
        if let Some(memory_content) = AgentEngine::get_memory_files_static(&cwd) {
            system_prompt.push_str("\n\n");
            system_prompt.push_str(&memory_content);
        }

        // Inject output style instructions if not default
        if self.context.output_style != "default" {
            system_prompt.push_str("\n\n# Output Style\n\n");
            match self.context.output_style.as_str() {
                "explanatory" => {
                    system_prompt.push_str("You are in **Explanatory Mode**. Before and after writing code, provide brief educational insights about implementation choices.\n");
                    system_prompt.push_str("Include 2-3 key educational points explaining WHY you chose this approach.\n");
                    system_prompt.push_str("These insights should be in the conversation, not in the codebase.\n");
                }
                "learning" => {
                    system_prompt.push_str("You are in **Learning Mode**. Help the user learn through hands-on practice.\n");
                    system_prompt.push_str("- Request user input for meaningful design decisions\n");
                    system_prompt.push_str("- Ask the user to write small code pieces (2-10 lines) for key decisions\n");
                    system_prompt.push_str("- Frame contributions as valuable design decisions, not busy work\n");
                    system_prompt.push_str("- Wait for user implementation before proceeding\n");
                }
                _ => {}
            }
        }

        self.system_prompt = system_prompt.clone();

        // Update the system message in conversation history
        if let Some(first) = self.messages.first_mut() {
            if matches!(first.role, Role::System) {
                first.content = Some(system_prompt);
            }
        }
    }

    pub fn set_effort(&mut self, level: EffortLevel) { self.context.effort = level; }
    pub fn effort(&self) -> EffortLevel { self.context.effort }
    pub fn current_model(&self) -> &str { &self.context.model }
    pub fn current_provider(&self) -> &str { &self.context.provider }
    pub fn permissions(&self) -> &PermissionManager { &self.permissions }
    pub fn permissions_mut(&mut self) -> &mut PermissionManager { &mut self.permissions }
    pub fn cost_tracker(&self) -> &CostTracker { &self.cost_tracker }
    pub fn cost_tracker_mut(&mut self) -> &mut CostTracker { &mut self.cost_tracker }
    pub fn get_database(&self) -> Option<&Database> { self.db.as_ref() }

    /// Set hook manager.
    pub fn set_hook_manager(&mut self, mgr: HookManager) {
        self.hook_manager = Some(mgr);
    }

    /// Set channels for the ask_user tool.
    pub fn set_ask_user_channels(
        &mut self,
        tx: mpsc::UnboundedSender<UserQuery>,
        rx: mpsc::UnboundedReceiver<String>,
    ) {
        self.ask_user_tx = Some(tx);
        self.ask_user_rx = Some(Arc::new(Mutex::new(rx)));
    }

    /// Build a ToolContext with access to shared resources.
    async fn build_tool_context(
        &self,
        progress_tx: Option<mpsc::UnboundedSender<yode_tools::tool::ToolProgress>>,
    ) -> ToolContext {
        let cwd = self.context.runtime.lock().await.cwd.clone();
        
        ToolContext {
            registry: Some(Arc::clone(&self.tools)),
            tasks: Some(Arc::clone(&self.task_store)),
            user_input_tx: self.ask_user_tx.clone(),
            user_input_rx: self.ask_user_rx.clone(),
            progress_tx,
            working_dir: Some(cwd),
            sub_agent_runner: None,
            mcp_resources: None,
            cron_manager: None,
            lsp_manager: None,
            worktree_state: None,
            read_file_history: Some(Arc::new(tokio::sync::Mutex::new(std::collections::HashSet::new()))),
            plan_mode: Some(Arc::clone(&self.plan_mode)),
        }
    }

    /// Get project documentation (YODE.md, CLAUDE.md, etc.)
    fn get_project_docs_static(project_root: &std::path::Path) -> String {
        let mut docs = Vec::new();

        // Check for project-specific instruction files in priority order
        let doc_patterns = [
            "YODE.md",
            "CLAUDE.md",
            "GEMINI.md",
            "AGENTS.md",
            ".yode/instructions.md",
            "docs/YODE.md",
            "docs/CLAUDE.md",
        ];

        for pattern in &doc_patterns {
            let path = project_root.join(pattern);
            if path.exists() {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    docs.push(format!("## From: {}\n\n{}", pattern, content));
                    info!("Loaded project documentation: {}", path.display());
                }
            }
        }

        if docs.is_empty() {
            return String::new();
        }

        let mut result = String::new();
        result.push_str("\n\n# Project Documentation\n\n");
        result.push_str("The following instructions are from project documentation files.\n\n");
        result.push_str(&docs.join("\n\n"));
        result
    }

    /// Get memory files content if they exist
    fn get_memory_files_static(project_root: &std::path::Path) -> Option<String> {
        let memory_dir = project_root.join("memory");
        if !memory_dir.exists() {
            return None;
        }

        let mut memory_contents = Vec::new();

        if let Ok(entries) = std::fs::read_dir(&memory_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("md") {
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        let file_name = path.file_name()
                            .and_then(|s| s.to_str())
                            .unwrap_or("unknown");
                        memory_contents.push(format!("## {}\n\n{}", file_name, content));
                        info!("Loaded memory file: {}", path.display());
                    }
                }
            }
        }

        if memory_contents.is_empty() {
            return None;
        }

        let mut result = String::new();
        result.push_str("# User Memory\n\n");
        result.push_str("The following are user memory notes for this project.\n\n");
        result.push_str(&memory_contents.join("\n\n"));
        Some(result)
    }

    /// Get the current message history.
    pub fn messages(&self) -> &[Message] {
        &self.messages
    }

    /// Get the context.
    pub fn context(&self) -> &AgentContext {
        &self.context
    }

    /// Restore messages from database for a resumed session.
    pub fn restore_messages(&mut self, messages: Vec<Message>) {
        // Keep the system prompt as the first message, then append restored messages
        let system_msg = self.messages.first().cloned();
        self.messages.clear();
        if let Some(sys) = system_msg {
            self.messages.push(sys);
        }
        self.messages.extend(messages);
        info!("Restored {} messages from database", self.messages.len() - 1);
    }

    /// Clear conversation history, keeping only the system prompt.
    pub fn clear_conversation(&mut self) {
        // Keep only the system prompt (first message)
        if self.messages.len() > 1 {
            let system_msg = self.messages.first().cloned();
            self.messages.clear();
            if let Some(sys) = system_msg {
                self.messages.push(sys);
            }
            info!("Cleared conversation, kept system prompt");
        }
    }

    /// Save a message to the database if available.
    fn persist_message(&self, role: &str, content: Option<&str>, reasoning: Option<&str>, tool_calls_json: Option<&str>, tool_call_id: Option<&str>) {
        if let Some(ref db) = self.db {
            if let Err(e) = db.save_message(&self.context.session_id, role, content, reasoning, tool_calls_json, tool_call_id) {
                warn!("Failed to persist message: {}", e);
            }
            if let Err(e) = db.touch_session(&self.context.session_id) {
                warn!("Failed to touch session: {}", e);
            }
        }
    }

    /// Run one user turn: send user message, loop through tool calls until final text response.
    pub async fn run_turn(
        &mut self,
        user_input: &str,
        event_tx: mpsc::UnboundedSender<EngineEvent>,
        mut confirm_rx: mpsc::UnboundedReceiver<ConfirmResponse>,
        ) -> Result<()> {
        let _ = event_tx.send(EngineEvent::Thinking);

        // Add user message
        self.messages.push(Message::user(user_input));
        self.persist_message("user", Some(user_input), None, None, None);

        // Reset tool call budget counter for this turn
        self.tool_call_count = 0;
        self.recent_tool_calls.clear();
        self.consecutive_failures = 0;
        self.total_tool_results_bytes = 0;
        self.violation_retries = 0;
        self.files_read.clear();
        self.files_modified.clear();
        self.error_buckets.clear();
        self.last_failed_signature = None;
        self.update_recovery_state();
        self.error_buckets.clear();
        self.last_failed_signature = None;
        self.update_recovery_state();

        loop {
            let _ = event_tx.send(EngineEvent::Thinking);

            // Build chat request
            let request = ChatRequest {
                model: self.context.model.clone(),
                messages: self.messages.clone(),
                tools: convert_tool_definitions(&self.tools),
                temperature: Some(0.7),
                max_tokens: Some(self.context.get_max_tokens()),
            };

            // Call LLM with timeout and retry
            let response = self.call_llm_with_retry(request).await?;

            // Track cost
            self.cost_tracker.record_usage(
                response.usage.prompt_tokens as u64,
                response.usage.completion_tokens as u64,
            );

            // Emit cost update
            let _ = event_tx.send(EngineEvent::CostUpdate {
                estimated_cost: self.cost_tracker.estimated_cost(),
                input_tokens: self.cost_tracker.usage().input_tokens,
                output_tokens: self.cost_tracker.usage().output_tokens,
            });

            // Check budget
            if self.cost_tracker.is_over_budget() {
                let _ = event_tx.send(EngineEvent::BudgetExceeded {
                    cost: self.cost_tracker.estimated_cost(),
                    limit: self.cost_tracker.remaining_budget().unwrap_or(0.0),
                });
            }

            // Check if context window needs compression (should_compress caches token count)
            if self.context_manager.should_compress(response.usage.prompt_tokens, &self.messages) {
                let removed = self.context_manager.compress(&mut self.messages);
                if removed > 0 {
                    let _ = event_tx.send(EngineEvent::ContextCompressed { removed });
                }
            }

            debug!(
                "LLM response: text={:?}, tool_calls={}",
                response.message.content.as_deref().unwrap_or(""),
                response.message.tool_calls.len()
            );

            // Add assistant message to history (with tag cleaning)
            let mut assistant_msg = response.message.clone();

            // Handle StopReason::MaxTokens
            if response.stop_reason == Some(yode_llm::types::StopReason::MaxTokens) {
                let warning = "\n\n[WARNING: Response truncated due to max_tokens limit. Consider increasing effort level if more detail is needed.]";
                if let Some(content) = &mut assistant_msg.content {
                    content.push_str(warning);
                } else {
                    assistant_msg.content = Some(warning.to_string());
                }
                warn!("LLM response truncated due to max_tokens");
            } else if response.stop_reason == Some(yode_llm::types::StopReason::StopSequence) || matches!(response.stop_reason, Some(yode_llm::types::StopReason::Other(_))) {
                if let Some(ref content) = assistant_msg.content {
                    if content.contains("[tool_") || content.contains("<tool_") {
                        warn!("LLM response stopped via stop sequence or other reason but contains incomplete tool tags. Reason: {:?}", response.stop_reason);
                        // Future improvement: auto-close JSON braces for partial tool calls
                    }
                }
            }

            if let Some(ref content) = assistant_msg.content {
                if content.contains("[tool_use") || content.contains("[DUMMY_TOOL") {
                    assistant_msg.content = Some(self.clean_assistant_response(content));
                }
            }

            // Safety gate: prevent malformed assistant history growth when model emits
            // pseudo tool calls in text but no structured tool_calls metadata.
            if assistant_msg.tool_calls.is_empty() {
                if let Some(content) = assistant_msg.content.clone() {
                    let recovered = self.recover_leaked_tool_calls(&content);
                    if !recovered.is_empty() {
                        info!("Recovered {} leaked tool calls from text response (non-streaming).", recovered.len());
                        assistant_msg.tool_calls = recovered;
                        self.violation_retries = 0;
                    } else if self.is_protocol_violation(&content) {
                        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
                        let bucket = self.error_buckets.entry(ToolErrorType::Protocol).or_insert(0);
                        *bucket += 1;
                    }
                }
            } else {
                self.violation_retries = 0;
            }

            self.messages.push(assistant_msg.clone());

            // Persist assistant message
            let tc_json = if !assistant_msg.tool_calls.is_empty() {
                serde_json::to_string(&assistant_msg.tool_calls).ok()
            } else {
                None
            };
            self.persist_message(
                "assistant",
                assistant_msg.content.as_deref(),
                assistant_msg.reasoning.as_deref(),
                tc_json.as_deref(),
                None,
            );

            // If there are tool calls, execute them (parallel where possible)
            if !assistant_msg.tool_calls.is_empty() {
                debug!(
                    "Tool batch incoming: total={}, consecutive_failures={}, recent_calls={}",
                    assistant_msg.tool_calls.len(),
                    self.consecutive_failures,
                    self.recent_tool_calls.len()
                );
                let (parallel, sequential) = self.partition_tool_calls(&assistant_msg.tool_calls);

                // Execute parallel tools concurrently
                let parallel_results = if !parallel.is_empty() {
                    info!("Executing {} tools in parallel", parallel.len());
                    self.execute_tools_parallel(&parallel, &event_tx).await
                } else {
                    vec![]
                };

                // Process parallel results
                for (tc, result) in &parallel_results {
                    let mut result = result.clone();
                    self.track_file_access(&tc.name, &result);
                    result = truncate_tool_result(result);
                    self.enforce_tool_budget(&mut result);

                    self.inject_intelligence(&mut result, &tc.name, &tc.arguments);

                    self.messages.push(Message::tool_result(&tc.id, &result.content));
                    self.persist_message("tool", Some(&result.content), None, None, Some(&tc.id));

                    let _ = event_tx.send(EngineEvent::ToolResult {
                        id: tc.id.clone(),
                        name: tc.name.clone(),
                        result,
                    });
                }

                // Execute sequential tools one by one
                for tool_call in &sequential {
                    let mut result = self
                        .handle_tool_call(tool_call, &event_tx, &mut confirm_rx, None)
                        .await?;

                    self.track_file_access(&tool_call.name, &result);
                    result = truncate_tool_result(result);
                    self.enforce_tool_budget(&mut result);

                    self.inject_intelligence(&mut result, &tool_call.name, &tool_call.arguments);

                    self.messages.push(Message::tool_result(&tool_call.id, &result.content));
                    self.persist_message("tool", Some(&result.content), None, None, Some(&tool_call.id));

                    let _ = event_tx.send(EngineEvent::ToolResult {
                        id: tool_call.id.clone(),
                        name: tool_call.name.clone(),
                        result,
                    });
                }

                continue;
            }

            // No tool calls — this is a text response, we're done
            if let Some(text) = &response.message.content {
                // Evidence gate: when we had repeated failures and no successful reads,
                // avoid overconfident finalization without grounding.
                if self.consecutive_failures >= 2 && self.files_read.is_empty() {
                    let guarded = format!(
                        "{}\n\n[EVIDENCE GATE: Multiple failures occurred and no successful file reads were recorded in this turn. Summarize verified facts only and ask for directory/path confirmation before concluding.]",
                        text
                    );
                    let _ = event_tx.send(EngineEvent::TextComplete(guarded));
                } else {
                    let _ = event_tx.send(EngineEvent::TextComplete(text.clone()));
                }
            }

            let _ = event_tx.send(EngineEvent::TurnComplete(response));
            let _ = event_tx.send(EngineEvent::Done);
            break;
        }

        Ok(())
    }

    /// Run one user turn with streaming LLM output.
    /// Accepts an optional CancellationToken for cooperative cancellation.
    pub async fn run_turn_streaming(
        &mut self,
        user_input: &str,
        event_tx: mpsc::UnboundedSender<EngineEvent>,
        mut confirm_rx: mpsc::UnboundedReceiver<ConfirmResponse>,
        cancel_token: Option<CancellationToken>,
    ) -> Result<()> {
        self.messages.push(Message::user(user_input));
        self.persist_message("user", Some(user_input), None, None, None);

        // Reset tool call budget counter for this turn
        self.tool_call_count = 0;
        self.recent_tool_calls.clear();
        self.consecutive_failures = 0;
        self.total_tool_results_bytes = 0;
        self.violation_retries = 0;
        self.files_read.clear();
        self.files_modified.clear();

        loop {
            // Check cancellation before each LLM call
            if let Some(ref token) = cancel_token {
                if token.is_cancelled() {
                    let _ = event_tx.send(EngineEvent::Done);
                    return Ok(());
                }
            }

            let _ = event_tx.send(EngineEvent::Thinking);

            let request = ChatRequest {
                model: self.context.model.clone(),
                messages: self.messages.clone(),
                tools: convert_tool_definitions(&self.tools),
                temperature: Some(0.7),
                max_tokens: Some(self.context.get_max_tokens()),
            };

            // Stream LLM response with timeout
            let (stream_tx, mut stream_rx) = mpsc::channel::<StreamEvent>(256);

            let turn_start = std::time::Instant::now();
            // Hard guard for "ruminating" stalls in streaming mode. Extended for reasoning models.
            let hard_turn_timeout = std::time::Duration::from_secs(600);

            let provider = self.provider.clone();
            let stream_handle = tokio::spawn(async move {
                let result = tokio::time::timeout(
                    std::time::Duration::from_secs(LLM_TIMEOUT_SECS.max(600)),
                    provider.chat_stream(request, stream_tx),
                ).await;
                match result {
                    Ok(inner) => inner,
                    Err(_) => Err(anyhow::anyhow!("LLM 调用超时 ({}秒)", LLM_TIMEOUT_SECS.max(600))),
                }
            });

            let mut full_text = String::new();
            let mut full_reasoning = String::new();
            let mut tool_calls: Vec<ToolCall> = Vec::new();
            let mut final_response: Option<ChatResponse> = None;
            let mut cancelled = false;
            let mut stalled = false;
            let mut last_progress_at = std::time::Instant::now();
            let stall_timeout = std::time::Duration::from_secs(120);

            loop {
                // Global per-turn timeout guard: stop endless streaming/ruminating.
                if turn_start.elapsed() > hard_turn_timeout {
                    warn!("Streaming turn timed out after {:?}; forcing completion", hard_turn_timeout);
                    stream_handle.abort();
                    stalled = true;
                    break;
                }

                // Watchdog: no progress events for too long => force break.
                if last_progress_at.elapsed() > stall_timeout {
                    warn!("Streaming stalled for {:?} without progress; forcing completion", stall_timeout);
                    stream_handle.abort();
                    stalled = true;
                    break;
                }

                if let Some(ref token) = cancel_token {
                    tokio::select! {
                        event = stream_rx.recv() => {
                            match event {
                                Some(ev) => {
                                    last_progress_at = std::time::Instant::now();
                                    let is_done = matches!(ev, StreamEvent::Done(_));
                                    Self::process_stream_event(ev, &mut full_text, &mut full_reasoning, &mut tool_calls, &mut final_response, &event_tx);
                                    if is_done { break; }
                                }
                                None => break,
                            }
                        }
                        _ = token.cancelled() => {
                            cancelled = true;
                            stream_handle.abort();
                            break;
                        }
                        _ = tokio::time::sleep(std::time::Duration::from_secs(2)) => {
                            // Emit heartbeat while waiting for token
                            let _ = event_tx.send(EngineEvent::Thinking);
                        }
                    }
                } else {
                    match tokio::time::timeout(std::time::Duration::from_secs(2), stream_rx.recv()).await {
                        Ok(Some(ev)) => {
                            last_progress_at = std::time::Instant::now();
                            let is_done = matches!(ev, StreamEvent::Done(_));
                            Self::process_stream_event(ev, &mut full_text, &mut full_reasoning, &mut tool_calls, &mut final_response, &event_tx);
                            if is_done { break; }
                        }
                        Ok(None) => break,
                        Err(_) => {
                            // periodic wake-up to re-check hard timeout and emit heartbeat
                            let _ = event_tx.send(EngineEvent::Thinking);
                        }
                    }
                }
            }

            if cancelled || stalled {
                // Save partial text if any
                if !full_text.is_empty() || !full_reasoning.is_empty() {
                    let mut blocks = Vec::new();
                    if !full_reasoning.is_empty() {
                        blocks.push(yode_llm::types::ContentBlock::Thinking { thinking: full_reasoning.clone(), signature: None });
                    }
                    if !full_text.is_empty() {
                        blocks.push(yode_llm::types::ContentBlock::Text { text: full_text.clone() });
                    }

                    let assistant_msg = Message {
                        role: Role::Assistant,
                        content: if full_text.is_empty() { None } else { Some(full_text.clone()) },
                        reasoning: if full_reasoning.is_empty() { None } else { Some(full_reasoning.clone()) },
                        content_blocks: blocks,
                        tool_calls: vec![],
                        tool_call_id: None,
                        images: Vec::new(),
                    };
                    self.messages.push(assistant_msg);
                    self.persist_message(
                        "assistant",
                        if full_text.is_empty() { None } else { Some(&full_text) },
                        if full_reasoning.is_empty() { None } else { Some(&full_reasoning) },
                        None,
                        None
                    );
                }
                if stalled {
                    let _ = event_tx.send(EngineEvent::TextComplete(
                        "[Watchdog] Streaming stalled; forcing graceful stop. Please retry with narrower scope.".to_string(),
                    ));
                }
                let _ = event_tx.send(EngineEvent::Done);
                return Ok(());
            }

            // Wait for stream task and check for errors
            if !cancelled {
                let stream_result = stream_handle.await;
                let stream_err = match stream_result {
                    Ok(Ok(())) => None,
                    Ok(Err(e)) => {
                        warn!("Stream failed: {}", e);
                        Some(e)
                    }
                    Err(e) => {
                        warn!("Stream task panicked: {}", e);
                        Some(anyhow::anyhow!("Stream task error: {}", e))
                    }
                };

                // If stream failed with no content, retry with backoff
                if let Some(err) = stream_err {
                    if full_text.is_empty() && tool_calls.is_empty() {
                        let kind = classify_error(&err);
                        if kind != ErrorKind::Fatal {
                            let max_attempts = max_retries_for(kind);
                            let mut retry_succeeded = false;

                            for attempt in 0..max_attempts {
                                let delay = retry_delay(kind, attempt);
                                let total_secs = delay.as_secs();

                                // Countdown: update UI every second
                                for remaining in (0..=total_secs).rev() {
                                    let _ = event_tx.send(EngineEvent::Retrying {
                                        error_message: format!("{}", err),
                                        attempt: attempt + 1,
                                        max_attempts,
                                        delay_secs: remaining,
                                    });
                                    if remaining > 0 {
                                        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                                        // Check cancellation during countdown
                                        if let Some(ref token) = cancel_token {
                                            if token.is_cancelled() {
                                                let _ = event_tx.send(EngineEvent::Done);
                                                return Ok(());
                                            }
                                        }
                                    }
                                }
                                info!("Retrying stream (attempt {}/{})", attempt + 1, max_attempts);

                                // Check cancellation before retry
                                if let Some(ref token) = cancel_token {
                                    if token.is_cancelled() {
                                        let _ = event_tx.send(EngineEvent::Done);
                                        return Ok(());
                                    }
                                }

                                let retry_request = ChatRequest {
                                    model: self.context.model.clone(),
                                    messages: self.messages.clone(),
                                    tools: convert_tool_definitions(&self.tools),
                                    temperature: Some(0.7),
                                    max_tokens: Some(4096),
                                };

                                // Retry streaming
                                let (retry_tx, mut retry_rx) = mpsc::channel::<StreamEvent>(256);
                                let retry_provider = self.provider.clone();
                                let retry_handle = tokio::spawn(async move {
                                    let result = tokio::time::timeout(
                                        std::time::Duration::from_secs(LLM_TIMEOUT_SECS),
                                        retry_provider.chat_stream(retry_request, retry_tx),
                                    ).await;
                                    match result {
                                        Ok(inner) => inner,
                                        Err(_) => Err(anyhow::anyhow!("LLM 调用超时 ({}秒)", LLM_TIMEOUT_SECS)),
                                    }
                                });

                                // Consume retry stream
                                let mut retry_cancelled = false;
                                loop {
                                    if let Some(ref token) = cancel_token {
                                        tokio::select! {
                                            event = retry_rx.recv() => {
                                                match event {
                                                    Some(ev) => {
                                                        let is_done = matches!(ev, StreamEvent::Done(_));
                                                        Self::process_stream_event(ev, &mut full_text, &mut full_reasoning, &mut tool_calls, &mut final_response, &event_tx);
                                                        if is_done { break; }
                                                    }
                                                    None => break,
                                                }
                                            }
                                            _ = token.cancelled() => {
                                                retry_cancelled = true;
                                                retry_handle.abort();
                                                break;
                                            }
                                        }
                                    } else {
                                        match retry_rx.recv().await {
                                            Some(ev) => {
                                                let is_done = matches!(ev, StreamEvent::Done(_));
                                                Self::process_stream_event(ev, &mut full_text, &mut full_reasoning, &mut tool_calls, &mut final_response, &event_tx);
                                                if is_done { break; }
                                            }
                                            None => break,
                                        }
                                    }
                                }

                                if retry_cancelled {
                                    if !full_text.is_empty() || !full_reasoning.is_empty() {
                                        let mut blocks = Vec::new();
                                        if !full_reasoning.is_empty() {
                                            blocks.push(yode_llm::types::ContentBlock::Thinking { thinking: full_reasoning.clone(), signature: None });
                                        }
                                        if !full_text.is_empty() {
                                            blocks.push(yode_llm::types::ContentBlock::Text { text: full_text.clone() });
                                        }

                                        let assistant_msg = Message {
                                            role: Role::Assistant,
                                            content: if full_text.is_empty() { None } else { Some(full_text.clone()) },
                                            reasoning: if full_reasoning.is_empty() { None } else { Some(full_reasoning.clone()) },
                                            content_blocks: blocks,
                                            tool_calls: vec![],
                                            tool_call_id: None,
                                            images: Vec::new(),
                                        };
                                        self.messages.push(assistant_msg);
                                        self.persist_message(
                                            "assistant",
                                            if full_text.is_empty() { None } else { Some(&full_text) },
                                            if full_reasoning.is_empty() { None } else { Some(&full_reasoning) },
                                            None,
                                            None
                                        );
                                    }
                                    let _ = event_tx.send(EngineEvent::Done);
                                    return Ok(());
                                }

                                match retry_handle.await {
                                    Ok(Ok(())) => {
                                        retry_succeeded = true;
                                        break;
                                    }
                                    Ok(Err(e2)) => {
                                        warn!("Stream retry {}/{} failed: {}", attempt + 1, max_attempts, e2);
                                    }
                                    Err(e2) => {
                                        warn!("Stream retry {}/{} panicked: {}", attempt + 1, max_attempts, e2);
                                    }
                                }

                                // If we got content during retry, count as success
                                if !full_text.is_empty() || !tool_calls.is_empty() {
                                    retry_succeeded = true;
                                    break;
                                }
                            }

                            if !retry_succeeded {
                                let _ = event_tx.send(EngineEvent::Error(format!("{}", err)));
                                let _ = event_tx.send(EngineEvent::Done);
                                return Err(err).context("LLM chat request failed");
                            }
                        } else {
                            // Fatal error — no retry
                            let _ = event_tx.send(EngineEvent::Error(format!("{}", err)));
                            let _ = event_tx.send(EngineEvent::Done);
                            return Err(err).context("LLM chat request failed");
                        }
                    }
                    // else: stream failed but we have partial content, keep it
                }
            }

            // Check if context window needs compression based on final response usage
            if let Some(ref resp) = final_response {
                if self.context_manager.should_compress(resp.usage.prompt_tokens, &self.messages) {
                    let removed = self.context_manager.compress(&mut self.messages);
                    if removed > 0 {
                        let _ = event_tx.send(EngineEvent::ContextCompressed { removed });
                    }
                }
            }

            // Build assistant message from stream
            let mut assistant_msg = Message {
                role: Role::Assistant,
                content: if full_text.is_empty() { None } else { Some(full_text.clone()) },
                reasoning: if full_reasoning.is_empty() { None } else { Some(full_reasoning.clone()) },
                content_blocks: Vec::new(), 
                tool_calls: tool_calls.clone(),
                tool_call_id: None,
                images: Vec::new(),
            };

            // Handle StopReason::MaxTokens
            if let Some(ref resp) = final_response {
                if resp.stop_reason == Some(yode_llm::types::StopReason::MaxTokens) {
                    let warning = "\n\n[WARNING: Response truncated due to max_tokens limit. Consider increasing effort level if more detail is needed.]";
                    if let Some(content) = &mut assistant_msg.content {
                        content.push_str(warning);
                        full_text = content.clone();
                    } else {
                        assistant_msg.content = Some(warning.to_string());
                        full_text = warning.to_string();
                    }
                    warn!("LLM streaming response truncated due to max_tokens");
                } else if resp.stop_reason == Some(yode_llm::types::StopReason::StopSequence) || matches!(resp.stop_reason, Some(yode_llm::types::StopReason::Other(_))) {
                    if full_text.contains("[tool_") || full_text.contains("<tool_") {
                        warn!("LLM streaming response stopped via stop sequence or other reason but contains incomplete tool tags. Reason: {:?}", resp.stop_reason);
                    }
                }
            }

            // --- PR-1: Strict Protocol Gate & Auto Recovery/Retry ---
            if assistant_msg.tool_calls.is_empty() && !full_text.is_empty() && self.is_protocol_violation(&full_text) {
                // Try recovery first (Repair instead of Fail)
                let recovered = self.recover_leaked_tool_calls(&full_text);
                if !recovered.is_empty() {
                    info!("Recovered {} leaked tool calls from text response. Proceeding with execution.", recovered.len());
                    assistant_msg.tool_calls = recovered;
                    tool_calls = assistant_msg.tool_calls.clone(); // Sync local variable
                    self.violation_retries = 0; // Success, reset violation counter
                } else if self.violation_retries < 2 {
                    self.violation_retries += 1;
                    warn!("Protocol violation detected (attempt {}). Retrying with strict constraints...", self.violation_retries);
                    let _ = event_tx.send(EngineEvent::Thinking);
                    
                    self.messages.push(Message::user(
                        "STRICT PROTOCOL VIOLATION: You outputted internal tool tags ([tool_use], [DUMMY_TOOL], etc.) in your text response. \
                         This is forbidden. Please respond again using ONLY natural language. Do NOT use tool tags or JSON in this response."
                    ));
                    continue;
                } else {
                    let err_msg = "Critical protocol failure: Model repeatedly outputted internal tool tags in text field. Aborting to prevent loop.";
                    error!("{}", err_msg);
                    let _ = event_tx.send(EngineEvent::Error(err_msg.to_string()));
                    let _ = event_tx.send(EngineEvent::Done);
                    return Ok(());
                }
            } else {
                // Successful response or recovered, reset violation counter
                self.violation_retries = 0;
            }

            // Clean leaked tags from content before storing in history
            if let Some(ref text) = assistant_msg.content {
                if self.is_protocol_violation(text) {
                    assistant_msg.content = Some(self.clean_assistant_response(text));
                    full_text = assistant_msg.content.as_ref().unwrap().clone();
                }
            }

            // Build content blocks for history
            if !full_reasoning.is_empty() {
                assistant_msg.content_blocks.push(yode_llm::types::ContentBlock::Thinking { thinking: full_reasoning.clone(), signature: None });
            }
            if !full_text.is_empty() {
                assistant_msg.content_blocks.push(yode_llm::types::ContentBlock::Text { text: full_text.clone() });
            }
            
            self.messages.push(assistant_msg.clone());

            // Persist assistant message
            let tc_json = if !tool_calls.is_empty() {
                serde_json::to_string(&tool_calls).ok()
            } else {
                None
            };
            self.persist_message(
                "assistant",
                assistant_msg.content.as_deref(),
                assistant_msg.reasoning.as_deref(),
                tc_json.as_deref(),
                None,
            );

            // Handle tool calls (parallel where possible)
            if !tool_calls.is_empty() {
                let (parallel, sequential) = self.partition_tool_calls(&tool_calls);

                // Execute parallel tools concurrently
                if !parallel.is_empty() {
                    // Check cancellation before parallel batch
                    if let Some(ref token) = cancel_token {
                        if token.is_cancelled() {
                            let _ = event_tx.send(EngineEvent::Done);
                            return Ok(());
                        }
                    }

                    info!("Executing {} tools in parallel (streaming)", parallel.len());
                    let parallel_results = self.execute_tools_parallel(&parallel, &event_tx).await;

                    for (tc, result) in parallel_results {
                        let mut result = result;
                        self.track_file_access(&tc.name, &result);
                        result = truncate_tool_result(result);
                        self.enforce_tool_budget(&mut result);

                        self.tool_call_count += 1;
                        if self.tool_call_count == TOOL_BUDGET_WARNING {
                            result.content.push_str("\n\n[Budget warning: 25 tool calls used. Stop exploring and produce your report.]");
                        } else if self.tool_call_count == TOOL_BUDGET_NOTICE {
                            result.content.push_str("\n\n[Budget notice: 15 tool calls used. Consider summarizing current findings before continuing.]");
                        }

                        self.messages.push(Message::tool_result(&tc.id, &result.content));
                        self.persist_message("tool", Some(&result.content), None, None, Some(&tc.id));

                        let _ = event_tx.send(EngineEvent::ToolResult {
                            id: tc.id.clone(),
                            name: tc.name.clone(),
                            result,
                        });
                    }
                }

                // Execute sequential tools one by one
                for tool_call in &sequential {
                    if let Some(ref token) = cancel_token {
                        if token.is_cancelled() {
                            let _ = event_tx.send(EngineEvent::Done);
                            return Ok(());
                        }
                    }

                    let result = self
                        .handle_tool_call(tool_call, &event_tx, &mut confirm_rx, cancel_token.as_ref())
                        .await?;

                    let mut result = truncate_tool_result(result);

                    self.inject_intelligence(&mut result, &tool_call.name, &tool_call.arguments);

                    self.messages
                        .push(Message::tool_result(&tool_call.id, &result.content));
                    self.persist_message("tool", Some(&result.content), None, None, Some(&tool_call.id));

                    let _ = event_tx.send(EngineEvent::ToolResult {
                        id: tool_call.id.clone(),
                        name: tool_call.name.clone(),
                        result,
                    });
                }
                continue;
            }

            // Done
            if let Some(mut resp) = final_response {
                // If model returned no tool calls and an excessively long narrative,
                // enforce concise completion to avoid apparent "hang" in UI.
                if resp.message.tool_calls.is_empty() {
                    if let Some(content) = resp.message.content.clone() {
                        if content.len() > 3000 {
                            let trimmed: String = content.chars().take(1800).collect();
                            resp.message.content = Some(format!(
                                "{}\n\n[Output truncated by runtime guard to keep response responsive. Ask to continue if you want more details.]",
                                trimmed
                            ));
                        }
                    }
                }
                // Ensure the final message in the response has all the text/reasoning we accumulated
                // (OpenAI Done event sometimes has empty content if it was all in deltas)
                if resp.message.content.is_none() && !full_text.is_empty() {
                    resp.message.content = Some(full_text.clone());
                }
                if resp.message.reasoning.is_none() && !full_reasoning.is_empty() {
                    resp.message.reasoning = Some(full_reasoning.clone());
                }

                // Clean tags from final message before history/persistence
                let content_for_analysis = resp.message.content.clone();
                if let Some(content) = content_for_analysis {
                    if content.contains("[tool_use") || content.contains("[DUMMY_TOOL") {
                        resp.message.content = Some(self.clean_assistant_response(&content));
                    }
                    
                    // Strict mode: do NOT recover leaked tool calls from plain text.
                    // Claude-style stability favors schema-valid metadata only.
                    // Text pseudo-calls frequently cause malformed histories and API 400s.
                    if resp.message.tool_calls.is_empty() && content.contains("[tool_use") {
                        warn!("Detected leaked tool-use text; skipping text-recovery to avoid invalid tool schema propagation");
                    }
                }

                self.messages.push(resp.message.clone());

                let tc_json = if !resp.message.tool_calls.is_empty() {
                    serde_json::to_string(&resp.message.tool_calls).ok()
                } else {
                    None
                };
                self.persist_message(
                    "assistant",
                    resp.message.content.as_deref(),
                    resp.message.reasoning.as_deref(),
                    tc_json.as_deref(),
                    None,
                );

                if resp.message.tool_calls.is_empty() {
                    debug!("Streaming turn complete with no tool calls; finishing turn.");
                    let _ = event_tx.send(EngineEvent::TurnComplete(resp));
                    let _ = event_tx.send(EngineEvent::Done);
                    break;
                } else {
                    // Tool calls present, emit TurnComplete but continue the loop
                    debug!(
                        "Streaming turn produced {} tool calls; continuing loop.",
                        resp.message.tool_calls.len()
                    );
                    let _ = event_tx.send(EngineEvent::TurnComplete(resp));
                }
            } else {
                // Should not happen if stream completed normally
                let _ = event_tx.send(EngineEvent::Done);
                break;
            }
        }

        Ok(())
    }

    /// Process a single stream event.
    fn process_stream_event(
        event: StreamEvent,
        full_text: &mut String,
        full_reasoning: &mut String,
        tool_calls: &mut Vec<ToolCall>,
        final_response: &mut Option<ChatResponse>,
        event_tx: &mpsc::UnboundedSender<EngineEvent>,
    ) {
        match event {
            StreamEvent::TextDelta(delta) => {
                full_text.push_str(&delta);
                let _ = event_tx.send(EngineEvent::TextDelta(delta));
            }
            StreamEvent::UsageUpdate(usage) => {
                let _ = event_tx.send(EngineEvent::UsageUpdate(usage));
            }
            StreamEvent::ReasoningDelta(delta) => {
                full_reasoning.push_str(&delta);
                let _ = event_tx.send(EngineEvent::ReasoningDelta(delta));
            }
            StreamEvent::ToolCallStart { id, name } => {
                tool_calls.push(ToolCall {
                    id: id.clone(),
                    name: name.clone(),
                    arguments: String::new(),
                });
                let _ = event_tx.send(EngineEvent::ToolCallStart {
                    id,
                    name,
                    arguments: String::new(),
                });
            }
            StreamEvent::ToolCallDelta { id, arguments } => {
                if let Some(tc) = tool_calls.iter_mut().find(|t| t.id == id) {
                    tc.arguments.push_str(&arguments);
                }
            }
            StreamEvent::ToolCallEnd { id: _ } => {}
            StreamEvent::Done(resp) => {
                if !full_reasoning.is_empty() {
                    let _ = event_tx.send(EngineEvent::ReasoningComplete(full_reasoning.clone()));
                }
                *final_response = Some(resp);
            }
            StreamEvent::Error(e) => {
                let _ = event_tx.send(EngineEvent::Error(e));
            }
        }
    }

    /// Call LLM with retry logic (non-streaming). Classifies errors and uses appropriate backoff.
    async fn call_llm_with_retry(&self, request: ChatRequest) -> Result<ChatResponse> {
        self.call_llm_with_retry_notify(request, None).await
    }

    /// Call LLM with retry logic, optionally notifying the UI about retries.
    async fn call_llm_with_retry_notify(
        &self,
        request: ChatRequest,
        event_tx: Option<&mpsc::UnboundedSender<EngineEvent>>,
    ) -> Result<ChatResponse> {
        let mut last_err = None;
        let mut attempt: u32 = 0;
        let mut max_attempts = MAX_RETRIES; // will be adjusted on first error

        loop {
            if attempt > 0 && attempt <= max_attempts {
                let kind = last_err.as_ref().map(classify_error).unwrap_or(ErrorKind::Transient);
                let delay = retry_delay(kind, attempt - 1);
                let total_secs = delay.as_secs();
                if let Some(tx) = event_tx {
                    // Countdown: update UI every second
                    for remaining in (0..=total_secs).rev() {
                        let _ = tx.send(EngineEvent::Retrying {
                            error_message: format!("{}", last_err.as_ref().unwrap()),
                            attempt,
                            max_attempts,
                            delay_secs: remaining,
                        });
                        if remaining > 0 {
                            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                        }
                    }
                } else {
                    tokio::time::sleep(delay).await;
                }
                info!("Retrying LLM call (attempt {}/{})", attempt, max_attempts);
            }

            if attempt > max_attempts {
                break;
            }

            let result = tokio::time::timeout(
                std::time::Duration::from_secs(LLM_TIMEOUT_SECS),
                self.provider.chat(request.clone()),
            ).await;

            match result {
                Ok(Ok(response)) => return Ok(response),
                Ok(Err(e)) => {
                    let kind = classify_error(&e);
                    if kind == ErrorKind::Fatal {
                        return Err(e).context("LLM chat request failed");
                    }
                    max_attempts = max_retries_for(kind);
                    warn!("LLM call failed (attempt {}/{}): {}", attempt + 1, max_attempts, e);
                    last_err = Some(e);
                }
                Err(_) => {
                    let err = anyhow::anyhow!("LLM 调用超时 ({}秒)", LLM_TIMEOUT_SECS);
                    max_attempts = max_retries_for(ErrorKind::Transient);
                    warn!("LLM call timed out (attempt {}/{})", attempt + 1, max_attempts);
                    last_err = Some(err);
                }
            }
            attempt += 1;
        }

        Err(last_err.unwrap_or_else(|| anyhow::anyhow!("LLM call failed after {} retries", max_attempts)))
            .context("LLM chat request failed")
    }

    fn inject_intelligence(&mut self, result: &mut ToolResult, tool_name: &str, tool_args: &str) {
        self.tool_call_count += 1;

        // --- PR-1: Recovery FSM Logic ---
        if result.is_error {
            self.consecutive_failures += 1;
            
            if let Some(err_type) = &result.error_type {
                let err_type_val = *err_type;
                {
                    let count = self.error_buckets.entry(err_type_val).or_insert(0);
                    *count += 1;
                }
                let bucket_count = *self.error_buckets.get(&err_type_val).unwrap_or(&0);

                let current_sig = format!("{}:{}", tool_name, tool_args);
                let is_exact_retry = self.last_failed_signature.as_ref() == Some(&current_sig);
                self.last_failed_signature = Some(current_sig);

                // Strategy Enforcement based on bucket count
                self.update_recovery_state();
                match (err_type_val, bucket_count) {
                    (ToolErrorType::NotFound, c) if c >= 2 => {
                        result.content.push_str("\n\n[CRITICAL STRATEGY CHANGE: You have failed to find paths multiple times. STOP assuming paths. You MUST run `ls` on the parent directory or use `glob` to re-anchor your workspace understanding before trying this path again.]");
                    }
                    (ToolErrorType::Validation, c) if c >= 2 || is_exact_retry => {
                        result.content.push_str("\n\n[CRITICAL STRATEGY CHANGE: Your tool parameters are repeatedly invalid. Read the tool definition carefully and check for typos in file names or JSON structure. Do NOT repeat the same parameters.]");
                    }
                    (ToolErrorType::Protocol, c) if c >= 2 => {
                        result.content.push_str("\n\n[CRITICAL STRATEGY CHANGE: You are repeatedly outputting internal tool tags in your text. This is a protocol violation. STOP using square brackets or tags like `[tool_use]` in your text response. Use ONLY natural language for text and the structured tool calling interface for tools.]");
                    }
                    (ToolErrorType::Timeout, c) if c >= 2 => {
                        result.content.push_str("\n\n[CRITICAL STRATEGY CHANGE: Operations are timing out. The scope is too large. Break your task into much smaller steps or use more specific search patterns.]");
                    }
                    _ => {
                        // For first failure, provide a gentle hint if not already present
                        if result.suggestion.is_none() && bucket_count == 1 {
                            let hint = match err_type_val {
                                ToolErrorType::NotFound => "Hint: Use `glob` or `ls` to verify the path exists.",
                                ToolErrorType::Validation => "Hint: Check the required parameters and types in the tool schema.",
                                _ => "Hint: Try a different approach or tool.",
                            };
                            result.content.push_str(&format!("\n\n{}", hint));
                        }
                    }
                }
            }
        } else {
            // Successful tool result, reset recovery state
            self.consecutive_failures = 0;
            self.error_buckets.clear();
            self.last_failed_signature = None;
            self.violation_retries = 0;

            // Auto-exit re-anchor mode after a successful lightweight discovery action.
            // This mirrors Claude-style recovery flow: re-anchor first, then resume normal execution.
            let is_discovery_tool = matches!(tool_name, "ls" | "glob" | "read_file" | "project_map");
            if self.recovery_state == RecoveryState::ReanchorRequired && is_discovery_tool {
                self.consecutive_failures = 0;
                self.error_buckets.clear();
                self.last_failed_signature = None;
                self.recovery_state = RecoveryState::Normal;
                result.content.push_str(
                    "\n\n[Recovery: Workspace re-anchored successfully. Normal tool execution is now resumed.]",
                );
            } else {
                self.consecutive_failures = 0;
                self.error_buckets.clear(); // Reset on success
                self.last_failed_signature = None;
                self.update_recovery_state();
            }
        }

        // === Existing State tracking ===

        // Extract file_path from tool arguments if present
        let file_path = serde_json::from_str::<serde_json::Value>(tool_args)
            .ok()
            .and_then(|v| v.get("file_path").and_then(|p| p.as_str()).map(String::from));

        if let Some(ref path) = file_path {
            match tool_name {
                "read_file" if !result.is_error => {
                    let line_count = result.content.lines().count();
                    // Duplicate read detection
                    if let Some(&prev_lines) = self.files_read.get(path.as_str()) {
                        result.content.push_str(&format!(
                            "\n\n[Note: You already read this file earlier ({} lines). \
                             If you need specific lines, use offset/limit instead of re-reading.]",
                            prev_lines
                        ));
                    }
                    self.files_read.insert(path.clone(), line_count);
                }
                "edit_file" | "write_file" | "multi_edit" if !result.is_error => {
                    self.files_modified.push(path.clone());
                }
                _ => {}
            }
        }

        // === Contextual intelligence based on what just happened ===

        // After editing: suggest cross-reference check
        if !result.is_error && (tool_name == "edit_file" || tool_name == "write_file") {
            if self.files_modified.len() == 1 {
                // First edit — remind about verification
                result.content.push_str(
                    "\n\n[Next: Run `bash` with build command to verify. \
                     If you changed a function signature, grep for callers to update them too.]"
                );
            } else if self.files_modified.len() > 3 {
                // Many edits — remind to build
                result.content.push_str(&format!(
                    "\n\n[You've modified {} files so far. Run a build to catch any issues before continuing.]",
                    self.files_modified.len()
                ));
            }
        }

        // After bash: analyze build errors
        if tool_name == "bash" && result.is_error {
            // Look for common Rust compile error patterns
            if result.content.contains("error[E") {
                // Extract first error location
                if let Some(line) = result.content.lines().find(|l| l.contains("error[E")) {
                    result.content.push_str(&format!(
                        "\n\n[Build error detected. Focus on the first error: `{}`\n\
                         Read the file at the indicated line to understand the issue before attempting a fix.]",
                        line.trim().chars().take(200).collect::<String>()
                    ));
                }
            }
        }

        // Consecutive failures — escalating strategy change
        if self.consecutive_failures == 2 {
            result.content.push_str(
                "\n\n[2 failures in a row. Your current approach isn't working. \
                 Step back: What assumption might be wrong? Try a different tool or strategy.]"
            );
        } else if self.consecutive_failures >= 3 {
            result.content.push_str(
                "\n\n[3+ consecutive failures. STOP searching and summarize what you know. \
                 Present your findings to the user and ask for guidance.]"
            );
        }

        // === Periodic intelligence ===

        // Goal reminder at 5 calls
        if self.tool_call_count == TOOL_GOAL_REMINDER {
            result.content.push_str(
                "\n\n[5 tool calls done. Quick check: Do you have enough information to act? \
                 If yes, stop gathering and start implementing.]"
            );
        }

        // Self-reflection every 10 calls
        if self.tool_call_count > 0 && self.tool_call_count % TOOL_REFLECT_INTERVAL == 0 {
            let state_summary = format!(
                "\n\n[Checkpoint: {} tool calls | {} files read | {} files modified. \
                 Summarize your understanding. What's your hypothesis? What's the most efficient next step?]",
                self.tool_call_count,
                self.files_read.len(),
                self.files_modified.len()
            );
            result.content.push_str(&state_summary);
        }

        // Budget warnings
        if self.tool_call_count == TOOL_BUDGET_WARNING {
            result.content.push_str(
                "\n\n[Budget: 25 calls used. Produce your answer/fix NOW.]"
            );
        } else if self.tool_call_count == TOOL_BUDGET_NOTICE {
            result.content.push_str(
                "\n\n[Budget: 15 calls. Start converging toward a solution.]"
            );
        }
    }

    /// Partition tool calls into (parallel, sequential) based on permission and read_only.
    fn partition_tool_calls(&self, tool_calls: &[ToolCall]) -> (Vec<ToolCall>, Vec<ToolCall>) {
        let mut parallel = Vec::new();
        let mut sequential = Vec::new();

        // Unified recovery-state gate (Claude-style): in degraded states we force
        // single-step execution to avoid amplifying invalid calls in parallel.
        if self.recovery_state != RecoveryState::Normal {
            return (parallel, tool_calls.to_vec());
        }

        for tc in tool_calls {
            let can_parallel = if let Some(tool) = self.tools.get(&tc.name) {
                let caps = tool.capabilities();
                caps.read_only && matches!(self.permissions.check(&tc.name), PermissionAction::Allow)
            } else {
                false
            };

            if can_parallel {
                parallel.push(tc.clone());
            } else {
                sequential.push(tc.clone());
            }
        }

        (parallel, sequential)
    }

    /// Execute a batch of read-only, auto-allowed tool calls in parallel.
    async fn execute_tools_parallel(
        &self,
        tool_calls: &[ToolCall],
        event_tx: &mpsc::UnboundedSender<EngineEvent>,
    ) -> Vec<(ToolCall, ToolResult)> {
        use futures::future::join_all;

        let mut futures = Vec::new();

        for tc in tool_calls {
            let tool = match self.tools.get(&tc.name) {
                Some(t) => t,
                None => continue,
            };

            let mut params: serde_json::Value = serde_json::from_str(&tc.arguments)
                .unwrap_or_else(|_| serde_json::Value::Object(serde_json::Map::new()));

            let schema = tool.parameters_schema();
            if let Err(msg) = validation::validate_and_coerce(&schema, &mut params) {
                let tc_clone = tc.clone();
                let result = ToolResult::error_typed(
                    format!("Parameter validation failed: {}", msg),
                    ToolErrorType::Validation,
                    true,
                    Some(format!("Fix the parameters and retry. Schema: {}", schema)),
                );
                futures.push(Box::pin(async move { (tc_clone, result) })
                    as Pin<Box<dyn std::future::Future<Output = (ToolCall, ToolResult)> + Send>>);
                continue;
            }

            let _ = event_tx.send(EngineEvent::ToolCallStart {
                id: tc.id.clone(),
                name: tc.name.clone(),
                arguments: tc.arguments.clone(),
            });

            info!("Executing tool in parallel: {} (auto-allowed, read-only)", tc.name);

            let (p_tx, mut p_rx) = mpsc::unbounded_channel::<yode_tools::tool::ToolProgress>();
            let event_tx_inner = event_tx.clone();
            let tc_id = tc.id.clone();
            let tc_name = tc.name.clone();
            tokio::spawn(async move {
                while let Some(progress) = p_rx.recv().await {
                    let _ = event_tx_inner.send(EngineEvent::ToolProgress {
                        id: tc_id.clone(),
                        name: tc_name.clone(),
                        progress,
                    });
                }
            });

            let ctx = self.build_tool_context(Some(p_tx)).await;
            let tool_name = tc.name.clone();
            let tc_clone = tc.clone();

            futures.push(Box::pin(async move {
                let start = std::time::Instant::now();
                let timeout = std::time::Duration::from_secs(PARALLEL_TOOL_TIMEOUT_SECS);
                let result = match tokio::time::timeout(timeout, tool.execute(params, &ctx)).await {
                    Ok(Ok(r)) => r,
                    Ok(Err(e)) => {
                        error!("Tool {} failed: {}", tool_name, e);
                        ToolResult::error(format!("Tool execution failed: {}", e))
                    }
                    Err(_) => {
                        warn!("Tool {} timed out after {}s", tool_name, PARALLEL_TOOL_TIMEOUT_SECS);
                        ToolResult::error_typed(
                            format!("Tool {} timed out after {} seconds", tool_name, PARALLEL_TOOL_TIMEOUT_SECS),
                            ToolErrorType::Timeout,
                            true,
                            Some("Try a smaller scope or more specific parameters.".to_string()),
                        )
                    }
                };
                debug!(tool = %tool_name, elapsed_ms = start.elapsed().as_millis() as u64, "Parallel tool completed");
                (tc_clone, result)
            }));
        }

        join_all(futures).await
    }

    /// Tracks file read/modified status from tool results.
    fn track_file_access(&mut self, tool_name: &str, result: &ToolResult) {
        if result.is_error {
            return;
        }

        // --- PR-2: Dynamic CWD Synchronization ---
        if let Some(ref metadata) = result.metadata {
            if let Some(new_cwd) = metadata.get("cwd").and_then(|v| v.as_str()) {
                let runtime = self.context.runtime.clone();
                let new_path = std::path::PathBuf::from(new_cwd);
                tokio::spawn(async move {
                    let mut rt = runtime.lock().await;
                    if rt.cwd != new_path {
                        debug!("Syncing session CWD to: {}", new_path.display());
                        rt.cwd = new_path.clone();
                        rt.last_success_cwd = new_path;
                    }
                });
            }

            if let Some(file_path) = metadata.get("file_path").and_then(|v| v.as_str()) {
                match tool_name {
                    "read_file" => {
                        let lines = metadata.get("total_lines").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                        self.files_read.insert(file_path.to_string(), lines);
                    }
                    "edit_file" | "write_file" | "multi_edit" | "notebook_edit" => {
                        self.files_modified.push(file_path.to_string());
                    }
                    _ => {}
                }
            }
        }
    }

    /// Cleans hallucinated protocol tags from LLM text response.
    fn clean_assistant_response(&self, text: &str) -> String {
        let re = Regex::new(r"(?s)\[DUMMY_TOOL_RESULT\]|\[tool_use\s+[^\]>]+[\]>](?:\s*[:]\s*)?\{.*?\}[\s\]>]*|\[tool_result\s+[^\]>]+[\]>](?:\s*[:]\s*)?\{.*?\}[\s\]>]*|\[tool_(?:use|result)\s+[^\]>]+[\]>]").unwrap();
        re.replace_all(text, "").to_string()
    }

    /// Detects if the assistant response contains forbidden internal protocol patterns.
    fn is_protocol_violation(&self, text: &str) -> bool {
        // Forbidden protocol markers that models sometimes hallucinate
        let forbidden_patterns = [
            "[tool_use", 
            "[DUMMY_TOOL", 
            "[tool_result",
            // Claude-like XML leakage (if not explicitly enabled for the model)
            "<tool_code>", 
            "<tool_input>", 
            "<tool_call>",
        ];

        for pattern in forbidden_patterns {
            if text.contains(pattern) {
                return true;
            }
        }
        false
    }

    /// Attempts to recover tool calls leaked into the text response.
    /// This happens with some providers like DashScope/Aliyun.
    fn recover_leaked_tool_calls(&self, text: &str) -> Vec<ToolCall> {
        let mut recovered = Vec::new();

        // Safety/perf guards: avoid expensive broad regex recovery on very large texts.
        // We only attempt recovery when explicit tool marker exists and within size budget.
        const RECOVERY_TEXT_MAX_CHARS: usize = 20_000;
        const RECOVERY_MAX_CALLS: usize = 8;
        if text.len() > RECOVERY_TEXT_MAX_CHARS || (!text.contains("[tool_use") && !text.contains("[DUMMY_TOOL")) {
            return recovered;
        }

        // Pattern 1: Look for [tool_use id=... name=...] { ... }
        let tag_re = Regex::new(r"(?s)\[tool_use\s+id=([^\s\]>]+)\s+name=([^\s\]>]+)[\]>]\s*(\{.*?\})").unwrap();
        for cap in tag_re.captures_iter(text).take(RECOVERY_MAX_CALLS) {
            recovered.push(ToolCall {
                id: cap[1].to_string(),
                name: cap[2].to_string(),
                arguments: cap[3].to_string(),
            });
        }

        // Pattern 2: Look for raw JSON blocks only in small texts and only if no explicit tags found.
        if recovered.is_empty() {
            let json_re = Regex::new(r#"(?s)\{\s*"(?:command|file_path|pattern|query)"\s*:.*?\}"#).unwrap();
            for (i, m) in json_re.find_iter(text).take(RECOVERY_MAX_CALLS).enumerate() {
                let json_str = m.as_str();
                let name = if json_str.contains("\"command\"") {
                    "bash"
                } else if json_str.contains("\"file_path\"") && json_str.contains("\"old_string\"") {
                    "edit_file"
                } else if json_str.contains("\"file_path\"") {
                    "read_file"
                } else if json_str.contains("\"pattern\"") {
                    "glob"
                } else {
                    "unknown"
                };

                if name != "unknown" {
                    recovered.push(ToolCall {
                        id: format!("recovered_{}", i),
                        name: name.to_string(),
                        arguments: json_str.to_string(),
                    });
                }
            }
        }

        recovered
    }

    /// Enforces per-turn aggregate tool result size limits.
    fn enforce_tool_budget(&mut self, result: &mut ToolResult) {
        let size = result.content.len();
        self.total_tool_results_bytes += size;

        if self.total_tool_results_bytes > MAX_TOTAL_TOOL_RESULTS_SIZE {
            let over_limit = self.total_tool_results_bytes - MAX_TOTAL_TOOL_RESULTS_SIZE;
            if size > over_limit {
                // This result pushed us over the limit. Truncate it heavily.
                let allowed = size.saturating_sub(over_limit);
                let preview_len = allowed.min(1000); // Give at least some preview
                let preview: String = result.content.chars().take(preview_len).collect();
                
                result.content = format!(
                    "{}\n\n[AGGREGATE BUDGET EXCEEDED: Remaining {} bytes of this result omitted. \
                     STOP requesting large outputs in this turn to avoid context overflow.]",
                    preview,
                    size - preview_len
                );
            } else {
                // We were already over the limit
                result.content = format!(
                    "[AGGREGATE BUDGET EXCEEDED: Full result ({} bytes) omitted to prevent context overflow. \
                     Summarize your current findings instead.]",
                    size
                );
            }
        }
    }

    /// Handle a single tool call...
    async fn handle_tool_call(
        &mut self,
        tool_call: &ToolCall,
        event_tx: &mpsc::UnboundedSender<EngineEvent>,
        confirm_rx: &mut mpsc::UnboundedReceiver<ConfirmResponse>,
        cancel_token: Option<&CancellationToken>,
    ) -> Result<ToolResult> {
        let tool = match self.tools.get(&tool_call.name) {
            Some(t) => t,
            None => {
                return Ok(ToolResult::error(format!(
                    "Unknown tool: {}",
                    tool_call.name
                )));
            }
        };

        // Parse arguments
        let mut params: serde_json::Value = serde_json::from_str(&tool_call.arguments)
            .unwrap_or_else(|_| serde_json::Value::Object(serde_json::Map::new()));

        // Validate and coerce parameters against the tool's schema
        let schema = tool.parameters_schema();
        if let Err(msg) = validation::validate_and_coerce(&schema, &mut params) {
            return Ok(ToolResult::error_typed(
                format!("Parameter validation failed: {}", msg),
                ToolErrorType::Validation,
                true,
                Some(format!("Fix the parameters and retry. Schema: {}", schema)),
            ));
        }

        // Claude-style unified recovery gate:
        // when re-anchor is required, only allow lightweight discovery tools.
        if self.recovery_state == RecoveryState::ReanchorRequired {
            let allow_reanchor_tool = matches!(
                tool_call.name.as_str(),
                "ls" | "glob" | "read_file" | "project_map"
            );
            if !allow_reanchor_tool {
                return Ok(ToolResult::error_typed(
                    format!(
                        "Recovery gate active: '{}' is temporarily blocked until workspace is re-anchored.",
                        tool_call.name
                    ),
                    ToolErrorType::Validation,
                    true,
                    Some(
                        "Run a lightweight discovery step first (ls/glob/read_file/project_map), then continue with execution tools."
                            .to_string(),
                    ),
                ));
            }
        }

        // --- PR-3: Deep Path Security Validation ---
        if let Some(file_path) = params.get("file_path").and_then(|v| v.as_str()) {
            let mut reason = None;
            if file_path.contains("..") {
                reason = Some("Path traversal (..) is strictly forbidden for security reasons.");
            } else if file_path.contains('$') || file_path.contains('%') {
                reason = Some("Unexpanded shell variables ($VAR, %VAR%) are not allowed in paths. Use absolute or relative literal paths.");
            } else if file_path.starts_with('~') {
                reason = Some("Tilde (~) is not expanded. Use the full absolute path or a path relative to the current working directory.");
            }

            if let Some(r) = reason {
                return Ok(ToolResult::error_typed(
                    format!("Security Block: '{}' is an invalid path. {}", file_path, r),
                    ToolErrorType::Validation,
                    true,
                    Some("Correct the path to a literal, normalized format and try again.".to_string()),
                ));
            }
        }

        // Check permissions (with content matching for bash commands)
        let command_content = if tool_call.name == "bash" {
            params.get("command").and_then(|v| v.as_str()).map(|s| s.to_string())
        } else {
            None
        };

        // Unified project/command gate (Claude-style preflight): block mismatched
        // language-specific commands until project root assumptions are corrected.
        if let Some(reason) = self.language_command_mismatch(&tool_call.name, &params) {
            return Ok(ToolResult::error_typed(
                format!("Command blocked by project gate: {}", reason),
                ToolErrorType::Validation,
                true,
                Some("Re-anchor with ls/glob/read on the target project root, then run matching build tooling.".to_string()),
            ));
        }

        // --- Tool Chain Validation ---
        if tool_call.name == "edit_file" || tool_call.name == "write_file" {
            if let Some(file_path) = params.get("file_path").and_then(|v| v.as_str()) {
                if !self.files_read.contains_key(file_path) {
                    return Ok(ToolResult::error_typed(
                        format!("You must read the file '{}' with read_file before editing or overwriting it.", file_path),
                        ToolErrorType::Validation,
                        true,
                        Some(format!("Call read_file(file_path=\"{}\") first.", file_path)),
                    ));
                }
            }
        }

        let action = self.permissions.check_with_content(&tool_call.name, command_content.as_deref());

        // For bash: additional security check via command classifier
        if tool_call.name == "bash" {
            if let Some(ref cmd) = command_content {
                // --- Deep Strategy Enforcement: Prefer dedicated tools over bash ---
                let cmd_lower = cmd.to_lowercase();
                // Check for forbidden commands even if they are part of a chain (e.g. cd .. && find)
                let forbidden_binaries = ["find", "grep", "rg", "ag", "ack"];
                let is_forbidden = forbidden_binaries.iter().any(|&bin| {
                    // Match word boundary: find, /usr/bin/find, etc. but not "find_me"
                    let pattern = format!(r"(\s|^|&&|;|\|){}(\s|$)", bin);
                    if let Ok(re) = Regex::new(&pattern) {
                        re.is_match(&cmd_lower)
                    } else {
                        false
                    }
                });

                let is_recursive_ls = cmd_lower.contains("ls ") && (cmd_lower.contains("-r") || cmd_lower.contains("-lar"));

                if is_forbidden || is_recursive_ls {
                    let (cmd_name, alternative) = if is_forbidden {
                        let matched = forbidden_binaries.iter().find(|&&b| cmd_lower.contains(b)).unwrap_or(&"search");
                        (*matched, match *matched {
                            "find" => "glob",
                            _ => "grep",
                        })
                    } else {
                        ("ls -R", "ls (without -R) or project_map")
                    };

                    return Ok(ToolResult::error_typed(
                        format!("Command blocked: Use the dedicated '{}' tool instead of running '{}' via bash.", alternative, cmd_name),
                        ToolErrorType::Validation,
                        true,
                        Some(format!("Running search/discovery via bash is inefficient. Use the '{}' tool for better results and TUI display.", alternative)),
                    ));
                }

                match CommandClassifier::classify(cmd) {
                    CommandRiskLevel::Destructive => {
                        return Ok(ToolResult::error_typed(
                            format!("Command blocked (destructive): {}", cmd),
                            ToolErrorType::PermissionDeny,
                            false,
                            Some("This command is classified as destructive and cannot be executed.".to_string()),
                        ));
                    }
                    _ => {} // Other risk levels handled by permission system
                }
            }
        }

        match action {
            PermissionAction::Allow => {
                info!("Executing tool: {} (auto-allowed)", tool_call.name);
                // Re-send ToolCallStart with full arguments so TUI can update display.
                // (The initial ToolCallStart from streaming had empty arguments.)
                let _ = event_tx.send(EngineEvent::ToolCallStart {
                    id: tool_call.id.clone(),
                    name: tool_call.name.clone(),
                    arguments: tool_call.arguments.clone(),
                });
            }
            PermissionAction::Confirm => {
                let _ = event_tx.send(EngineEvent::ToolConfirmRequired {
                    id: tool_call.id.clone(),
                    name: tool_call.name.clone(),
                    arguments: tool_call.arguments.clone(),
                });

                debug!("Waiting for user confirmation: tool={}", tool_call.name);
                let confirm_start = std::time::Instant::now();
                let confirm_timeout = std::time::Duration::from_secs(90);
                loop {
                    if confirm_start.elapsed() > confirm_timeout {
                        return Ok(ToolResult::error_typed(
                            format!("Confirmation timed out for tool '{}'", tool_call.name),
                            ToolErrorType::Timeout,
                            true,
                            Some("No confirmation was received within 90s. Re-run or switch to a read-only alternative.".to_string()),
                        ));
                    }

                    if let Some(token) = cancel_token {
                        if token.is_cancelled() {
                            return Ok(ToolResult::error_typed(
                                format!("Tool confirmation cancelled: {}", tool_call.name),
                                ToolErrorType::Timeout,
                                true,
                                Some("User cancelled while waiting for confirmation.".to_string()),
                            ));
                        }
                    }

                    match tokio::time::timeout(std::time::Duration::from_millis(500), confirm_rx.recv()).await {
                        Ok(Some(ConfirmResponse::Allow)) => {
                            info!("Tool {} confirmed by user", tool_call.name);
                            break;
                        }
                        Ok(Some(ConfirmResponse::Deny)) => {
                            info!("Tool {} denied by user", tool_call.name);
                            self.permissions.record_denial(&tool_call.name);
                            return Ok(ToolResult::error(
                                "Tool execution denied by user.".to_string(),
                            ));
                        }
                        Ok(None) => {
                            return Ok(ToolResult::error_typed(
                                "Confirmation channel closed.".to_string(),
                                ToolErrorType::Execution,
                                true,
                                Some("Please retry the action. If this repeats, check TUI confirmation event handling.".to_string()),
                            ));
                        }
                        Err(_) => {
                            // periodic wakeup for cancellation and timeout checks
                        }
                    }
                }
            }
            PermissionAction::Deny => {
                return Ok(ToolResult::error(format!(
                    "Tool {} is not permitted.",
                    tool_call.name
                )));
            }
        }

        // Dedup detection: warn if same tool+args was called recently
        let call_sig = (tool_call.name.clone(), tool_call.arguments.clone());
        let current_sig_text = format!("{}:{}", tool_call.name, tool_call.arguments);

        // Allow repetition for observer tools (ls, git_status, etc.)
        let is_observer_tool = [
            "ls",
            "glob",
            "grep",
            "git_status",
            "git_diff",
            "git_log",
            "project_map",
            "todo",
            "read_file",
        ]
        .contains(&tool_call.name.as_str());

        // Hard strategy gate: if the exact failed signature keeps repeating after
        // multiple failures, reject early and force a strategy switch.
        if !is_observer_tool
            && self.consecutive_failures >= 2
            && self.last_failed_signature.as_ref() == Some(&current_sig_text)
        {
            return Ok(ToolResult::error_typed(
                format!(
                    "Blocked repeated failing call: {} is being retried with identical arguments after multiple failures.",
                    tool_call.name
                ),
                ToolErrorType::Validation,
                true,
                Some("Do not retry the same call. Re-anchor first (ls/glob/read), then change tool arguments.".to_string()),
            ));
        }

        if self.recent_tool_calls.contains(&call_sig) && !is_observer_tool {
            return Ok(ToolResult::error_typed(
                format!(
                    "Duplicate tool call detected: {} was called with identical arguments recently. \
                     If you are stuck, try a different approach, search for more information, or summarize your progress.",
                    tool_call.name
                ),
                ToolErrorType::Validation,
                true,
                Some("Do NOT resend identical tool parameters. Re-anchor with a lightweight read/list action, then adjust arguments.".to_string()),
            ));
        }
        self.recent_tool_calls.push(call_sig);
        // Keep only last 10 calls to avoid unbounded growth
        if self.recent_tool_calls.len() > 10 {
            self.recent_tool_calls.remove(0);
        }

        // Execute the tool with timing
        self.cost_tracker.record_tool_call();
        debug!("Tool execute start: tool={} args={}", tool_call.name, tool_call.arguments);
        let start_time = std::time::Instant::now();

        // Pre-tool-use hook
        if let Some(ref hook_mgr) = self.hook_manager {
            let hook_ctx = HookContext {
                event: "pre_tool_use".into(),
                session_id: self.context.session_id.clone(),
                working_dir: self.context.working_dir_compat().display().to_string(),
                tool_name: Some(tool_call.name.clone()),

                tool_input: Some(serde_json::from_str(&tool_call.arguments).unwrap_or_default()),
                tool_output: None,
                error: None,
                user_prompt: None,
            };
            if let Some(blocked) = hook_mgr.check_blocked(HookEvent::PreToolUse, &hook_ctx).await {
                return Ok(ToolResult::error_typed(
                    format!("Blocked by hook: {}", blocked.reason.unwrap_or_default()),
                    ToolErrorType::PermissionDeny,
                    false,
                    None,
                ));
            }
        }

        let (p_tx, mut p_rx) = mpsc::unbounded_channel::<yode_tools::tool::ToolProgress>();
        let event_tx_inner = event_tx.clone();
        let tc_id = tool_call.id.clone();
        let tc_name = tool_call.name.clone();
        tokio::spawn(async move {
            while let Some(progress) = p_rx.recv().await {
                let _ = event_tx_inner.send(EngineEvent::ToolProgress {
                    id: tc_id.clone(),
                    name: tc_name.clone(),
                    progress,
                });
            }
        });

        let ctx = self.build_tool_context(Some(p_tx)).await;

        // Pre-tool-use hook
        if let Some(ref hook_mgr) = self.hook_manager {
            let hook_ctx = HookContext {
                event: "pre_tool_use".into(),
                session_id: self.context.session_id.clone(),
                working_dir: ctx.working_dir.as_ref().map(|p| p.display().to_string()).unwrap_or_default(),
                tool_name: Some(tool_call.name.clone()),
                tool_input: Some(serde_json::from_str(&tool_call.arguments).unwrap_or_default()),
                tool_output: None,
                error: None,
                user_prompt: None,
            };
            if let Some(blocked) = hook_mgr.check_blocked(HookEvent::PreToolUse, &hook_ctx).await {
                return Ok(ToolResult::error_typed(
                    format!("Blocked by hook: {}", blocked.reason.unwrap_or_default()),
                    ToolErrorType::PermissionDeny,
                    false,
                    None,
                ));
            }
        }

        let mut result = match tokio::time::timeout(
            std::time::Duration::from_secs(120),
            tool.execute(params, &ctx),
        )
        .await
        {
            Ok(Ok(result)) => result,
            Ok(Err(e)) => {
                error!("Tool {} failed: {}", tool_call.name, e);
                ToolResult::error(format!("Tool execution failed: {}", e))
            }
            Err(_) => ToolResult::error_typed(
                format!("Tool execution timed out after 120s: {}", tool_call.name),
                ToolErrorType::Timeout,
                true,
                Some("Narrow the command scope or run a lighter probe first.".to_string()),
            ),
        };
        let elapsed = start_time.elapsed();
        debug!(tool = %tool_call.name, elapsed_ms = elapsed.as_millis() as u64, "Tool execution completed");

        self.track_file_access(&tool_call.name, &result);
        self.enforce_tool_budget(&mut result);

        // Append recovery suggestion to content so LLM can see it
        if result.is_error {
            // Add contextual recovery hints based on error type
            let auto_hint = match result.error_type {
                Some(ToolErrorType::NotFound) => {
                    Some(format!(
                        "Try using `glob` to find the correct path, or `grep` to search for the symbol by name."
                    ))
                }
                Some(ToolErrorType::Validation) => {
                    Some(format!(
                        "Re-check parameter types and required fields. Schema: {}",
                        tool.parameters_schema()
                    ))
                }
                Some(ToolErrorType::Timeout) => {
                    Some("Reduce the scope of the operation (smaller file range, fewer results) and retry.".to_string())
                }
                Some(ToolErrorType::Permission) => {
                    Some("This operation requires user confirmation. The user denied it — try an alternative approach.".to_string())
                }
                _ => None,
            };

            // Prefer tool-provided suggestion, fall back to auto-generated hint
            if let Some(ref suggestion) = result.suggestion {
                result.content.push_str(&format!("\n\nSuggestion: {}", suggestion));
            } else if let Some(hint) = auto_hint {
                result.content.push_str(&format!("\n\nSuggestion: {}", hint));
            }
        }

        Ok(result)
    }

    /// Generate a prompt suggestion using LLM.
    /// This is a lightweight call that suggests what the user might type next.
    pub async fn generate_prompt_suggestion(&self, recent_messages: &[Message]) -> Option<String> {
        // Suggestion prompt based on Claude Code's implementation
        let suggestion_prompt = r#"[SUGGESTION MODE: Suggest what the user might naturally type next.]

FIRST: Look at the user's recent messages and original request.

Your job is to predict what THEY would type - not what you think they should do.

THE TEST: Would they think "I was just about to type that"?

EXAMPLES:
- User asked "fix the bug and run tests", bug is fixed -> "run the tests"
- After code written -> "try it out"
- Claude offers options -> suggest the one the user would likely pick
- Claude asks to continue -> "yes" or "go ahead"
- Task complete, obvious follow-up -> "commit this" or "push it"

Be specific: "run the tests" beats "continue".

NEVER SUGGEST:
- Evaluative ("looks good", "thanks")
- Questions ("what about...?")
- Claude-voice ("Let me...", "I'll...", "Here's...")
- New ideas they didn't ask about
- Multiple sentences

Stay silent if the next step isn't obvious from what the user said.

Format: 2-12 words, match the user's style. Or nothing.

Reply with ONLY the suggestion, no quotes or explanation."#;

        // Build messages for suggestion generation
        let mut messages = vec![Message::system(suggestion_prompt)];

        // Add recent conversation context (last 6 messages for context)
        let context_start = recent_messages.len().saturating_sub(6);
        for msg in &recent_messages[context_start..] {
            if let Some(ref content) = msg.content {
                if !content.trim().is_empty() {
                    messages.push(msg.clone());
                }
            }
        }

        // Create a lightweight request with no tools
        let request = ChatRequest {
            model: self.context.model.clone(),
            messages,
            tools: vec![],
            temperature: Some(0.7),
            max_tokens: Some(50),
        };

        // Make the LLM call with timeout
        let provider = Arc::clone(&self.provider);

        // Reduced timeout to 5 seconds for suggestion (should be fast)
        match tokio::time::timeout(
            std::time::Duration::from_secs(5),
            provider.chat(request)
        ).await {
            Ok(Ok(response)) => {
                if let Some(content) = response.message.content {
                    let suggestion = content.trim().to_string();
                    // Filter out empty or meta suggestions
                    if !suggestion.is_empty()
                        && suggestion.len() <= 100
                        && !suggestion.starts_with('[')
                        && !suggestion.contains("silence")
                    {
                        return Some(suggestion);
                    }
                }
            }
            Ok(Err(e)) => {
                debug!("Prompt suggestion generation failed: {}", e);
            }
            Err(_) => {
                // Timeout is expected for slow APIs - log at trace level
                tracing::trace!("Prompt suggestion generation timed out (expected for slow APIs)");
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use yode_llm::types::ToolCall;
    use yode_tools::registry::ToolRegistry;
    use yode_tools::tool::{Tool, ToolCapabilities, ToolContext, ToolResult};

    /// Minimal mock LLM provider (never actually called in these tests).
    struct MockProvider;

    #[async_trait::async_trait]
    impl yode_llm::provider::LlmProvider for MockProvider {
        fn name(&self) -> &str { "mock" }
        async fn chat(&self, _req: yode_llm::types::ChatRequest) -> anyhow::Result<yode_llm::types::ChatResponse> {
            unimplemented!("Mock provider should not be called in unit tests")
        }
        async fn chat_stream(&self, _req: yode_llm::types::ChatRequest, _tx: tokio::sync::mpsc::Sender<yode_llm::types::StreamEvent>) -> anyhow::Result<()> {
            unimplemented!()
        }
        async fn list_models(&self) -> anyhow::Result<Vec<yode_llm::ModelInfo>> {
            Ok(vec![])
        }
    }

    /// A mock read-only tool for testing parallel execution.
    struct MockReadTool { name: String }

    #[async_trait::async_trait]
    impl Tool for MockReadTool {
        fn name(&self) -> &str { &self.name }
        fn description(&self) -> &str { "mock read tool" }
        fn parameters_schema(&self) -> serde_json::Value {
            serde_json::json!({"type": "object", "properties": {}})
        }
        fn capabilities(&self) -> ToolCapabilities {
            ToolCapabilities {
                requires_confirmation: false,
                supports_auto_execution: true,
                read_only: true,
            }
        }
        async fn execute(&self, _params: serde_json::Value, _ctx: &ToolContext) -> anyhow::Result<ToolResult> {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            Ok(ToolResult::success(format!("result from {}", self.name)))
        }
    }

    /// A mock write tool that requires confirmation.
    struct MockWriteTool;

    #[async_trait::async_trait]
    impl Tool for MockWriteTool {
        fn name(&self) -> &str { "mock_write" }
        fn description(&self) -> &str { "mock write tool" }
        fn parameters_schema(&self) -> serde_json::Value {
            serde_json::json!({"type": "object", "properties": {}})
        }
        fn capabilities(&self) -> ToolCapabilities {
            ToolCapabilities {
                requires_confirmation: true,
                supports_auto_execution: false,
                read_only: false,
            }
        }
        async fn execute(&self, _params: serde_json::Value, _ctx: &ToolContext) -> anyhow::Result<ToolResult> {
            Ok(ToolResult::success("write done".to_string()))
        }
    }

    fn make_engine(tools: Vec<Arc<dyn Tool>>, confirm_tools: Vec<String>) -> AgentEngine {
        let mut registry = ToolRegistry::new();
        for t in tools {
            registry.register(t);
        }
        let provider: Arc<dyn yode_llm::provider::LlmProvider> = Arc::new(MockProvider);
        let permissions = PermissionManager::from_confirmation_list(confirm_tools);
        let context = AgentContext::new(
            std::env::current_dir().unwrap(),
            "mock".to_string(),
            "claude-sonnet-4".to_string(),
        );
        AgentEngine::new(provider, Arc::new(registry), permissions, context)
    }

    // --- partition_tool_calls tests ---

    #[test]
    fn test_partition_all_read_only() {
        let engine = make_engine(
            vec![
                Arc::new(MockReadTool { name: "r1".into() }),
                Arc::new(MockReadTool { name: "r2".into() }),
                Arc::new(MockReadTool { name: "r3".into() }),
            ],
            vec![],
        );
        let tcs = vec![
            ToolCall { id: "1".into(), name: "r1".into(), arguments: "{}".into() },
            ToolCall { id: "2".into(), name: "r2".into(), arguments: "{}".into() },
            ToolCall { id: "3".into(), name: "r3".into(), arguments: "{}".into() },
        ];
        let (par, seq) = engine.partition_tool_calls(&tcs);
        assert_eq!(par.len(), 3);
        assert_eq!(seq.len(), 0);
    }

    #[test]
    fn test_partition_mixed() {
        let engine = make_engine(
            vec![
                Arc::new(MockReadTool { name: "reader".into() }),
                Arc::new(MockWriteTool),
            ],
            vec!["mock_write".into()],
        );
        let tcs = vec![
            ToolCall { id: "1".into(), name: "reader".into(), arguments: "{}".into() },
            ToolCall { id: "2".into(), name: "mock_write".into(), arguments: "{}".into() },
            ToolCall { id: "3".into(), name: "reader".into(), arguments: "{}".into() },
        ];
        let (par, seq) = engine.partition_tool_calls(&tcs);
        assert_eq!(par.len(), 2);
        assert_eq!(seq.len(), 1);
        assert_eq!(seq[0].name, "mock_write");
    }

    #[test]
    fn test_partition_unknown_tool() {
        let engine = make_engine(vec![], vec![]);
        let tcs = vec![
            ToolCall { id: "1".into(), name: "nonexistent".into(), arguments: "{}".into() },
        ];
        let (par, seq) = engine.partition_tool_calls(&tcs);
        assert_eq!(par.len(), 0);
        assert_eq!(seq.len(), 1);
    }

    #[test]
    fn test_partition_read_only_needing_confirm() {
        let engine = make_engine(
            vec![Arc::new(MockReadTool { name: "sensitive".into() })],
            vec!["sensitive".into()],
        );
        let tcs = vec![
            ToolCall { id: "1".into(), name: "sensitive".into(), arguments: "{}".into() },
        ];
        let (par, seq) = engine.partition_tool_calls(&tcs);
        assert_eq!(par.len(), 0, "Confirm-required tools must not be parallelized");
        assert_eq!(seq.len(), 1);
    }

    // --- execute_tools_parallel tests ---

    #[tokio::test]
    async fn test_parallel_returns_all_results_in_order() {
        let engine = make_engine(
            vec![
                Arc::new(MockReadTool { name: "a".into() }),
                Arc::new(MockReadTool { name: "b".into() }),
                Arc::new(MockReadTool { name: "c".into() }),
            ],
            vec![],
        );
        let tcs = vec![
            ToolCall { id: "x1".into(), name: "a".into(), arguments: "{}".into() },
            ToolCall { id: "x2".into(), name: "b".into(), arguments: "{}".into() },
            ToolCall { id: "x3".into(), name: "c".into(), arguments: "{}".into() },
        ];
        let (tx, mut rx) = mpsc::unbounded_channel();
        let results = engine.execute_tools_parallel(&tcs, &tx).await;

        assert_eq!(results.len(), 3);
        assert_eq!(results[0].0.id, "x1");
        assert_eq!(results[1].0.id, "x2");
        assert_eq!(results[2].0.id, "x3");
        for (_tc, r) in &results {
            assert!(!r.is_error);
        }

        // Check events
        let mut starts = 0;
        while let Ok(ev) = rx.try_recv() {
            if matches!(ev, EngineEvent::ToolCallStart { .. }) { starts += 1; }
        }
        assert_eq!(starts, 3);
    }

    #[tokio::test]
    async fn test_parallel_empty() {
        let engine = make_engine(vec![], vec![]);
        let (tx, _rx) = mpsc::unbounded_channel();
        let results = engine.execute_tools_parallel(&[], &tx).await;
        assert!(results.is_empty());
    }
}

/// Implementation of SubAgentRunner that creates a fresh AgentEngine for each sub-agent.
pub struct SubAgentRunnerImpl {
    pub provider: Arc<dyn LlmProvider>,
    pub tools: Arc<ToolRegistry>,
    pub context: AgentContext,
}

impl SubAgentRunner for SubAgentRunnerImpl {
    fn run_sub_agent(
        &self,
        prompt: String,
        options: SubAgentOptions,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<String>> + Send + '_>> {
        let allowed_tools = options.allowed_tools.clone();
        let subagent_model = options.model.clone();
        
        Box::pin(async move {
            // Create a filtered tool registry for the sub-agent
            let mut sub_registry = ToolRegistry::new();
            if allowed_tools.is_empty() {
                // Give all tools
                for tool in self.tools.list() {
                    sub_registry.register(tool);
                }
            } else {
                for name in &allowed_tools {
                    if let Some(tool) = self.tools.get(name) {
                        sub_registry.register(tool);
                    }
                }
            }

            let sub_registry = Arc::new(sub_registry);

            // Create a permissive permission manager for sub-agents
            let permissions = PermissionManager::permissive(); // auto-allow all

            // Model override for sub-agent
            let mut sub_context = self.context.clone();
            if let Some(m) = subagent_model {
                sub_context.model = m;
            }

            // Create sub-agent engine
            let mut engine = AgentEngine::new(
                Arc::clone(&self.provider),
                sub_registry,
                permissions,
                sub_context,
            );

            // Run non-streaming turn
            let (_event_tx, mut _event_rx) = mpsc::unbounded_channel::<EngineEvent>();
            let (_confirm_tx, confirm_rx) = mpsc::unbounded_channel();

            // Handle description in system prompt or similar if needed
            let turn_prompt = format!("[Sub-task: {}]\n\n{}", options.description, prompt);

            let (result_tx, mut result_rx) = mpsc::unbounded_channel();
            
            // Note: We currently don't have a clean way to get the final text from run_turn
            // easily without streaming, but we can capture the TextComplete event.
            let engine_handle = tokio::spawn(async move {
                engine.run_turn(&turn_prompt, result_tx, confirm_rx).await
            });

            let mut result_text = String::new();
            while let Some(event) = result_rx.recv().await {
                if let EngineEvent::TextComplete(text) = event {
                    result_text = text;
                }
            }

            engine_handle.await??;

            if result_text.is_empty() {
                result_text = "Sub-agent completed without text output.".to_string();
            }

            Ok(result_text)
        })
    }
}
