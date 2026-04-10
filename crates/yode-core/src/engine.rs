mod compaction_runtime;
mod hooks_runtime;
mod llm_runtime;
mod retry;
mod session_state;
mod subagent_runner;
mod tool_result;
mod types;

use regex::Regex;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::pin::Pin;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc,
};

use anyhow::{Context as _, Result};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use tokio::sync::{mpsc, Mutex};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

use yode_llm::provider::LlmProvider;
use yode_llm::types::{
    ChatRequest, ChatResponse, Message, Role, StreamEvent, ToolCall,
};
use yode_tools::registry::ToolRegistry;
use yode_tools::runtime_tasks::{
    RuntimeTask, RuntimeTaskNotification, RuntimeTaskStore,
};
use yode_tools::state::TaskStore;
use yode_tools::tool::{
    ToolContext, ToolErrorType, ToolResult, UserQuery,
};
use yode_tools::validation;

use crate::context::{AgentContext, EffortLevel, QuerySource};
use crate::context_manager::{CompressionReport, ContextManager};
use crate::cost_tracker::CostTracker;
use crate::db::{Database, SessionArtifacts};
use crate::hooks::{HookContext, HookEvent, HookManager};
use crate::instructions::{load_instruction_context, load_memory_context};
use crate::permission::{CommandClassifier, CommandRiskLevel, PermissionAction, PermissionManager};
use crate::session_memory::{
    build_live_snapshot, clear_live_session_memory, live_session_memory_path,
    persist_compaction_memory, persist_live_session_memory, persist_live_session_memory_summary,
    render_live_session_memory_prompt,
};
use crate::tool_runtime::{
    write_tool_turn_artifact, ToolResultTruncationView, ToolTurnArtifact,
};
use crate::transcript::write_compaction_transcript;
pub use types::{
    ConfirmResponse, EngineEvent, EngineRuntimeState, PromptCacheRuntimeState,
    SystemPromptSegmentRuntimeState,
};
use retry::{classify_error, hex_short, max_retries_for, retry_delay, ErrorKind};
use subagent_runner::SubAgentRunnerImpl;
use tool_result::{
    annotate_tool_result_runtime_metadata, convert_tool_definitions,
    set_tool_runtime_truncation_metadata, truncate_tool_result,
};
use types::{
    latest_transcript_runtime_state, ProjectKind, RecoveryState, SharedMemoryStatus,
    SystemPromptBuild, ToolExecutionOutcome, ToolExecutionTrace,
};

/// Maximum total size for all tool results in a single turn (200KB)
const MAX_TOTAL_TOOL_RESULTS_SIZE: usize = 200 * 1024;

/// LLM call timeout in seconds
const LLM_TIMEOUT_SECS: u64 = 120;

/// Per-tool timeout for parallel execution (30 seconds)
const PARALLEL_TOOL_TIMEOUT_SECS: u64 = 30;
/// Tool call budget thresholds for analysis guidance.
const TOOL_BUDGET_NOTICE: u32 = 15;
const TOOL_BUDGET_WARNING: u32 = 25;
/// Self-reflection injection point.
const TOOL_REFLECT_INTERVAL: u32 = 10;
/// Goal reminder injection point.
const TOOL_GOAL_REMINDER: u32 = 5;
/// Stop retrying auto-compaction after repeated failures.
const MAX_CONSECUTIVE_COMPACTION_FAILURES: u32 = 3;
const SESSION_MEMORY_INIT_CHAR_THRESHOLD: usize = 8_000;
const SESSION_MEMORY_CHAR_DELTA_THRESHOLD: usize = 4_000;
const SESSION_MEMORY_TOOL_DELTA_THRESHOLD: u32 = 3;

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
    /// Shared runtime task store for background bash/sub-agent work.
    runtime_task_store: Arc<Mutex<RuntimeTaskStore>>,
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
    /// Total tool progress events seen in this session.
    tool_progress_event_count: u32,
    /// Tool progress events seen in the current turn.
    current_turn_tool_progress_events: u32,
    /// Most recent tool progress message.
    last_tool_progress_message: Option<String>,
    /// Tool name for the most recent progress update.
    last_tool_progress_tool: Option<String>,
    /// Timestamp of the most recent tool progress update.
    last_tool_progress_at: Option<String>,
    /// Parallel batches executed across the current session.
    parallel_tool_batch_count: u32,
    /// Parallel batches executed in the current turn.
    current_turn_parallel_batches: u32,
    /// Tool calls executed in parallel across the current session.
    parallel_tool_call_count: u32,
    /// Tool calls executed in parallel in the current turn.
    current_turn_parallel_calls: u32,
    /// Largest observed parallel batch size in the current session.
    max_parallel_batch_size: usize,
    /// Largest observed parallel batch size in the current turn.
    current_turn_max_parallel_batch_size: usize,
    /// Budget notices emitted across the current session.
    tool_budget_notice_count: u32,
    /// Budget warnings emitted across the current session.
    tool_budget_warning_count: u32,
    /// Whether the current turn already emitted the notice threshold.
    current_turn_budget_notice_emitted: bool,
    /// Whether the current turn already emitted the warning threshold.
    current_turn_budget_warning_emitted: bool,
    /// Most recent budget warning text.
    last_tool_budget_warning: Option<String>,
    /// Truncated tool results across the current session.
    tool_truncation_count: u32,
    /// Truncated tool results in the current turn.
    current_turn_truncated_results: u32,
    /// Most recent truncation reason.
    last_tool_truncation_reason: Option<String>,
    /// Aggregated tool error types across the current session.
    tool_error_type_counts: BTreeMap<String, u32>,
    /// Failure signatures seen across the current session.
    repeated_tool_failure_patterns: HashMap<String, u32>,
    /// Most recent repeated failure summary.
    latest_repeated_tool_failure: Option<String>,
    /// Incrementing index for tool turns.
    tool_turn_counter: u64,
    /// When the current tool turn started.
    current_tool_turn_started_at: Option<String>,
    /// When the most recent tool turn completed.
    last_tool_turn_completed_at: Option<String>,
    /// Full trace for the current turn.
    current_tool_execution_traces: Vec<ToolExecutionTrace>,
    /// Last completed turn trace snapshot.
    last_tool_turn_traces: Vec<ToolExecutionTrace>,
    /// Latest per-turn tool artifact path.
    last_tool_turn_artifact_path: Option<String>,
    /// Error buckets for state-machine recovery (Type -> Count).
    error_buckets: std::collections::HashMap<ToolErrorType, u32>,
    /// Last failed path/command to detect identical retry loops.
    last_failed_signature: Option<String>,
    /// Recovery state transition counters.
    recovery_single_step_count: u32,
    recovery_reanchor_count: u32,
    recovery_need_user_guidance_count: u32,
    recovery_breadcrumbs: Vec<String>,
    last_recovery_artifact_path: Option<String>,
    /// Most recent permission decision explanation surfaced to diagnostics.
    last_permission_tool: Option<String>,
    last_permission_action: Option<String>,
    last_permission_explanation: Option<String>,
    last_permission_artifact_path: Option<String>,
    /// Tool call ids whose latest result was an error in the current session.
    failed_tool_call_ids: HashSet<String>,
    /// Whether the engine is in plan mode.
    plan_mode: Arc<Mutex<bool>>,
    /// Detected project kind for current session root.
    project_kind: ProjectKind,
    /// Unified recovery state for tool-call orchestration.
    recovery_state: RecoveryState,
    /// Current query source, used for context-management policy decisions.
    current_query_source: QuerySource,
    /// Consecutive auto-compaction failures for circuit breaking.
    compaction_failures: u32,
    /// Successful compactions in the current session.
    total_compactions: u32,
    /// Successful automatic compactions in the current session.
    auto_compactions: u32,
    /// Successful manual compactions in the current session.
    manual_compactions: u32,
    /// Most recent reason that opened the auto-compaction circuit breaker.
    last_compaction_breaker_reason: Option<String>,
    /// Whether auto-compaction has been disabled for this session.
    autocompact_disabled: bool,
    /// Guard against nested compaction attempts.
    compaction_in_progress: bool,
    /// Cumulative tool calls across the current session.
    session_tool_calls_total: u32,
    /// Whether live session memory has crossed its initial activation threshold.
    session_memory_initialized: bool,
    /// Message char count at the last live session memory refresh.
    last_session_memory_char_count: usize,
    /// Total tool calls at the last live session memory refresh.
    last_session_memory_tool_count: u32,
    /// Whether an async live session memory update is already running.
    session_memory_update_in_progress: Arc<AtomicBool>,
    /// Generation counter used to invalidate stale async session memory writes.
    session_memory_generation: Arc<AtomicU64>,
    /// Shared runtime status for live session memory updates from background tasks.
    shared_memory_status: Arc<Mutex<SharedMemoryStatus>>,
    /// Most recent compaction mode.
    last_compaction_mode: Option<String>,
    /// Most recent compaction timestamp.
    last_compaction_at: Option<String>,
    /// Most recent compaction summary excerpt.
    last_compaction_summary_excerpt: Option<String>,
    /// Most recent compaction session memory artifact path.
    last_compaction_session_memory_path: Option<String>,
    /// Most recent compaction transcript artifact path.
    last_compaction_transcript_path: Option<String>,
    /// Prompt tokens that triggered the most recent compaction.
    last_compaction_prompt_tokens: Option<u32>,
    /// Running total for compaction-trigger prompt token telemetry.
    compaction_prompt_tokens_total: u64,
    /// Sample count for compaction-trigger prompt token telemetry.
    compaction_prompt_token_samples: u32,
    /// Histogram of compaction outcomes and skip reasons.
    compaction_cause_histogram: BTreeMap<String, u32>,
    /// Prompt cache telemetry accumulated across turns.
    prompt_cache_runtime: PromptCacheRuntimeState,
    /// Estimated token footprint for the current system prompt.
    system_prompt_estimated_tokens: usize,
    /// Segment breakdown for the current system prompt.
    system_prompt_segments: Vec<SystemPromptSegmentRuntimeState>,
}

/// Convert yode-tools ToolDefinition to yode-llm ToolDefinition.
impl AgentEngine {
    fn detect_project_kind(root: &std::path::Path) -> ProjectKind {
        let has_cargo = root.join("Cargo.toml").exists();
        let has_node = root.join("package.json").exists();
        let has_python =
            root.join("pyproject.toml").exists() || root.join("requirements.txt").exists();

        match (has_cargo, has_node, has_python) {
            (true, false, false) => ProjectKind::Rust,
            (false, true, false) => ProjectKind::Node,
            (false, false, true) => ProjectKind::Python,
            (false, false, false) => ProjectKind::Unknown,
            _ => ProjectKind::Mixed,
        }
    }

    fn update_recovery_state(&mut self) {
        let not_found = *self
            .error_buckets
            .get(&ToolErrorType::NotFound)
            .unwrap_or(&0);
        let validation = *self
            .error_buckets
            .get(&ToolErrorType::Validation)
            .unwrap_or(&0);
        let timeout = *self
            .error_buckets
            .get(&ToolErrorType::Timeout)
            .unwrap_or(&0);

        let next_state = if self.consecutive_failures >= 3 {
            RecoveryState::NeedUserGuidance
        } else if validation >= 2 || timeout >= 2 || self.consecutive_failures >= 2 {
            RecoveryState::SingleStepMode
        } else if not_found >= 2 {
            RecoveryState::ReanchorRequired
        } else {
            RecoveryState::Normal
        };

        if next_state != self.recovery_state {
            let breadcrumb = format!(
                "{}: {:?} -> {:?} (consecutive_failures={}, last_signature={})",
                Self::now_timestamp(),
                self.recovery_state,
                next_state,
                self.consecutive_failures,
                self.last_failed_signature.as_deref().unwrap_or("none")
            );
            self.recovery_breadcrumbs.push(breadcrumb);
            if self.recovery_breadcrumbs.len() > 8 {
                let extra = self.recovery_breadcrumbs.len() - 8;
                self.recovery_breadcrumbs.drain(0..extra);
            }
            match next_state {
                RecoveryState::SingleStepMode => {
                    self.recovery_single_step_count =
                        self.recovery_single_step_count.saturating_add(1);
                }
                RecoveryState::ReanchorRequired => {
                    self.recovery_reanchor_count = self.recovery_reanchor_count.saturating_add(1);
                }
                RecoveryState::NeedUserGuidance => {
                    self.recovery_need_user_guidance_count =
                        self.recovery_need_user_guidance_count.saturating_add(1);
                }
                RecoveryState::Normal => {}
            }
        };
        self.recovery_state = next_state;
        self.write_recovery_artifact();
    }

    fn write_recovery_artifact(&mut self) {
        let dir = self.context.working_dir_compat().join(".yode").join("recovery");
        if std::fs::create_dir_all(&dir).is_err() {
            return;
        }
        let path = dir.join("latest-recovery.md");
        let breadcrumbs = if self.recovery_breadcrumbs.is_empty() {
            "- none".to_string()
        } else {
            self.recovery_breadcrumbs
                .iter()
                .map(|line| format!("- {}", line))
                .collect::<Vec<_>>()
                .join("\n")
        };
        let body = format!(
            "# Recovery State\n\n- State: {:?}\n- Updated At: {}\n- Consecutive failures: {}\n- Single-step count: {}\n- Reanchor count: {}\n- Need-guidance count: {}\n- Last failed signature: {}\n- Last permission tool: {}\n- Last permission action: {}\n\n## Breadcrumbs\n\n{}\n",
            self.recovery_state,
            Self::now_timestamp(),
            self.consecutive_failures,
            self.recovery_single_step_count,
            self.recovery_reanchor_count,
            self.recovery_need_user_guidance_count,
            self.last_failed_signature.as_deref().unwrap_or("none"),
            self.last_permission_tool.as_deref().unwrap_or("none"),
            self.last_permission_action.as_deref().unwrap_or("none"),
            breadcrumbs
        );
        if std::fs::write(&path, body).is_ok() {
            self.last_recovery_artifact_path = Some(path.display().to_string());
        }
    }

    fn write_permission_artifact(
        &mut self,
        source: &str,
        tool_name: &str,
        decision: &str,
        reason: &str,
        effective_input: &serde_json::Value,
        effective_arguments: &str,
        original_input: &serde_json::Value,
        original_arguments: &str,
        input_changed_by_hook: bool,
    ) {
        let dir = self.context.working_dir_compat().join(".yode").join("hooks");
        if std::fs::create_dir_all(&dir).is_err() {
            return;
        }
        let path = dir.join("latest-permission.json");
        let payload = serde_json::json!({
            "updated_at": Self::now_timestamp(),
            "source": source,
            "tool": tool_name,
            "decision": decision,
            "reason": reason,
            "effective_input_snapshot": effective_input,
            "effective_arguments_snapshot": effective_arguments,
            "original_input_snapshot": original_input,
            "original_arguments_snapshot": original_arguments,
            "input_changed_by_hook": input_changed_by_hook,
        });
        if std::fs::write(
            &path,
            serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string()),
        )
        .is_ok()
        {
            self.last_permission_artifact_path = Some(path.display().to_string());
        }
    }

    fn language_command_mismatch(
        &self,
        tool_name: &str,
        params: &serde_json::Value,
    ) -> Option<String> {
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
        let starts_with_npm = cmd.starts_with("npm ")
            || cmd.starts_with("pnpm ")
            || cmd.starts_with("yarn ")
            || cmd.starts_with("bun ");

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
        let system_prompt_build = Self::build_system_prompt_for_context(&context);
        let system_prompt = system_prompt_build.prompt.clone();

        let messages = vec![Message::system(&system_prompt)];

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
            runtime_task_store: Arc::new(Mutex::new(RuntimeTaskStore::new())),
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
            tool_progress_event_count: 0,
            current_turn_tool_progress_events: 0,
            last_tool_progress_message: None,
            last_tool_progress_tool: None,
            last_tool_progress_at: None,
            parallel_tool_batch_count: 0,
            current_turn_parallel_batches: 0,
            parallel_tool_call_count: 0,
            current_turn_parallel_calls: 0,
            max_parallel_batch_size: 0,
            current_turn_max_parallel_batch_size: 0,
            tool_budget_notice_count: 0,
            tool_budget_warning_count: 0,
            current_turn_budget_notice_emitted: false,
            current_turn_budget_warning_emitted: false,
            last_tool_budget_warning: None,
            tool_truncation_count: 0,
            current_turn_truncated_results: 0,
            last_tool_truncation_reason: None,
            tool_error_type_counts: BTreeMap::new(),
            repeated_tool_failure_patterns: HashMap::new(),
            latest_repeated_tool_failure: None,
            tool_turn_counter: 0,
            current_tool_turn_started_at: None,
            last_tool_turn_completed_at: None,
            current_tool_execution_traces: Vec::new(),
            last_tool_turn_traces: Vec::new(),
            last_tool_turn_artifact_path: None,
            error_buckets: std::collections::HashMap::new(),
            last_failed_signature: None,
            recovery_single_step_count: 0,
            recovery_reanchor_count: 0,
            recovery_need_user_guidance_count: 0,
            recovery_breadcrumbs: Vec::new(),
            last_recovery_artifact_path: None,
            last_permission_tool: None,
            last_permission_action: None,
            last_permission_explanation: None,
            last_permission_artifact_path: None,
            failed_tool_call_ids: HashSet::new(),
            plan_mode: Arc::new(Mutex::new(false)),
            project_kind: detected_project_kind,
            recovery_state: RecoveryState::Normal,
            current_query_source: QuerySource::User,
            compaction_failures: 0,
            total_compactions: 0,
            auto_compactions: 0,
            manual_compactions: 0,
            last_compaction_breaker_reason: None,
            autocompact_disabled: false,
            compaction_in_progress: false,
            session_tool_calls_total: 0,
            session_memory_initialized: false,
            last_session_memory_char_count: 0,
            last_session_memory_tool_count: 0,
            session_memory_update_in_progress: Arc::new(AtomicBool::new(false)),
            session_memory_generation: Arc::new(AtomicU64::new(0)),
            shared_memory_status: Arc::new(Mutex::new(SharedMemoryStatus::default())),
            last_compaction_mode: None,
            last_compaction_at: None,
            last_compaction_summary_excerpt: None,
            last_compaction_session_memory_path: None,
            last_compaction_transcript_path: None,
            last_compaction_prompt_tokens: None,
            compaction_prompt_tokens_total: 0,
            compaction_prompt_token_samples: 0,
            compaction_cause_histogram: BTreeMap::new(),
            prompt_cache_runtime: PromptCacheRuntimeState::default(),
            system_prompt_estimated_tokens: system_prompt_build.estimated_tokens,
            system_prompt_segments: system_prompt_build.segments,
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
        self.reset_autocompact_state();
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
        let system_prompt_build = Self::build_system_prompt_for_context(&self.context);
        let system_prompt = system_prompt_build.prompt;

        self.system_prompt = system_prompt.clone();
        self.system_prompt_estimated_tokens = system_prompt_build.estimated_tokens;
        self.system_prompt_segments = system_prompt_build.segments;

        // Update the system message in conversation history
        if let Some(first) = self.messages.first_mut() {
            if matches!(first.role, Role::System) {
                first.content = Some(system_prompt);
                first.normalize_in_place();
            }
        }
    }

    fn build_system_prompt_for_context(context: &AgentContext) -> SystemPromptBuild {
        let mut segments = Vec::new();
        let cwd = context.working_dir_compat();
        let mut push_segment = |label: &str, content: String| {
            if !content.trim().is_empty() {
                segments.push((label.to_string(), content));
            }
        };

        push_segment("Base prompt", include_str!("../../../prompts/system.md").to_string());

        let mut environment = String::from("# Environment\n\n");
        environment.push_str(&format!(
            "- Working directory: {}\n- Project root: {}\n- Platform: {} ({})\n- Date: {}\n- Model: {}\n- Provider: {}\n",
            cwd.display(),
            cwd.display(),
            std::env::consts::OS,
            std::env::consts::ARCH,
            chrono::Local::now().format("%Y-%m-%d"),
            context.model,
            context.provider,
        ));

        if cwd.join(".git").exists() {
            environment.push_str("- Git repo: yes\n");
            if let Ok(output) = std::process::Command::new("git")
                .args(["branch", "--show-current"])
                .current_dir(&cwd)
                .output()
            {
                let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !branch.is_empty() {
                    environment.push_str(&format!("- Branch: {}\n", branch));
                }
            }
        }
        push_segment("Environment", environment);

        if let Some(instruction_content) = load_instruction_context(&cwd) {
            push_segment("Instruction memory", instruction_content);
        }

        if let Some(memory_content) = load_memory_context(&cwd) {
            push_segment("Persistent memory", memory_content);
        }

        if context.output_style != "default" {
            let mut output_style = String::from("# Output Style\n\n");
            match context.output_style.as_str() {
                "explanatory" => {
                    output_style.push_str("You are in **Explanatory Mode**. Before and after writing code, provide brief educational insights about implementation choices.\n");
                    output_style.push_str("Include 2-3 key educational points explaining WHY you chose this approach.\n");
                    output_style.push_str(
                        "These insights should be in the conversation, not in the codebase.\n",
                    );
                }
                "learning" => {
                    output_style.push_str("You are in **Learning Mode**. Help the user learn through hands-on practice.\n");
                    output_style
                        .push_str("- Request user input for meaningful design decisions\n");
                    output_style.push_str("- Ask the user to write small code pieces (2-10 lines) for key decisions\n");
                    output_style.push_str(
                        "- Frame contributions as valuable design decisions, not busy work\n",
                    );
                    output_style.push_str("- Wait for user implementation before proceeding\n");
                }
                _ => {}
            }
            push_segment("Output style", output_style);
        }

        let prompt = segments
            .iter()
            .map(|(_, content)| content.trim_end().to_string())
            .collect::<Vec<_>>()
            .join("\n\n");
        let estimator = ContextManager::new(&context.model);
        let runtime_segments = segments
            .into_iter()
            .map(|(label, content)| SystemPromptSegmentRuntimeState {
                chars: content.chars().count(),
                estimated_tokens: estimator.estimate_tokens_for_messages(&[Message::system(
                    content.clone(),
                )]),
                label,
            })
            .collect::<Vec<_>>();

        SystemPromptBuild {
            estimated_tokens: estimator
                .estimate_tokens_for_messages(&[Message::system(prompt.clone())]),
            prompt,
            segments: runtime_segments,
        }
    }

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
    pub fn cost_tracker(&self) -> &CostTracker {
        &self.cost_tracker
    }
    pub fn cost_tracker_mut(&mut self) -> &mut CostTracker {
        &mut self.cost_tracker
    }
    pub fn get_database(&self) -> Option<&Database> {
        self.db.as_ref()
    }

    pub fn runtime_state(&self) -> EngineRuntimeState {
        let shared_status = self
            .shared_memory_status
            .try_lock()
            .ok()
            .map(|state| {
                (
                    state.last_session_memory_update_at.clone(),
                    state.last_session_memory_update_path.clone(),
                    state.last_session_memory_generated_summary,
                    state.session_memory_update_count,
                )
            })
            .unwrap_or((None, None, false, 0));
        let hook_stats = self
            .hook_manager
            .as_ref()
            .map(|mgr| mgr.stats_snapshot())
            .unwrap_or_default();
        let recent_permission_denials = self
            .permissions
            .recent_denials(5)
            .into_iter()
            .map(|entry| {
                format!(
                    "{} x{} (consecutive {}, at {})",
                    entry.tool_name, entry.count, entry.consecutive, entry.last_at
                )
            })
            .collect::<Vec<_>>();
        let (tool_trace_scope, tool_traces) = if self.current_tool_execution_traces.is_empty() {
            (
                "last".to_string(),
                self.last_tool_turn_traces
                    .iter()
                    .map(ToolExecutionTrace::to_view)
                    .collect(),
            )
        } else {
            (
                "current".to_string(),
                self.current_tool_execution_traces
                    .iter()
                    .map(ToolExecutionTrace::to_view)
                    .collect(),
            )
        };
        EngineRuntimeState {
            query_source: format!("{:?}", self.current_query_source),
            autocompact_disabled: self.autocompact_disabled,
            compaction_failures: self.compaction_failures,
            total_compactions: self.total_compactions,
            auto_compactions: self.auto_compactions,
            manual_compactions: self.manual_compactions,
            last_compaction_breaker_reason: self.last_compaction_breaker_reason.clone(),
            context_window_tokens: self.context_manager.context_window(),
            compaction_threshold_tokens: self.context_manager.compression_threshold_tokens(),
            estimated_context_tokens: self
                .context_manager
                .estimate_tokens_for_messages(&self.messages),
            message_count: self.messages.len(),
            live_session_memory_initialized: self.session_memory_initialized,
            live_session_memory_updating: self
                .session_memory_update_in_progress
                .load(Ordering::SeqCst),
            live_session_memory_path: live_session_memory_path(&self.context.working_dir_compat())
                .display()
                .to_string(),
            session_tool_calls_total: self.session_tool_calls_total,
            last_compaction_mode: self.last_compaction_mode.clone(),
            last_compaction_at: self.last_compaction_at.clone(),
            last_compaction_summary_excerpt: self.last_compaction_summary_excerpt.clone(),
            last_compaction_session_memory_path: self.last_compaction_session_memory_path.clone(),
            last_compaction_transcript_path: self.last_compaction_transcript_path.clone(),
            last_session_memory_update_at: shared_status.0,
            last_session_memory_update_path: shared_status.1,
            last_session_memory_generated_summary: shared_status.2,
            session_memory_update_count: shared_status.3,
            tracked_failed_tool_results: self.failed_tool_call_ids.len(),
            hook_total_executions: hook_stats.total_executions,
            hook_timeout_count: hook_stats.timeout_count,
            hook_execution_error_count: hook_stats.execution_error_count,
            hook_nonzero_exit_count: hook_stats.nonzero_exit_count,
            hook_wake_notification_count: hook_stats.wake_notification_count,
            last_hook_failure_event: hook_stats.last_failure_event,
            last_hook_failure_command: hook_stats.last_failure_command,
            last_hook_failure_reason: hook_stats.last_failure_reason,
            last_hook_failure_at: hook_stats.last_failure_at,
            last_hook_timeout_command: hook_stats.last_timeout_command,
            last_compaction_prompt_tokens: self.last_compaction_prompt_tokens,
            avg_compaction_prompt_tokens: (self.compaction_prompt_token_samples > 0).then(|| {
                (self.compaction_prompt_tokens_total / self.compaction_prompt_token_samples as u64)
                    as u32
            }),
            compaction_cause_histogram: self.compaction_cause_histogram.clone(),
            system_prompt_estimated_tokens: self.system_prompt_estimated_tokens,
            system_prompt_segments: self.system_prompt_segments.clone(),
            prompt_cache: self.prompt_cache_runtime.clone(),
            recovery_state: format!("{:?}", self.recovery_state),
            recovery_single_step_count: self.recovery_single_step_count,
            recovery_reanchor_count: self.recovery_reanchor_count,
            recovery_need_user_guidance_count: self.recovery_need_user_guidance_count,
            last_failed_signature: self.last_failed_signature.clone(),
            recovery_breadcrumbs: self.recovery_breadcrumbs.clone(),
            last_recovery_artifact_path: self.last_recovery_artifact_path.clone(),
            last_permission_tool: self.last_permission_tool.clone(),
            last_permission_action: self.last_permission_action.clone(),
            last_permission_explanation: self.last_permission_explanation.clone(),
            last_permission_artifact_path: self.last_permission_artifact_path.clone(),
            recent_permission_denials,
            current_turn_tool_calls: self.tool_call_count,
            current_turn_tool_output_bytes: self.total_tool_results_bytes,
            current_turn_tool_progress_events: self.current_turn_tool_progress_events,
            current_turn_parallel_batches: self.current_turn_parallel_batches,
            current_turn_parallel_calls: self.current_turn_parallel_calls,
            current_turn_max_parallel_batch_size: self.current_turn_max_parallel_batch_size,
            current_turn_truncated_results: self.current_turn_truncated_results,
            current_turn_budget_notice_emitted: self.current_turn_budget_notice_emitted,
            current_turn_budget_warning_emitted: self.current_turn_budget_warning_emitted,
            tool_budget_notice_count: self.tool_budget_notice_count,
            tool_budget_warning_count: self.tool_budget_warning_count,
            last_tool_budget_warning: self.last_tool_budget_warning.clone(),
            tool_progress_event_count: self.tool_progress_event_count,
            last_tool_progress_message: self.last_tool_progress_message.clone(),
            last_tool_progress_tool: self.last_tool_progress_tool.clone(),
            last_tool_progress_at: self.last_tool_progress_at.clone(),
            parallel_tool_batch_count: self.parallel_tool_batch_count,
            parallel_tool_call_count: self.parallel_tool_call_count,
            max_parallel_batch_size: self.max_parallel_batch_size,
            tool_truncation_count: self.tool_truncation_count,
            last_tool_truncation_reason: self.last_tool_truncation_reason.clone(),
            latest_repeated_tool_failure: self.latest_repeated_tool_failure.clone(),
            read_file_history: self.read_file_history_preview(),
            command_tool_duplication_hints: self.command_tool_duplication_hints(),
            last_tool_turn_completed_at: self.last_tool_turn_completed_at.clone(),
            last_tool_turn_artifact_path: self.last_tool_turn_artifact_path.clone(),
            tool_error_type_counts: self.tool_error_type_counts.clone(),
            tool_trace_scope,
            tool_traces,
        }
    }

    fn read_file_history_preview(&self) -> Vec<String> {
        let mut entries = self
            .files_read
            .iter()
            .map(|(path, lines)| format!("{} ({} lines)", path, lines))
            .collect::<Vec<_>>();
        entries.sort();
        entries.into_iter().take(8).collect()
    }

    fn command_tool_duplication_hints(&self) -> Vec<String> {
        self.last_tool_turn_traces
            .iter()
            .chain(self.current_tool_execution_traces.iter())
            .filter(|trace| trace.tool_name == "bash")
            .filter_map(|trace| {
                let summary = trace.metadata_summary.as_deref()?;
                summary
                    .contains("rewrite_suggestion=")
                    .then(|| summary.to_string())
            })
            .take(6)
            .collect()
    }

    pub fn runtime_tasks_snapshot(&self) -> Vec<RuntimeTask> {
        self.runtime_task_store
            .try_lock()
            .ok()
            .map(|store| store.list())
            .unwrap_or_default()
    }

    pub fn runtime_task_snapshot(&self, id: &str) -> Option<RuntimeTask> {
        self.runtime_task_store
            .try_lock()
            .ok()
            .and_then(|store| store.get(id))
    }

    pub fn cancel_runtime_task(&self, id: &str) -> bool {
        self.runtime_task_store
            .try_lock()
            .ok()
            .map(|mut store| store.request_cancel(id))
            .unwrap_or(false)
    }

    pub fn drain_runtime_task_notifications(&self) -> Vec<RuntimeTaskNotification> {
        self.runtime_task_store
            .try_lock()
            .ok()
            .map(|mut store| store.drain_notifications())
            .unwrap_or_default()
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
            runtime_tasks: Some(Arc::clone(&self.runtime_task_store)),
            user_input_tx: self.ask_user_tx.clone(),
            user_input_rx: self.ask_user_rx.clone(),
            progress_tx,
            working_dir: Some(cwd),
            sub_agent_runner: Some(Arc::new(SubAgentRunnerImpl {
                provider: Arc::clone(&self.provider),
                tools: Arc::clone(&self.tools),
                context: self.context.clone(),
                runtime_tasks: Arc::clone(&self.runtime_task_store),
            })),
            mcp_resources: None,
            cron_manager: None,
            lsp_manager: None,
            worktree_state: None,
            read_file_history: Some(Arc::new(tokio::sync::Mutex::new(
                std::collections::HashSet::new(),
            ))),
            plan_mode: Some(Arc::clone(&self.plan_mode)),
        }
    }

    async fn current_runtime_working_dir(&self) -> String {
        self.context.runtime.lock().await.cwd.display().to_string()
    }

    fn parse_tool_input(arguments: &str) -> Value {
        serde_json::from_str(arguments).unwrap_or_else(|_| Value::Object(Map::new()))
    }

    fn now_timestamp() -> String {
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
    }

    fn reset_tool_turn_runtime(&mut self) {
        self.tool_turn_counter = self.tool_turn_counter.saturating_add(1);
        self.current_tool_turn_started_at = Some(Self::now_timestamp());
        self.current_turn_tool_progress_events = 0;
        self.current_turn_parallel_batches = 0;
        self.current_turn_parallel_calls = 0;
        self.current_turn_max_parallel_batch_size = 0;
        self.current_turn_budget_notice_emitted = false;
        self.current_turn_budget_warning_emitted = false;
        self.current_turn_truncated_results = 0;
        self.current_tool_execution_traces.clear();
        self.total_tool_results_bytes = 0;
        self.tool_call_count = 0;
    }

    fn reset_prompt_cache_turn_runtime(&mut self) {
        self.prompt_cache_runtime.last_turn_prompt_tokens = None;
        self.prompt_cache_runtime.last_turn_completion_tokens = None;
        self.prompt_cache_runtime.last_turn_cache_write_tokens = None;
        self.prompt_cache_runtime.last_turn_cache_read_tokens = None;
    }

    fn record_compaction_cause(&mut self, cause: &str) {
        *self
            .compaction_cause_histogram
            .entry(cause.to_string())
            .or_insert(0) += 1;
    }

    fn record_response_usage(
        &mut self,
        usage: &yode_llm::types::Usage,
        event_tx: &mpsc::UnboundedSender<EngineEvent>,
    ) {
        self.cost_tracker.record_usage(
            usage.uncached_prompt_tokens() as u64,
            usage.completion_tokens as u64,
        );
        if usage.cache_write_tokens > 0 || usage.cache_read_tokens > 0 {
            self.cost_tracker.record_cache_usage(
                usage.cache_write_tokens as u64,
                usage.cache_read_tokens as u64,
            );
        }

        if usage.has_reported_tokens() {
            self.prompt_cache_runtime.last_turn_prompt_tokens = Some(usage.prompt_tokens);
            self.prompt_cache_runtime.last_turn_completion_tokens =
                Some(usage.completion_tokens);
            self.prompt_cache_runtime.last_turn_cache_write_tokens =
                Some(usage.cache_write_tokens);
            self.prompt_cache_runtime.last_turn_cache_read_tokens =
                Some(usage.cache_read_tokens);
            self.prompt_cache_runtime.reported_turns =
                self.prompt_cache_runtime.reported_turns.saturating_add(1);
            if usage.cache_write_tokens > 0 {
                self.prompt_cache_runtime.cache_write_turns =
                    self.prompt_cache_runtime.cache_write_turns.saturating_add(1);
            }
            if usage.cache_read_tokens > 0 {
                self.prompt_cache_runtime.cache_read_turns =
                    self.prompt_cache_runtime.cache_read_turns.saturating_add(1);
            }
            self.prompt_cache_runtime.cache_write_tokens_total = self
                .prompt_cache_runtime
                .cache_write_tokens_total
                .saturating_add(usage.cache_write_tokens as u64);
            self.prompt_cache_runtime.cache_read_tokens_total = self
                .prompt_cache_runtime
                .cache_read_tokens_total
                .saturating_add(usage.cache_read_tokens as u64);
        }

        let _ = event_tx.send(EngineEvent::CostUpdate {
            estimated_cost: self.cost_tracker.estimated_cost(),
            input_tokens: self.cost_tracker.usage().input_tokens,
            output_tokens: self.cost_tracker.usage().output_tokens,
            cache_write_tokens: self.cost_tracker.usage().cache_write_tokens,
            cache_read_tokens: self.cost_tracker.usage().cache_read_tokens,
        });

        if self.cost_tracker.is_over_budget() {
            let _ = event_tx.send(EngineEvent::BudgetExceeded {
                cost: self.cost_tracker.estimated_cost(),
                limit: self.cost_tracker.remaining_budget().unwrap_or(0.0),
            });
        }
    }

    fn record_tool_progress_summary(
        &mut self,
        tool_name: &str,
        count: u32,
        last_message: Option<String>,
    ) {
        if count == 0 {
            return;
        }
        self.tool_progress_event_count = self.tool_progress_event_count.saturating_add(count);
        self.current_turn_tool_progress_events =
            self.current_turn_tool_progress_events.saturating_add(count);
        if let Some(message) = last_message {
            self.last_tool_progress_message = Some(message);
            self.last_tool_progress_tool = Some(tool_name.to_string());
            self.last_tool_progress_at = Some(Self::now_timestamp());
        }
    }

    fn register_parallel_batch(&mut self, batch_size: usize) -> u32 {
        self.parallel_tool_batch_count = self.parallel_tool_batch_count.saturating_add(1);
        self.current_turn_parallel_batches = self.current_turn_parallel_batches.saturating_add(1);
        self.parallel_tool_call_count = self
            .parallel_tool_call_count
            .saturating_add(batch_size as u32);
        self.current_turn_parallel_calls = self
            .current_turn_parallel_calls
            .saturating_add(batch_size as u32);
        self.max_parallel_batch_size = self.max_parallel_batch_size.max(batch_size);
        self.current_turn_max_parallel_batch_size =
            self.current_turn_max_parallel_batch_size.max(batch_size);
        self.parallel_tool_batch_count
    }

    fn maybe_record_tool_budget_warning(&mut self) -> Option<String> {
        if self.tool_call_count >= TOOL_BUDGET_WARNING && !self.current_turn_budget_warning_emitted
        {
            let message =
                "Budget warning: 25 tool calls used. Stop exploring and produce your report.";
            self.current_turn_budget_warning_emitted = true;
            self.tool_budget_warning_count = self.tool_budget_warning_count.saturating_add(1);
            self.last_tool_budget_warning = Some(message.to_string());
            return Some(message.to_string());
        }

        if self.tool_call_count >= TOOL_BUDGET_NOTICE && !self.current_turn_budget_notice_emitted {
            let message =
                "Budget notice: 15 tool calls used. Consider summarizing current findings before continuing.";
            self.current_turn_budget_notice_emitted = true;
            self.tool_budget_notice_count = self.tool_budget_notice_count.saturating_add(1);
            self.last_tool_budget_warning = Some(message.to_string());
            return Some(message.to_string());
        }

        None
    }

    fn note_tool_truncation(&mut self, truncation: &ToolResultTruncationView) {
        self.tool_truncation_count = self.tool_truncation_count.saturating_add(1);
        self.current_turn_truncated_results = self.current_turn_truncated_results.saturating_add(1);
        self.last_tool_truncation_reason = Some(truncation.reason.clone());
    }

    fn summarize_result_metadata(metadata: &Option<Value>) -> Option<String> {
        let meta = metadata.as_ref()?.as_object()?;
        let mut parts = Vec::new();
        for key in [
            "file_path",
            "byte_count",
            "line_count",
            "replacements",
            "applied_edits",
            "command_type",
            "rewrite_suggestion",
            "url",
            "count",
        ] {
            if let Some(value) = meta.get(key) {
                let rendered = if let Some(s) = value.as_str() {
                    s.to_string()
                } else {
                    value.to_string()
                };
                parts.push(format!("{}={}", key, rendered));
            }
        }
        if parts.is_empty() {
            None
        } else {
            Some(parts.join(", "))
        }
    }

    fn extract_diff_preview(metadata: &Option<Value>) -> Option<String> {
        let diff = metadata
            .as_ref()
            .and_then(|meta| meta.get("diff_preview"))
            .and_then(|value| value.as_object())?;

        let mut lines = Vec::new();
        if let Some(removed) = diff.get("removed").and_then(|value| value.as_array()) {
            for line in removed.iter().filter_map(|value| value.as_str()) {
                lines.push(format!("-{}", line));
            }
            if let Some(extra) = diff.get("more_removed").and_then(|value| value.as_u64()) {
                if extra > 0 {
                    lines.push(format!("... {} more removed", extra));
                }
            }
        }
        if let Some(added) = diff.get("added").and_then(|value| value.as_array()) {
            for line in added.iter().filter_map(|value| value.as_str()) {
                lines.push(format!("+{}", line));
            }
            if let Some(extra) = diff.get("more_added").and_then(|value| value.as_u64()) {
                if extra > 0 {
                    lines.push(format!("... {} more added", extra));
                }
            }
        }

        if lines.is_empty() {
            None
        } else {
            Some(lines.join("\n"))
        }
    }

    fn output_preview(content: &str) -> String {
        const MAX_LINES: usize = 6;
        const MAX_CHARS: usize = 500;

        let lines = content.lines().take(MAX_LINES).collect::<Vec<_>>();
        let mut preview = lines.join("\n");
        if preview.chars().count() > MAX_CHARS {
            preview = preview.chars().take(MAX_CHARS).collect::<String>();
            preview.push_str("\n... [preview truncated]");
        } else if content.lines().count() > MAX_LINES {
            preview.push_str("\n... [more lines omitted]");
        }
        preview
    }

    fn failure_signature(tool_call: &ToolCall, error_type: Option<&str>) -> String {
        let mut hasher = Sha256::new();
        hasher.update(tool_call.name.as_bytes());
        hasher.update(tool_call.arguments.as_bytes());
        if let Some(kind) = error_type {
            hasher.update(kind.as_bytes());
        }
        let digest = hasher.finalize();
        format!(
            "{}:{}:{}",
            tool_call.name,
            error_type.unwrap_or("unknown"),
            hex_short(&digest)
        )
    }

    fn tool_truncation_from_metadata(metadata: &Option<Value>) -> Option<ToolResultTruncationView> {
        let tool_runtime = metadata
            .as_ref()
            .and_then(|meta| meta.get("tool_runtime"))
            .and_then(|value| value.as_object())?;
        let truncation = tool_runtime.get("truncation")?.as_object()?;
        Some(ToolResultTruncationView {
            reason: truncation
                .get("reason")
                .and_then(|value| value.as_str())
                .unwrap_or("unknown")
                .to_string(),
            original_bytes: truncation
                .get("original_bytes")
                .and_then(|value| value.as_u64())
                .unwrap_or(0) as usize,
            kept_bytes: truncation
                .get("kept_bytes")
                .and_then(|value| value.as_u64())
                .unwrap_or(0) as usize,
            omitted_bytes: truncation
                .get("omitted_bytes")
                .and_then(|value| value.as_u64())
                .unwrap_or(0) as usize,
        })
    }

    fn record_tool_execution_trace(
        &mut self,
        tool_call: &ToolCall,
        result: &ToolResult,
        started_at: Option<String>,
        duration_ms: u64,
        progress_updates: u32,
        parallel_batch: Option<u32>,
        input_bytes: usize,
    ) {
        let error_type = result.error_type.map(|kind| format!("{:?}", kind));
        if let Some(kind) = error_type.clone() {
            *self.tool_error_type_counts.entry(kind.clone()).or_insert(0) += 1;
        }

        let repeated_failure_count = if result.is_error {
            let signature = Self::failure_signature(tool_call, error_type.as_deref());
            let count = self
                .repeated_tool_failure_patterns
                .entry(signature)
                .and_modify(|existing| *existing = existing.saturating_add(1))
                .or_insert(1);
            if *count >= 2 {
                self.latest_repeated_tool_failure = Some(format!(
                    "{} [{}] x{}",
                    tool_call.name,
                    error_type.as_deref().unwrap_or("unknown"),
                    *count
                ));
            }
            *count
        } else {
            0
        };

        let truncation = Self::tool_truncation_from_metadata(&result.metadata);
        if let Some(ref truncation) = truncation {
            self.note_tool_truncation(truncation);
        }
        let trace = ToolExecutionTrace {
            call_id: tool_call.id.clone(),
            tool_name: tool_call.name.clone(),
            started_at,
            duration_ms,
            input_bytes,
            output_bytes: result.content.len(),
            progress_updates,
            success: !result.is_error,
            error_type,
            parallel_batch,
            truncation,
            repeated_failure_count,
            metadata_summary: Self::summarize_result_metadata(&result.metadata),
            diff_preview: Self::extract_diff_preview(&result.metadata),
            output_preview: Self::output_preview(&result.content),
        };
        self.current_tool_execution_traces.push(trace);
    }

    fn complete_tool_turn_artifact(&mut self) {
        if self.current_tool_execution_traces.is_empty() {
            self.current_tool_turn_started_at = None;
            return;
        }

        let total_calls = self.current_tool_execution_traces.len() as u32;
        let success_count = self
            .current_tool_execution_traces
            .iter()
            .filter(|trace| trace.success)
            .count() as u32;
        let failed_count = total_calls.saturating_sub(success_count);
        let mut current_error_type_counts = BTreeMap::new();
        for trace in &self.current_tool_execution_traces {
            if let Some(kind) = trace.error_type.as_ref() {
                *current_error_type_counts.entry(kind.clone()).or_insert(0) += 1;
            }
        }

        let artifact = ToolTurnArtifact {
            turn_index: self.tool_turn_counter,
            started_at: self.current_tool_turn_started_at.clone(),
            completed_at: Some(Self::now_timestamp()),
            total_calls,
            success_count,
            failed_count,
            total_output_bytes: self.total_tool_results_bytes,
            truncated_results: self.current_turn_truncated_results,
            progress_events: self.current_turn_tool_progress_events,
            parallel_batches: self.current_turn_parallel_batches,
            parallel_calls: self.current_turn_parallel_calls,
            max_parallel_batch_size: self.current_turn_max_parallel_batch_size,
            budget_notice_emitted: self.current_turn_budget_notice_emitted,
            budget_warning_emitted: self.current_turn_budget_warning_emitted,
            last_budget_warning: self.last_tool_budget_warning.clone(),
            latest_repeated_failure: self.latest_repeated_tool_failure.clone(),
            error_type_counts: current_error_type_counts,
            calls: self
                .current_tool_execution_traces
                .iter()
                .map(ToolExecutionTrace::to_view)
                .collect(),
        };

        if let Ok(path) = write_tool_turn_artifact(
            &self.context.working_dir_compat(),
            &self.context.session_id,
            &artifact,
        ) {
            self.last_tool_turn_artifact_path = Some(path.display().to_string());
        }

        self.last_tool_turn_completed_at = artifact.completed_at.clone();
        self.last_tool_turn_traces = self.current_tool_execution_traces.clone();
        self.current_tool_execution_traces.clear();
        self.current_tool_turn_started_at = None;
    }

    /// Run one user turn: send user message, loop through tool calls until final text response.
    pub async fn run_turn(
        &mut self,
        user_input: &str,
        source: QuerySource,
        event_tx: mpsc::UnboundedSender<EngineEvent>,
        mut confirm_rx: mpsc::UnboundedReceiver<ConfirmResponse>,
    ) -> Result<()> {
        self.current_query_source = source;
        self.rebuild_system_prompt();
        let _ = event_tx.send(EngineEvent::Thinking);

        let prompt_submit_ctx = HookContext {
            event: HookEvent::UserPromptSubmit.to_string(),
            session_id: self.context.session_id.clone(),
            working_dir: self.context.working_dir_compat().display().to_string(),
            tool_name: None,
            tool_input: None,
            tool_output: None,
            error: None,
            user_prompt: Some(user_input.to_string()),
            metadata: Some(json!({
                "query_source": format!("{:?}", self.current_query_source),
            })),
        };
        self.append_hook_outputs_as_system_message(
            HookEvent::UserPromptSubmit,
            prompt_submit_ctx,
            "System Auto-Context via user_prompt_submit hooks",
        )
        .await;

        // Optional pre-read hook before LLM call
        if let Some(ref hook_mgr) = self.hook_manager {
            let hook_ctx = HookContext {
                event: "pre_turn".into(),
                session_id: self.context.session_id.clone(),
                working_dir: self.context.working_dir_compat().display().to_string(),
                tool_name: None,
                tool_input: None,
                tool_output: None,
                error: None,
                user_prompt: Some(user_input.to_string()),
                metadata: None,
            };
            let results = hook_mgr.execute(HookEvent::PreTurn, &hook_ctx).await;
            let mut combined = String::new();
            for res in results {
                if let Some(out) = res.stdout {
                    combined.push_str(&out);
                    combined.push_str("\n\n");
                }
            }
            if !combined.is_empty() {
                self.messages.push(Message::system(format!(
                    "[System Auto-Context via pre_turn hooks]\n{}",
                    combined
                )));
            }
            self.append_hook_wake_notifications_as_system_message();
        }

        // Add user message
        self.messages.push(Message::user(user_input));
        self.persist_message("user", Some(user_input), None, None, None);

        // Reset tool/runtime counters for this turn
        self.reset_tool_turn_runtime();
        self.reset_prompt_cache_turn_runtime();
        self.recent_tool_calls.clear();
        self.consecutive_failures = 0;
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

            self.record_response_usage(&response.usage, &event_tx);

            self.maybe_compact_context(response.usage.prompt_tokens, &event_tx)
                .await;

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
            } else if response.stop_reason == Some(yode_llm::types::StopReason::StopSequence)
                || matches!(
                    response.stop_reason,
                    Some(yode_llm::types::StopReason::Other(_))
                )
            {
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
                        info!(
                            "Recovered {} leaked tool calls from text response (non-streaming).",
                            recovered.len()
                        );
                        assistant_msg.tool_calls = recovered;
                        self.violation_retries = 0;
                    } else if self.is_protocol_violation(&content) {
                        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
                        let bucket = self
                            .error_buckets
                            .entry(ToolErrorType::Protocol)
                            .or_insert(0);
                        *bucket += 1;
                    }
                }
            } else {
                self.violation_retries = 0;
            }

            assistant_msg.normalize_in_place();
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
                for outcome in &parallel_results {
                    let tc = &outcome.tool_call;
                    let result = self
                        .finalize_tool_result(
                            tc,
                            outcome.result.clone(),
                            outcome.started_at.clone(),
                            outcome.duration_ms,
                            outcome.progress_updates,
                            outcome.parallel_batch,
                        )
                        .await;
                    self.messages
                        .push(Message::tool_result(&tc.id, &result.content));
                    self.persist_message("tool", Some(&result.content), None, None, Some(&tc.id));

                    let _ = event_tx.send(EngineEvent::ToolResult {
                        id: tc.id.clone(),
                        name: tc.name.clone(),
                        result,
                    });
                }

                // Execute sequential tools one by one
                for tool_call in &sequential {
                    let outcome = self
                        .handle_tool_call(tool_call, &event_tx, &mut confirm_rx, None)
                        .await?;
                    let result = self
                        .finalize_tool_result(
                            &outcome.tool_call,
                            outcome.result,
                            outcome.started_at,
                            outcome.duration_ms,
                            outcome.progress_updates,
                            outcome.parallel_batch,
                        )
                        .await;
                    self.messages
                        .push(Message::tool_result(&outcome.tool_call.id, &result.content));
                    self.persist_message(
                        "tool",
                        Some(&result.content),
                        None,
                        None,
                        Some(&outcome.tool_call.id),
                    );

                    let _ = event_tx.send(EngineEvent::ToolResult {
                        id: outcome.tool_call.id.clone(),
                        name: outcome.tool_call.name.clone(),
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

            self.maybe_refresh_live_session_memory(Some(&event_tx));
            self.complete_tool_turn_artifact();
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
        source: QuerySource,
        event_tx: mpsc::UnboundedSender<EngineEvent>,
        mut confirm_rx: mpsc::UnboundedReceiver<ConfirmResponse>,
        cancel_token: Option<CancellationToken>,
    ) -> Result<()> {
        self.current_query_source = source;
        self.rebuild_system_prompt();
        let prompt_submit_ctx = HookContext {
            event: HookEvent::UserPromptSubmit.to_string(),
            session_id: self.context.session_id.clone(),
            working_dir: self.context.working_dir_compat().display().to_string(),
            tool_name: None,
            tool_input: None,
            tool_output: None,
            error: None,
            user_prompt: Some(user_input.to_string()),
            metadata: Some(json!({
                "query_source": format!("{:?}", self.current_query_source),
            })),
        };
        self.append_hook_outputs_as_system_message(
            HookEvent::UserPromptSubmit,
            prompt_submit_ctx,
            "System Auto-Context via user_prompt_submit hooks",
        )
        .await;
        // Optional pre-read hook before LLM call
        if let Some(ref hook_mgr) = self.hook_manager {
            let hook_ctx = HookContext {
                event: "pre_turn".into(),
                session_id: self.context.session_id.clone(),
                working_dir: self.context.working_dir_compat().display().to_string(),
                tool_name: None,
                tool_input: None,
                tool_output: None,
                error: None,
                user_prompt: Some(user_input.to_string()),
                metadata: None,
            };
            let results = hook_mgr.execute(HookEvent::PreTurn, &hook_ctx).await;
            let mut combined = String::new();
            for res in results {
                if let Some(out) = res.stdout {
                    combined.push_str(&out);
                    combined.push_str("\n\n");
                }
            }
            if !combined.is_empty() {
                self.messages.push(Message::system(format!(
                    "[System Auto-Context via pre_turn hooks]\n{}",
                    combined
                )));
            }
            self.append_hook_wake_notifications_as_system_message();
        }

        self.messages.push(Message::user(user_input));
        self.persist_message("user", Some(user_input), None, None, None);

        // Reset tool/runtime counters for this turn
        self.reset_tool_turn_runtime();
        self.reset_prompt_cache_turn_runtime();
        self.recent_tool_calls.clear();
        self.consecutive_failures = 0;
        self.violation_retries = 0;
        self.files_read.clear();
        self.files_modified.clear();

        loop {
            // Check cancellation before each LLM call
            if let Some(ref token) = cancel_token {
                if token.is_cancelled() {
                    self.complete_tool_turn_artifact();
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
                )
                .await;
                match result {
                    Ok(inner) => inner,
                    Err(_) => Err(anyhow::anyhow!(
                        "LLM 调用超时 ({}秒)",
                        LLM_TIMEOUT_SECS.max(600)
                    )),
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
                    warn!(
                        "Streaming turn timed out after {:?}; forcing completion",
                        hard_turn_timeout
                    );
                    stream_handle.abort();
                    stalled = true;
                    break;
                }

                // Watchdog: no progress events for too long => force break.
                if last_progress_at.elapsed() > stall_timeout {
                    warn!(
                        "Streaming stalled for {:?} without progress; forcing completion",
                        stall_timeout
                    );
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
                    match tokio::time::timeout(std::time::Duration::from_secs(2), stream_rx.recv())
                        .await
                    {
                        Ok(Some(ev)) => {
                            last_progress_at = std::time::Instant::now();
                            let is_done = matches!(ev, StreamEvent::Done(_));
                            Self::process_stream_event(
                                ev,
                                &mut full_text,
                                &mut full_reasoning,
                                &mut tool_calls,
                                &mut final_response,
                                &event_tx,
                            );
                            if is_done {
                                break;
                            }
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
                        blocks.push(yode_llm::types::ContentBlock::Thinking {
                            thinking: full_reasoning.clone(),
                            signature: None,
                        });
                    }
                    if !full_text.is_empty() {
                        blocks.push(yode_llm::types::ContentBlock::Text {
                            text: full_text.clone(),
                        });
                    }

                    let assistant_msg = Message {
                        role: Role::Assistant,
                        content: if full_text.is_empty() {
                            None
                        } else {
                            Some(full_text.clone())
                        },
                        reasoning: if full_reasoning.is_empty() {
                            None
                        } else {
                            Some(full_reasoning.clone())
                        },
                        content_blocks: blocks,
                        tool_calls: vec![],
                        tool_call_id: None,
                        images: Vec::new(),
                    }
                    .normalized();
                    self.messages.push(assistant_msg);
                    self.persist_message(
                        "assistant",
                        if full_text.is_empty() {
                            None
                        } else {
                            Some(&full_text)
                        },
                        if full_reasoning.is_empty() {
                            None
                        } else {
                            Some(&full_reasoning)
                        },
                        None,
                        None,
                    );
                }
                if stalled {
                    let _ = event_tx.send(EngineEvent::TextComplete(
                        "[Watchdog] Streaming stalled; forcing graceful stop. Please retry with narrower scope.".to_string(),
                    ));
                }
                self.complete_tool_turn_artifact();
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
                                                self.complete_tool_turn_artifact();
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
                                        self.complete_tool_turn_artifact();
                                        let _ = event_tx.send(EngineEvent::Done);
                                        return Ok(());
                                    }
                                }

                                let retry_request = ChatRequest {
                                    model: self.context.model.clone(),
                                    messages: self.messages.clone(),
                                    tools: convert_tool_definitions(&self.tools),
                                    temperature: Some(0.7),
                                    max_tokens: Some(self.context.get_max_tokens()),
                                };

                                // Retry streaming
                                let (retry_tx, mut retry_rx) = mpsc::channel::<StreamEvent>(256);
                                let retry_provider = self.provider.clone();
                                let retry_handle = tokio::spawn(async move {
                                    let result = tokio::time::timeout(
                                        std::time::Duration::from_secs(LLM_TIMEOUT_SECS),
                                        retry_provider.chat_stream(retry_request, retry_tx),
                                    )
                                    .await;
                                    match result {
                                        Ok(inner) => inner,
                                        Err(_) => Err(anyhow::anyhow!(
                                            "LLM 调用超时 ({}秒)",
                                            LLM_TIMEOUT_SECS
                                        )),
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
                                                Self::process_stream_event(
                                                    ev,
                                                    &mut full_text,
                                                    &mut full_reasoning,
                                                    &mut tool_calls,
                                                    &mut final_response,
                                                    &event_tx,
                                                );
                                                if is_done {
                                                    break;
                                                }
                                            }
                                            None => break,
                                        }
                                    }
                                }

                                if retry_cancelled {
                                    if !full_text.is_empty() || !full_reasoning.is_empty() {
                                        let mut blocks = Vec::new();
                                        if !full_reasoning.is_empty() {
                                            blocks.push(yode_llm::types::ContentBlock::Thinking {
                                                thinking: full_reasoning.clone(),
                                                signature: None,
                                            });
                                        }
                                        if !full_text.is_empty() {
                                            blocks.push(yode_llm::types::ContentBlock::Text {
                                                text: full_text.clone(),
                                            });
                                        }

                                        let assistant_msg = Message {
                                            role: Role::Assistant,
                                            content: if full_text.is_empty() {
                                                None
                                            } else {
                                                Some(full_text.clone())
                                            },
                                            reasoning: if full_reasoning.is_empty() {
                                                None
                                            } else {
                                                Some(full_reasoning.clone())
                                            },
                                            content_blocks: blocks,
                                            tool_calls: vec![],
                                            tool_call_id: None,
                                            images: Vec::new(),
                                        }
                                        .normalized();
                                        self.messages.push(assistant_msg);
                                        self.persist_message(
                                            "assistant",
                                            if full_text.is_empty() {
                                                None
                                            } else {
                                                Some(&full_text)
                                            },
                                            if full_reasoning.is_empty() {
                                                None
                                            } else {
                                                Some(&full_reasoning)
                                            },
                                            None,
                                            None,
                                        );
                                    }
                                    self.complete_tool_turn_artifact();
                                    let _ = event_tx.send(EngineEvent::Done);
                                    return Ok(());
                                }

                                match retry_handle.await {
                                    Ok(Ok(())) => {
                                        retry_succeeded = true;
                                        break;
                                    }
                                    Ok(Err(e2)) => {
                                        warn!(
                                            "Stream retry {}/{} failed: {}",
                                            attempt + 1,
                                            max_attempts,
                                            e2
                                        );
                                    }
                                    Err(e2) => {
                                        warn!(
                                            "Stream retry {}/{} panicked: {}",
                                            attempt + 1,
                                            max_attempts,
                                            e2
                                        );
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
                                self.complete_tool_turn_artifact();
                                let _ = event_tx.send(EngineEvent::Done);
                                return Err(err).context("LLM chat request failed");
                            }
                        } else {
                            // Fatal error — no retry
                            let _ = event_tx.send(EngineEvent::Error(format!("{}", err)));
                            self.complete_tool_turn_artifact();
                            let _ = event_tx.send(EngineEvent::Done);
                            return Err(err).context("LLM chat request failed");
                        }
                    }
                    // else: stream failed but we have partial content, keep it
                }
            }

            if let Some(ref resp) = final_response {
                self.record_response_usage(&resp.usage, &event_tx);
                self.maybe_compact_context(resp.usage.prompt_tokens, &event_tx)
                    .await;
            }

            // Build assistant message from stream
            let mut assistant_msg = Message {
                role: Role::Assistant,
                content: if full_text.is_empty() {
                    None
                } else {
                    Some(full_text.clone())
                },
                reasoning: if full_reasoning.is_empty() {
                    None
                } else {
                    Some(full_reasoning.clone())
                },
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
                } else if (resp.stop_reason == Some(yode_llm::types::StopReason::StopSequence)
                    || matches!(
                        resp.stop_reason,
                        Some(yode_llm::types::StopReason::Other(_))
                    ))
                    && (full_text.contains("[tool_") || full_text.contains("<tool_"))
                {
                    warn!("LLM streaming response stopped via stop sequence or other reason but contains incomplete tool tags. Reason: {:?}", resp.stop_reason);
                }
            }

            // --- PR-1: Strict Protocol Gate & Auto Recovery/Retry ---
            if assistant_msg.tool_calls.is_empty()
                && !full_text.is_empty()
                && self.is_protocol_violation(&full_text)
            {
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
                assistant_msg
                    .content_blocks
                    .push(yode_llm::types::ContentBlock::Thinking {
                        thinking: full_reasoning.clone(),
                        signature: None,
                    });
            }
            if !full_text.is_empty() {
                assistant_msg
                    .content_blocks
                    .push(yode_llm::types::ContentBlock::Text {
                        text: full_text.clone(),
                    });
            }

            assistant_msg.normalize_in_place();
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
                            self.complete_tool_turn_artifact();
                            let _ = event_tx.send(EngineEvent::Done);
                            return Ok(());
                        }
                    }

                    info!("Executing {} tools in parallel (streaming)", parallel.len());
                    let parallel_results = self.execute_tools_parallel(&parallel, &event_tx).await;

                    for outcome in parallel_results {
                        let tc = &outcome.tool_call;
                        let result = self
                            .finalize_tool_result(
                                tc,
                                outcome.result,
                                outcome.started_at,
                                outcome.duration_ms,
                                outcome.progress_updates,
                                outcome.parallel_batch,
                            )
                            .await;
                        self.messages
                            .push(Message::tool_result(&tc.id, &result.content));
                        self.persist_message(
                            "tool",
                            Some(&result.content),
                            None,
                            None,
                            Some(&tc.id),
                        );

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
                            self.complete_tool_turn_artifact();
                            let _ = event_tx.send(EngineEvent::Done);
                            return Ok(());
                        }
                    }

                    let outcome = self
                        .handle_tool_call(
                            tool_call,
                            &event_tx,
                            &mut confirm_rx,
                            cancel_token.as_ref(),
                        )
                        .await?;

                    let result = self
                        .finalize_tool_result(
                            &outcome.tool_call,
                            outcome.result,
                            outcome.started_at,
                            outcome.duration_ms,
                            outcome.progress_updates,
                            outcome.parallel_batch,
                        )
                        .await;
                    self.messages
                        .push(Message::tool_result(&outcome.tool_call.id, &result.content));
                    self.persist_message(
                        "tool",
                        Some(&result.content),
                        None,
                        None,
                        Some(&outcome.tool_call.id),
                    );

                    let _ = event_tx.send(EngineEvent::ToolResult {
                        id: outcome.tool_call.id.clone(),
                        name: outcome.tool_call.name.clone(),
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
                    self.maybe_refresh_live_session_memory(Some(&event_tx));
                    self.complete_tool_turn_artifact();
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
                self.complete_tool_turn_artifact();
                let _ = event_tx.send(EngineEvent::Done);
                break;
            }
        }

        Ok(())
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
            let is_discovery_tool =
                matches!(tool_name, "ls" | "glob" | "read_file" | "project_map");
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
            .and_then(|v| {
                v.get("file_path")
                    .and_then(|p| p.as_str())
                    .map(String::from)
            });

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
                     If you changed a function signature, grep for callers to update them too.]",
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
                 Step back: What assumption might be wrong? Try a different tool or strategy.]",
            );
        } else if self.consecutive_failures >= 3 {
            result.content.push_str(
                "\n\n[3+ consecutive failures. STOP searching and summarize what you know. \
                 Present your findings to the user and ask for guidance.]",
            );
        }

        // === Periodic intelligence ===

        // Goal reminder at 5 calls
        if self.tool_call_count == TOOL_GOAL_REMINDER {
            result.content.push_str(
                "\n\n[5 tool calls done. Quick check: Do you have enough information to act? \
                 If yes, stop gathering and start implementing.]",
            );
        }

        // Self-reflection every 10 calls
        if self.tool_call_count > 0 && self.tool_call_count.is_multiple_of(TOOL_REFLECT_INTERVAL) {
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
        if let Some(message) = self.maybe_record_tool_budget_warning() {
            result.content.push_str(&format!("\n\n[{}]", message));
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
                caps.read_only
                    && matches!(self.permissions.check(&tc.name), PermissionAction::Allow)
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
        &mut self,
        tool_calls: &[ToolCall],
        event_tx: &mpsc::UnboundedSender<EngineEvent>,
    ) -> Vec<ToolExecutionOutcome> {
        use futures::future::join_all;

        let mut futures = Vec::new();
        let batch_id = self.register_parallel_batch(tool_calls.len());

        for tc in tool_calls {
            let tool = match self.tools.get(&tc.name) {
                Some(t) => t,
                None => continue,
            };

            let mut params: serde_json::Value = serde_json::from_str(&tc.arguments)
                .unwrap_or_else(|_| serde_json::Value::Object(serde_json::Map::new()));
            let working_dir = self.current_runtime_working_dir().await;

            if let Some(blocked) = self
                .run_pre_tool_use_hook(&tc.name, &tc.arguments, &working_dir, &mut params)
                .await
            {
                let tc_clone = tc.clone();
                futures.push(Box::pin(async move {
                    ToolExecutionOutcome {
                        tool_call: tc_clone,
                        result: blocked,
                        started_at: Some(Self::now_timestamp()),
                        duration_ms: 0,
                        progress_updates: 0,
                        last_progress_message: None,
                        parallel_batch: Some(batch_id),
                    }
                })
                    as Pin<
                        Box<dyn std::future::Future<Output = ToolExecutionOutcome> + Send>,
                    >);
                continue;
            }

            let schema = tool.parameters_schema();
            if let Err(msg) = validation::validate_and_coerce(&schema, &mut params) {
                let tc_clone = tc.clone();
                let result = ToolResult::error_typed(
                    format!("Parameter validation failed: {}", msg),
                    ToolErrorType::Validation,
                    true,
                    Some(format!("Fix the parameters and retry. Schema: {}", schema)),
                );
                futures.push(Box::pin(async move {
                    ToolExecutionOutcome {
                        tool_call: tc_clone,
                        result,
                        started_at: Some(Self::now_timestamp()),
                        duration_ms: 0,
                        progress_updates: 0,
                        last_progress_message: None,
                        parallel_batch: Some(batch_id),
                    }
                })
                    as Pin<
                        Box<dyn std::future::Future<Output = ToolExecutionOutcome> + Send>,
                    >);
                continue;
            }
            let effective_arguments =
                serde_json::to_string(&params).unwrap_or_else(|_| tc.arguments.clone());

            let _ = event_tx.send(EngineEvent::ToolCallStart {
                id: tc.id.clone(),
                name: tc.name.clone(),
                arguments: effective_arguments,
            });

            info!(
                "Executing tool in parallel: {} (auto-allowed, read-only)",
                tc.name
            );

            let (p_tx, mut p_rx) = mpsc::unbounded_channel::<yode_tools::tool::ToolProgress>();
            let event_tx_inner = event_tx.clone();
            let tc_id = tc.id.clone();
            let tc_name = tc.name.clone();
            let progress_counter = Arc::new(AtomicU64::new(0));
            let progress_counter_inner = Arc::clone(&progress_counter);
            let last_progress_message = Arc::new(std::sync::Mutex::new(None::<String>));
            let last_progress_message_inner = Arc::clone(&last_progress_message);
            tokio::spawn(async move {
                while let Some(progress) = p_rx.recv().await {
                    progress_counter_inner.fetch_add(1, Ordering::Relaxed);
                    if let Ok(mut slot) = last_progress_message_inner.lock() {
                        *slot = Some(progress.message.clone());
                    }
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
            let started_at = Some(Self::now_timestamp());

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
                let progress_updates = progress_counter.load(Ordering::Relaxed) as u32;
                let last_progress_message = last_progress_message
                    .lock()
                    .ok()
                    .and_then(|slot| slot.clone());
                ToolExecutionOutcome {
                    tool_call: tc_clone,
                    result,
                    started_at,
                    duration_ms: start.elapsed().as_millis() as u64,
                    progress_updates,
                    last_progress_message,
                    parallel_batch: Some(batch_id),
                }
            }));
        }

        let outcomes = join_all(futures).await;
        for outcome in &outcomes {
            self.record_tool_progress_summary(
                &outcome.tool_call.name,
                outcome.progress_updates,
                outcome.last_progress_message.clone(),
            );
        }
        outcomes
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
                        let lines = metadata
                            .get("total_lines")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0) as usize;
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
        if text.len() > RECOVERY_TEXT_MAX_CHARS
            || (!text.contains("[tool_use") && !text.contains("[DUMMY_TOOL"))
        {
            return recovered;
        }

        // Pattern 1: Look for [tool_use id=... name=...] { ... }
        let tag_re =
            Regex::new(r"(?s)\[tool_use\s+id=([^\s\]>]+)\s+name=([^\s\]>]+)[\]>]\s*(\{.*?\})")
                .unwrap();
        for cap in tag_re.captures_iter(text).take(RECOVERY_MAX_CALLS) {
            recovered.push(ToolCall {
                id: cap[1].to_string(),
                name: cap[2].to_string(),
                arguments: cap[3].to_string(),
            });
        }

        // Pattern 2: Look for raw JSON blocks only in small texts and only if no explicit tags found.
        if recovered.is_empty() {
            let json_re =
                Regex::new(r#"(?s)\{\s*"(?:command|file_path|pattern|query)"\s*:.*?\}"#).unwrap();
            for (i, m) in json_re.find_iter(text).take(RECOVERY_MAX_CALLS).enumerate() {
                let json_str = m.as_str();
                let name = if json_str.contains("\"command\"") {
                    "bash"
                } else if json_str.contains("\"file_path\"") && json_str.contains("\"old_string\"")
                {
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
                let original_bytes = size;

                result.content = format!(
                    "{}\n\n[AGGREGATE BUDGET EXCEEDED: Remaining {} bytes of this result omitted. \
                     STOP requesting large outputs in this turn to avoid context overflow.]",
                    preview,
                    size - preview_len
                );
                set_tool_runtime_truncation_metadata(
                    result,
                    &ToolResultTruncationView {
                        reason: "aggregate_budget_partial".to_string(),
                        original_bytes,
                        kept_bytes: result.content.len(),
                        omitted_bytes: original_bytes.saturating_sub(result.content.len()),
                    },
                );
            } else {
                // We were already over the limit
                let original_bytes = size;
                result.content = format!(
                    "[AGGREGATE BUDGET EXCEEDED: Full result ({} bytes) omitted to prevent context overflow. \
                     Summarize your current findings instead.]",
                    size
                );
                set_tool_runtime_truncation_metadata(
                    result,
                    &ToolResultTruncationView {
                        reason: "aggregate_budget_omitted".to_string(),
                        original_bytes,
                        kept_bytes: result.content.len(),
                        omitted_bytes: original_bytes.saturating_sub(result.content.len()),
                    },
                );
            }
        }
    }

    async fn finalize_tool_result(
        &mut self,
        tool_call: &ToolCall,
        mut result: ToolResult,
        started_at: Option<String>,
        duration_ms: u64,
        progress_updates: u32,
        parallel_batch: Option<u32>,
    ) -> ToolResult {
        self.track_file_access(&tool_call.name, &result);
        result = truncate_tool_result(result);
        self.enforce_tool_budget(&mut result);
        self.inject_intelligence(&mut result, &tool_call.name, &tool_call.arguments);

        let working_dir = self.current_runtime_working_dir().await;
        let effective_input = Self::parse_tool_input(&tool_call.arguments);
        self.run_post_tool_use_hooks(tool_call, &effective_input, &working_dir, &mut result)
            .await;
        annotate_tool_result_runtime_metadata(
            &mut result,
            duration_ms,
            progress_updates,
            parallel_batch,
            tool_call.arguments.len(),
        );

        self.record_tool_execution_trace(
            tool_call,
            &result,
            started_at,
            duration_ms,
            progress_updates,
            parallel_batch,
            tool_call.arguments.len(),
        );
        self.record_tool_result_status(&tool_call.id, &result);
        result
    }

    fn immediate_tool_outcome(
        tool_call: &ToolCall,
        started_at: &Option<String>,
        result: ToolResult,
    ) -> ToolExecutionOutcome {
        ToolExecutionOutcome {
            tool_call: tool_call.clone(),
            result,
            started_at: started_at.clone(),
            duration_ms: 0,
            progress_updates: 0,
            last_progress_message: None,
            parallel_batch: None,
        }
    }

    /// Handle a single tool call...
    async fn handle_tool_call(
        &mut self,
        tool_call: &ToolCall,
        event_tx: &mpsc::UnboundedSender<EngineEvent>,
        confirm_rx: &mut mpsc::UnboundedReceiver<ConfirmResponse>,
        cancel_token: Option<&CancellationToken>,
    ) -> Result<ToolExecutionOutcome> {
        let started_at = Some(Self::now_timestamp());
        let tool = match self.tools.get(&tool_call.name) {
            Some(t) => t,
            None => {
                return Ok(ToolExecutionOutcome {
                    tool_call: tool_call.clone(),
                    result: ToolResult::error(format!("Unknown tool: {}", tool_call.name)),
                    started_at,
                    duration_ms: 0,
                    progress_updates: 0,
                    last_progress_message: None,
                    parallel_batch: None,
                });
            }
        };

        // Parse arguments
        let original_params: serde_json::Value = serde_json::from_str(&tool_call.arguments)
            .unwrap_or_else(|_| serde_json::Value::Object(serde_json::Map::new()));
        let mut params = original_params.clone();
        let working_dir = self.current_runtime_working_dir().await;

        if let Some(blocked) = self
            .run_pre_tool_use_hook(
                &tool_call.name,
                &tool_call.arguments,
                &working_dir,
                &mut params,
            )
            .await
        {
            return Ok(ToolExecutionOutcome {
                tool_call: tool_call.clone(),
                result: blocked,
                started_at,
                duration_ms: 0,
                progress_updates: 0,
                last_progress_message: None,
                parallel_batch: None,
            });
        }

        // Claude-style unified recovery gate:
        // when re-anchor is required, only allow lightweight discovery tools.
        if self.recovery_state == RecoveryState::ReanchorRequired {
            let allow_reanchor_tool = matches!(
                tool_call.name.as_str(),
                "ls" | "glob" | "read_file" | "project_map"
            );
            if !allow_reanchor_tool {
                return Ok(ToolExecutionOutcome {
                    tool_call: tool_call.clone(),
                    result: ToolResult::error_typed(
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
                    ),
                    started_at,
                    duration_ms: 0,
                    progress_updates: 0,
                    last_progress_message: None,
                    parallel_batch: None,
                });
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
                return Ok(Self::immediate_tool_outcome(
                    tool_call,
                    &started_at,
                    ToolResult::error_typed(
                        format!("Security Block: '{}' is an invalid path. {}", file_path, r),
                        ToolErrorType::Validation,
                        true,
                        Some(
                            "Correct the path to a literal, normalized format and try again."
                                .to_string(),
                        ),
                    ),
                ));
            }
        }

        // Check permissions (with content matching for bash commands)
        let command_content = if tool_call.name == "bash" {
            params
                .get("command")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        } else {
            None
        };
        let effective_arguments =
            serde_json::to_string(&params).unwrap_or_else(|_| tool_call.arguments.clone());
        let input_changed_by_hook = params != original_params;

        // Unified project/command gate (Claude-style preflight): block mismatched
        // language-specific commands until project root assumptions are corrected.
        if let Some(reason) = self.language_command_mismatch(&tool_call.name, &params) {
            return Ok(Self::immediate_tool_outcome(
                tool_call,
                &started_at,
                ToolResult::error_typed(
                    format!("Command blocked by project gate: {}", reason),
                    ToolErrorType::Validation,
                    true,
                    Some("Re-anchor with ls/glob/read on the target project root, then run matching build tooling.".to_string()),
                ),
            ));
        }

        // --- Tool Chain Validation ---
        if tool_call.name == "edit_file" || tool_call.name == "write_file" {
            if let Some(file_path) = params.get("file_path").and_then(|v| v.as_str()) {
                if !self.files_read.contains_key(file_path) {
                    return Ok(Self::immediate_tool_outcome(
                        tool_call,
                        &started_at,
                        ToolResult::error_typed(
                            format!("You must read the file '{}' with read_file before editing or overwriting it.", file_path),
                            ToolErrorType::Validation,
                            true,
                            Some(format!("Call read_file(file_path=\"{}\") first.", file_path)),
                        ),
                    ));
                }
            }
        }

        let permission_explanation = self
            .permissions
            .explain_with_content(&tool_call.name, command_content.as_deref());
        self.last_permission_tool = Some(tool_call.name.clone());
        self.last_permission_action = Some(permission_explanation.action.label().to_string());
        self.last_permission_explanation = Some(permission_explanation.reason.clone());
        self.write_permission_artifact(
            "permission_manager",
            &tool_call.name,
            permission_explanation.action.label(),
            &permission_explanation.reason,
            &params,
            &effective_arguments,
            &original_params,
            &tool_call.arguments,
            input_changed_by_hook,
        );
        let action = permission_explanation.action.clone();

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

                let is_recursive_ls = cmd_lower.contains("ls ")
                    && (cmd_lower.contains("-r") || cmd_lower.contains("-lar"));

                if is_forbidden || is_recursive_ls {
                    let (cmd_name, alternative) = if is_forbidden {
                        let matched = forbidden_binaries
                            .iter()
                            .find(|&&b| cmd_lower.contains(b))
                            .unwrap_or(&"search");
                        (
                            *matched,
                            match *matched {
                                "find" => "glob",
                                _ => "grep",
                            },
                        )
                    } else {
                        ("ls -R", "ls (without -R) or project_map")
                    };

                    return Ok(Self::immediate_tool_outcome(
                        tool_call,
                        &started_at,
                        ToolResult::error_typed(
                            format!("Command blocked: Use the dedicated '{}' tool instead of running '{}' via bash.", alternative, cmd_name),
                            ToolErrorType::Validation,
                            true,
                            Some(format!("Running search/discovery via bash is inefficient. Use the '{}' tool for better results and TUI display.", alternative)),
                        ),
                    ));
                }

                if CommandClassifier::classify(cmd) == CommandRiskLevel::Destructive {
                    self.last_permission_action = Some("deny".to_string());
                    self.last_permission_explanation = Some(
                        "Dangerous bash command blocked by destructive-command guard. Use a safer non-destructive probe first."
                            .to_string(),
                    );
                    self.write_permission_artifact(
                        "destructive_guard",
                        &tool_call.name,
                        "deny",
                        "Dangerous bash command blocked by destructive-command guard. Use a safer non-destructive probe first.",
                        &params,
                        &effective_arguments,
                        &original_params,
                        &tool_call.arguments,
                        input_changed_by_hook,
                    );
                    return Ok(Self::immediate_tool_outcome(
                        tool_call,
                        &started_at,
                        ToolResult::error_typed(
                            format!("Command blocked (destructive): {}", cmd),
                            ToolErrorType::PermissionDeny,
                            false,
                            Some(
                                "This command is classified as destructive and cannot be executed. Stop and propose a safer fallback such as `git status`, `git diff`, `ls`, or a dry-run variant before attempting any mutation again."
                                    .to_string(),
                            ),
                        ),
                    ));
                }
                // Other risk levels handled by permission system
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
                    arguments: effective_arguments.clone(),
                });
            }
            PermissionAction::Confirm => {
                let permission_request_ctx = HookContext {
                    event: HookEvent::PermissionRequest.to_string(),
                    session_id: self.context.session_id.clone(),
                    working_dir: self.context.working_dir_compat().display().to_string(),
                    tool_name: Some(tool_call.name.clone()),
                    tool_input: Some(params.clone()),
                    tool_output: None,
                    error: None,
                    user_prompt: None,
                    metadata: Some(json!({
                        "decision": "confirm",
                        "effective_input_snapshot": params.clone(),
                        "effective_arguments_snapshot": effective_arguments.clone(),
                        "original_input_snapshot": original_params.clone(),
                        "original_arguments_snapshot": tool_call.arguments.clone(),
                        "input_changed_by_hook": input_changed_by_hook,
                    })),
                };
                self.execute_advisory_hooks(HookEvent::PermissionRequest, permission_request_ctx)
                    .await;

                let _ = event_tx.send(EngineEvent::ToolConfirmRequired {
                    id: tool_call.id.clone(),
                    name: tool_call.name.clone(),
                    arguments: effective_arguments.clone(),
                });

                debug!("Waiting for user confirmation: tool={}", tool_call.name);
                let confirm_start = std::time::Instant::now();
                let confirm_timeout = std::time::Duration::from_secs(90);
                loop {
                    if confirm_start.elapsed() > confirm_timeout {
                        return Ok(Self::immediate_tool_outcome(
                            tool_call,
                            &started_at,
                            ToolResult::error_typed(
                                format!("Confirmation timed out for tool '{}'", tool_call.name),
                                ToolErrorType::Timeout,
                                true,
                                Some("No confirmation was received within 90s. Re-run or switch to a read-only alternative.".to_string()),
                            ),
                        ));
                    }

                    if let Some(token) = cancel_token {
                        if token.is_cancelled() {
                            return Ok(Self::immediate_tool_outcome(
                                tool_call,
                                &started_at,
                                ToolResult::error_typed(
                                    format!("Tool confirmation cancelled: {}", tool_call.name),
                                    ToolErrorType::Timeout,
                                    true,
                                    Some(
                                        "User cancelled while waiting for confirmation."
                                            .to_string(),
                                    ),
                                ),
                            ));
                        }
                    }

                    match tokio::time::timeout(
                        std::time::Duration::from_millis(500),
                        confirm_rx.recv(),
                    )
                    .await
                    {
                        Ok(Some(ConfirmResponse::Allow)) => {
                            info!("Tool {} confirmed by user", tool_call.name);
                            break;
                        }
                        Ok(Some(ConfirmResponse::Deny)) => {
                            info!("Tool {} denied by user", tool_call.name);
                            self.permissions.record_denial(&tool_call.name);
                            self.write_permission_artifact(
                                "user_confirmation",
                                &tool_call.name,
                                "deny",
                                "Tool execution denied by user.",
                                &params,
                                &effective_arguments,
                                &original_params,
                                &tool_call.arguments,
                                input_changed_by_hook,
                            );
                            let denied_ctx = HookContext {
                                event: HookEvent::PermissionDenied.to_string(),
                                session_id: self.context.session_id.clone(),
                                working_dir: self
                                    .context
                                    .working_dir_compat()
                                    .display()
                                    .to_string(),
                                tool_name: Some(tool_call.name.clone()),
                                tool_input: Some(params.clone()),
                                tool_output: None,
                                error: Some("Tool execution denied by user.".to_string()),
                                user_prompt: None,
                                metadata: Some(json!({
                                    "source": "user_confirmation",
                                    "effective_input_snapshot": params.clone(),
                                    "effective_arguments_snapshot": effective_arguments.clone(),
                                    "original_input_snapshot": original_params.clone(),
                                    "original_arguments_snapshot": tool_call.arguments.clone(),
                                    "input_changed_by_hook": input_changed_by_hook,
                                })),
                            };
                            self.execute_advisory_hooks(HookEvent::PermissionDenied, denied_ctx)
                                .await;
                            return Ok(Self::immediate_tool_outcome(
                                tool_call,
                                &started_at,
                                ToolResult::error("Tool execution denied by user.".to_string()),
                            ));
                        }
                        Ok(None) => {
                            return Ok(Self::immediate_tool_outcome(
                                tool_call,
                                &started_at,
                                ToolResult::error_typed(
                                    "Confirmation channel closed.".to_string(),
                                    ToolErrorType::Execution,
                                    true,
                                    Some("Please retry the action. If this repeats, check TUI confirmation event handling.".to_string()),
                                ),
                            ));
                        }
                        Err(_) => {
                            // periodic wakeup for cancellation and timeout checks
                        }
                    }
                }
            }
            PermissionAction::Deny => {
                let denied_ctx = HookContext {
                    event: HookEvent::PermissionDenied.to_string(),
                    session_id: self.context.session_id.clone(),
                    working_dir: self.context.working_dir_compat().display().to_string(),
                    tool_name: Some(tool_call.name.clone()),
                    tool_input: Some(params.clone()),
                    tool_output: None,
                    error: Some(format!("Tool {} is not permitted.", tool_call.name)),
                    user_prompt: None,
                    metadata: Some(json!({
                        "source": "permission_manager",
                        "effective_input_snapshot": params.clone(),
                        "effective_arguments_snapshot": effective_arguments.clone(),
                        "original_input_snapshot": original_params.clone(),
                        "original_arguments_snapshot": tool_call.arguments.clone(),
                        "input_changed_by_hook": input_changed_by_hook,
                    })),
                };
                self.execute_advisory_hooks(HookEvent::PermissionDenied, denied_ctx)
                    .await;
                return Ok(Self::immediate_tool_outcome(
                    tool_call,
                    &started_at,
                    ToolResult::error_typed(
                        format!(
                            "Tool {} is not permitted. {}",
                            tool_call.name, permission_explanation.reason
                        ),
                        ToolErrorType::PermissionDeny,
                        false,
                        Some(
                            "Use a safer read-only tool first, or switch permission mode / rules explicitly before retrying."
                                .to_string(),
                        ),
                    ),
                ));
            }
        }

        // Dedup detection: warn if same tool+args was called recently
        let call_sig = (tool_call.name.clone(), effective_arguments.clone());
        let current_sig_text = format!("{}:{}", tool_call.name, effective_arguments);

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
            return Ok(Self::immediate_tool_outcome(
                tool_call,
                &started_at,
                ToolResult::error_typed(
                    format!(
                        "Blocked repeated failing call: {} is being retried with identical arguments after multiple failures.",
                        tool_call.name
                    ),
                    ToolErrorType::Validation,
                    true,
                    Some("Do not retry the same call. Re-anchor first (ls/glob/read), then change tool arguments.".to_string()),
                ),
            ));
        }

        if self.recent_tool_calls.contains(&call_sig) && !is_observer_tool {
            return Ok(Self::immediate_tool_outcome(
                tool_call,
                &started_at,
                ToolResult::error_typed(
                    format!(
                        "Duplicate tool call detected: {} was called with identical arguments recently. \
                         If you are stuck, try a different approach, search for more information, or summarize your progress.",
                        tool_call.name
                    ),
                    ToolErrorType::Validation,
                    true,
                    Some("Do NOT resend identical tool parameters. Re-anchor with a lightweight read/list action, then adjust arguments.".to_string()),
                ),
            ));
        }
        self.recent_tool_calls.push(call_sig);
        // Keep only last 10 calls to avoid unbounded growth
        if self.recent_tool_calls.len() > 10 {
            self.recent_tool_calls.remove(0);
        }

        // Execute the tool with timing
        self.cost_tracker.record_tool_call();
        debug!(
            "Tool execute start: tool={} args={}",
            tool_call.name, tool_call.arguments
        );
        let start_time = std::time::Instant::now();

        let (p_tx, mut p_rx) = mpsc::unbounded_channel::<yode_tools::tool::ToolProgress>();
        let event_tx_inner = event_tx.clone();
        let tc_id = tool_call.id.clone();
        let tc_name = tool_call.name.clone();
        let progress_counter = Arc::new(AtomicU64::new(0));
        let progress_counter_inner = Arc::clone(&progress_counter);
        let last_progress_message = Arc::new(std::sync::Mutex::new(None::<String>));
        let last_progress_message_inner = Arc::clone(&last_progress_message);
        tokio::spawn(async move {
            while let Some(progress) = p_rx.recv().await {
                progress_counter_inner.fetch_add(1, Ordering::Relaxed);
                if let Ok(mut slot) = last_progress_message_inner.lock() {
                    *slot = Some(progress.message.clone());
                }
                let _ = event_tx_inner.send(EngineEvent::ToolProgress {
                    id: tc_id.clone(),
                    name: tc_name.clone(),
                    progress,
                });
            }
        });

        let ctx = self.build_tool_context(Some(p_tx)).await;

        // Validate and coerce parameters against the tool's schema after hooks
        // so updatedInput/modified_input can take effect.
        let schema = tool.parameters_schema();
        if let Err(msg) = validation::validate_and_coerce(&schema, &mut params) {
            return Ok(Self::immediate_tool_outcome(
                tool_call,
                &started_at,
                ToolResult::error_typed(
                    format!("Parameter validation failed: {}", msg),
                    ToolErrorType::Validation,
                    true,
                    Some(format!("Fix the parameters and retry. Schema: {}", schema)),
                ),
            ));
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

        // Append recovery suggestion to content so LLM can see it
        if result.is_error {
            // Add contextual recovery hints based on error type
            let auto_hint = match result.error_type {
                Some(ToolErrorType::NotFound) => {
                    Some("Try using `glob` to find the correct path, or `grep` to search for the symbol by name.".to_string())
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
                result
                    .content
                    .push_str(&format!("\n\nSuggestion: {}", suggestion));
            } else if let Some(hint) = auto_hint {
                result
                    .content
                    .push_str(&format!("\n\nSuggestion: {}", hint));
            }
        }

        let progress_updates = progress_counter.load(Ordering::Relaxed) as u32;
        let last_progress_message = last_progress_message
            .lock()
            .ok()
            .and_then(|slot| slot.clone());
        self.record_tool_progress_summary(
            &tool_call.name,
            progress_updates,
            last_progress_message.clone(),
        );

        Ok(ToolExecutionOutcome {
            tool_call: tool_call.clone(),
            result,
            started_at,
            duration_ms: elapsed.as_millis() as u64,
            progress_updates,
            last_progress_message,
            parallel_batch: None,
        })
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
        match tokio::time::timeout(std::time::Duration::from_secs(5), provider.chat(request)).await
        {
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
        fn name(&self) -> &str {
            "mock"
        }
        async fn chat(
            &self,
            _req: yode_llm::types::ChatRequest,
        ) -> anyhow::Result<yode_llm::types::ChatResponse> {
            unimplemented!("Mock provider should not be called in unit tests")
        }
        async fn chat_stream(
            &self,
            _req: yode_llm::types::ChatRequest,
            _tx: tokio::sync::mpsc::Sender<yode_llm::types::StreamEvent>,
        ) -> anyhow::Result<()> {
            unimplemented!()
        }
        async fn list_models(&self) -> anyhow::Result<Vec<yode_llm::ModelInfo>> {
            Ok(vec![])
        }
    }

    /// A mock read-only tool for testing parallel execution.
    struct MockReadTool {
        name: String,
    }

    #[async_trait::async_trait]
    impl Tool for MockReadTool {
        fn name(&self) -> &str {
            &self.name
        }
        fn description(&self) -> &str {
            "mock read tool"
        }
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
        async fn execute(
            &self,
            _params: serde_json::Value,
            _ctx: &ToolContext,
        ) -> anyhow::Result<ToolResult> {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            Ok(ToolResult::success(format!("result from {}", self.name)))
        }
    }

    /// A mock write tool that requires confirmation.
    struct MockWriteTool;

    #[async_trait::async_trait]
    impl Tool for MockWriteTool {
        fn name(&self) -> &str {
            "mock_write"
        }
        fn description(&self) -> &str {
            "mock write tool"
        }
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
        async fn execute(
            &self,
            _params: serde_json::Value,
            _ctx: &ToolContext,
        ) -> anyhow::Result<ToolResult> {
            Ok(ToolResult::success("write done".to_string()))
        }
    }

    struct MockPathTool;

    #[async_trait::async_trait]
    impl Tool for MockPathTool {
        fn name(&self) -> &str {
            "mock_path"
        }
        fn description(&self) -> &str {
            "mock path tool"
        }
        fn parameters_schema(&self) -> serde_json::Value {
            serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" }
                },
                "required": ["path"]
            })
        }
        fn capabilities(&self) -> ToolCapabilities {
            ToolCapabilities {
                requires_confirmation: false,
                supports_auto_execution: true,
                read_only: false,
            }
        }
        async fn execute(
            &self,
            params: serde_json::Value,
            _ctx: &ToolContext,
        ) -> anyhow::Result<ToolResult> {
            let path = params
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or("missing");
            Ok(ToolResult::success(format!("path={}", path)))
        }
    }

    fn make_engine(tools: Vec<Arc<dyn Tool>>, confirm_tools: Vec<String>) -> AgentEngine {
        let mut registry = ToolRegistry::new();
        for t in tools {
            registry.register(t);
        }
        let provider: Arc<dyn yode_llm::provider::LlmProvider> = Arc::new(MockProvider);
        let permissions = PermissionManager::from_confirmation_list(confirm_tools);
        let workdir =
            std::env::temp_dir().join(format!("yode-engine-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&workdir).unwrap();
        let context = AgentContext::new(workdir, "mock".to_string(), "claude-sonnet-4".to_string());
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
            ToolCall {
                id: "1".into(),
                name: "r1".into(),
                arguments: "{}".into(),
            },
            ToolCall {
                id: "2".into(),
                name: "r2".into(),
                arguments: "{}".into(),
            },
            ToolCall {
                id: "3".into(),
                name: "r3".into(),
                arguments: "{}".into(),
            },
        ];
        let (par, seq) = engine.partition_tool_calls(&tcs);
        assert_eq!(par.len(), 3);
        assert_eq!(seq.len(), 0);
    }

    #[test]
    fn test_partition_mixed() {
        let engine = make_engine(
            vec![
                Arc::new(MockReadTool {
                    name: "reader".into(),
                }),
                Arc::new(MockWriteTool),
            ],
            vec!["mock_write".into()],
        );
        let tcs = vec![
            ToolCall {
                id: "1".into(),
                name: "reader".into(),
                arguments: "{}".into(),
            },
            ToolCall {
                id: "2".into(),
                name: "mock_write".into(),
                arguments: "{}".into(),
            },
            ToolCall {
                id: "3".into(),
                name: "reader".into(),
                arguments: "{}".into(),
            },
        ];
        let (par, seq) = engine.partition_tool_calls(&tcs);
        assert_eq!(par.len(), 2);
        assert_eq!(seq.len(), 1);
        assert_eq!(seq[0].name, "mock_write");
    }

    #[test]
    fn test_partition_unknown_tool() {
        let engine = make_engine(vec![], vec![]);
        let tcs = vec![ToolCall {
            id: "1".into(),
            name: "nonexistent".into(),
            arguments: "{}".into(),
        }];
        let (par, seq) = engine.partition_tool_calls(&tcs);
        assert_eq!(par.len(), 0);
        assert_eq!(seq.len(), 1);
    }

    #[test]
    fn test_partition_read_only_needing_confirm() {
        let engine = make_engine(
            vec![Arc::new(MockReadTool {
                name: "sensitive".into(),
            })],
            vec!["sensitive".into()],
        );
        let tcs = vec![ToolCall {
            id: "1".into(),
            name: "sensitive".into(),
            arguments: "{}".into(),
        }];
        let (par, seq) = engine.partition_tool_calls(&tcs);
        assert_eq!(
            par.len(),
            0,
            "Confirm-required tools must not be parallelized"
        );
        assert_eq!(seq.len(), 1);
    }

    // --- execute_tools_parallel tests ---

    #[tokio::test]
    async fn test_parallel_returns_all_results_in_order() {
        let mut engine = make_engine(
            vec![
                Arc::new(MockReadTool { name: "a".into() }),
                Arc::new(MockReadTool { name: "b".into() }),
                Arc::new(MockReadTool { name: "c".into() }),
            ],
            vec![],
        );
        let tcs = vec![
            ToolCall {
                id: "x1".into(),
                name: "a".into(),
                arguments: "{}".into(),
            },
            ToolCall {
                id: "x2".into(),
                name: "b".into(),
                arguments: "{}".into(),
            },
            ToolCall {
                id: "x3".into(),
                name: "c".into(),
                arguments: "{}".into(),
            },
        ];
        let (tx, mut rx) = mpsc::unbounded_channel();
        let results = engine.execute_tools_parallel(&tcs, &tx).await;

        assert_eq!(results.len(), 3);
        assert_eq!(results[0].tool_call.id, "x1");
        assert_eq!(results[1].tool_call.id, "x2");
        assert_eq!(results[2].tool_call.id, "x3");
        for outcome in &results {
            assert!(!outcome.result.is_error);
        }

        // Check events
        let mut starts = 0;
        while let Ok(ev) = rx.try_recv() {
            if matches!(ev, EngineEvent::ToolCallStart { .. }) {
                starts += 1;
            }
        }
        assert_eq!(starts, 3);
    }

    #[tokio::test]
    async fn test_parallel_empty() {
        let mut engine = make_engine(vec![], vec![]);
        let (tx, _rx) = mpsc::unbounded_channel();
        let results = engine.execute_tools_parallel(&[], &tx).await;
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_autocompact_circuit_breaker_trips_after_repeated_failures() {
        let mut engine = make_engine(vec![], vec![]);
        engine.messages = vec![
            Message::system("system"),
            Message::user("u1"),
            Message::assistant("a1"),
            Message::user("u2"),
            Message::assistant("a2"),
            Message::user("u3"),
            Message::assistant("a3"),
        ];
        engine.current_query_source = QuerySource::User;

        let (tx, mut rx) = mpsc::unbounded_channel();
        for _ in 0..MAX_CONSECUTIVE_COMPACTION_FAILURES {
            engine.maybe_compact_context(160_000, &tx).await;
        }

        assert!(engine.autocompact_disabled);
        assert_eq!(
            engine.compaction_failures,
            MAX_CONSECUTIVE_COMPACTION_FAILURES
        );
        assert!(engine.messages.iter().any(|msg| {
            msg.content
                .as_deref()
                .unwrap_or_default()
                .contains("Auto-compact disabled")
        }));
        let runtime = engine.runtime_state();
        assert_eq!(
            runtime.last_compaction_breaker_reason.as_deref(),
            Some("compression made no changes")
        );
        assert!(matches!(rx.try_recv(), Ok(EngineEvent::Error(_))));
    }

    #[test]
    fn test_recovery_artifact_written_on_state_transition() {
        let mut engine = make_engine(vec![], vec![]);
        engine.last_failed_signature = Some("bash:{\"command\":\"cargo test\"}".to_string());
        engine.error_buckets.insert(ToolErrorType::Validation, 2);
        engine.update_recovery_state();

        assert_eq!(engine.recovery_state, RecoveryState::SingleStepMode);
        let artifact = engine
            .last_recovery_artifact_path
            .as_ref()
            .expect("recovery artifact should exist");
        let content = std::fs::read_to_string(artifact).unwrap();
        assert!(content.contains("SingleStepMode"));
        assert!(content.contains("Breadcrumbs"));
    }

    #[tokio::test]
    async fn test_compact_query_source_skips_autocompact() {
        let mut engine = make_engine(vec![], vec![]);
        let big = "x".repeat(18_000);
        engine.messages = vec![
            Message::system("system"),
            Message::user(&big),
            Message::assistant(&big),
            Message::tool_result("tc1", &big),
            Message::user(&big),
            Message::assistant(&big),
            Message::user("recent1"),
            Message::assistant("recent2"),
            Message::user("recent3"),
            Message::assistant("recent4"),
            Message::user("recent5"),
            Message::assistant("recent6"),
        ];
        engine.current_query_source = QuerySource::Compact;

        let before_len = engine.messages.len();
        let (tx, _rx) = mpsc::unbounded_channel();
        engine.maybe_compact_context(160_000, &tx).await;

        assert_eq!(engine.messages.len(), before_len);
        assert_eq!(engine.compaction_failures, 0);
        assert!(!engine.messages.iter().any(|msg| {
            msg.content
                .as_deref()
                .unwrap_or_default()
                .starts_with("[Context summary]")
        }));
    }

    #[tokio::test]
    async fn test_force_compact_ignores_auto_compact_guard() {
        let mut engine = make_engine(vec![], vec![]);
        let big = "x".repeat(18_000);
        engine.messages = vec![
            Message::system("system"),
            Message::user(&big),
            Message::assistant(&big),
            Message::tool_result("tc1", &big),
            Message::user(&big),
            Message::assistant(&big),
            Message::user("recent1"),
            Message::assistant("recent2"),
            Message::user("recent3"),
            Message::assistant("recent4"),
            Message::user("recent5"),
            Message::assistant("recent6"),
        ];
        engine.current_query_source = QuerySource::Compact;
        engine.autocompact_disabled = true;

        let (tx, _rx) = mpsc::unbounded_channel();
        let changed = engine.force_compact(tx).await;

        assert!(changed);
        let runtime = engine.runtime_state();
        assert_eq!(runtime.total_compactions, 1);
        assert_eq!(runtime.manual_compactions, 1);
        assert!(
            engine.messages.len() < 12
                || engine.messages.iter().any(|msg| {
                    msg.content
                        .as_deref()
                        .unwrap_or_default()
                        .contains("[compressed]")
                })
        );
    }

    #[tokio::test]
    async fn test_initialize_session_hooks_injects_system_context() {
        let mut engine = make_engine(vec![], vec![]);
        let hook_dir =
            std::env::temp_dir().join(format!("yode-session-hook-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&hook_dir).unwrap();
        let mut hook_mgr = crate::hooks::HookManager::new(hook_dir);
        hook_mgr.register(crate::hooks::HookDefinition {
            command: "echo session context".into(),
            events: vec!["session_start".into()],
            tool_filter: None,
            timeout_secs: 5,
            can_block: false,
        });
        engine.set_hook_manager(hook_mgr);

        engine.initialize_session_hooks("startup").await;

        assert!(engine.messages.iter().any(|msg| {
            msg.content
                .as_deref()
                .unwrap_or_default()
                .contains("session_start hooks")
        }));
        assert!(engine.messages.iter().any(|msg| {
            msg.content
                .as_deref()
                .unwrap_or_default()
                .contains("session context")
        }));
    }

    #[tokio::test]
    async fn test_session_start_hook_wake_notification_is_injected() {
        let mut engine = make_engine(vec![], vec![]);
        let hook_dir = std::env::temp_dir().join(format!(
            "yode-session-hook-wake-test-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&hook_dir).unwrap();
        let mut hook_mgr = crate::hooks::HookManager::new(hook_dir);
        hook_mgr.register(crate::hooks::HookDefinition {
            command:
                "printf '%s' '{\"hookSpecificOutput\":{\"wakeNotification\":\"background hook finished\"}}' && exit 2"
                    .into(),
            events: vec!["session_start".into()],
            tool_filter: None,
            timeout_secs: 5,
            can_block: false,
        });
        engine.set_hook_manager(hook_mgr);

        engine.initialize_session_hooks("startup").await;

        assert!(engine.messages.iter().any(|msg| {
            msg.content
                .as_deref()
                .unwrap_or_default()
                .contains("[Hook Wake via session_start")
        }));
        assert!(engine.messages.iter().any(|msg| {
            msg.content
                .as_deref()
                .unwrap_or_default()
                .contains("background hook finished")
        }));
    }

    #[tokio::test]
    async fn test_pre_compact_hook_context_includes_runtime_metadata() {
        let mut engine = make_engine(vec![], vec![]);
        let hook_dir =
            std::env::temp_dir().join(format!("yode-compact-hook-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&hook_dir).unwrap();
        let dump_path = hook_dir.join("pre-compact-context.json");
        let mut hook_mgr = crate::hooks::HookManager::new(hook_dir.clone());
        hook_mgr.register(crate::hooks::HookDefinition {
            command: format!(
                "printf '%s' \"$YODE_HOOK_CONTEXT\" > {}",
                dump_path.display()
            ),
            events: vec!["pre_compact".into()],
            tool_filter: None,
            timeout_secs: 5,
            can_block: false,
        });
        engine.set_hook_manager(hook_mgr);
        let big = "x".repeat(18_000);
        engine.messages = vec![
            Message::system("system"),
            Message::user(&big),
            Message::assistant(&big),
            Message::tool_result("tc1", &big),
            Message::user(&big),
            Message::assistant(&big),
            Message::user("recent1"),
            Message::assistant("recent2"),
            Message::user("recent3"),
            Message::assistant("recent4"),
            Message::user("recent5"),
            Message::assistant("recent6"),
        ];
        engine.recovery_state = RecoveryState::SingleStepMode;
        engine.recovery_single_step_count = 2;
        engine.last_failed_signature = Some("bash:{\"command\":\"cargo test\"}".to_string());

        let (tx, _rx) = mpsc::unbounded_channel();
        let _ = engine.force_compact(tx).await;

        let value: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dump_path).unwrap()).unwrap();
        let runtime = value
            .get("metadata")
            .and_then(|v| v.get("runtime"))
            .and_then(|v| v.as_object())
            .unwrap();
        assert!(runtime.contains_key("total_compactions"));
        assert!(runtime.contains_key("live_session_memory_initialized"));
        assert!(runtime.contains_key("session_memory_update_count"));
        assert_eq!(
            runtime.get("recovery_state").and_then(|v| v.as_str()),
            Some("SingleStepMode")
        );
        assert_eq!(
            runtime
                .get("last_failed_signature")
                .and_then(|v| v.as_str()),
            Some("bash:{\"command\":\"cargo test\"}")
        );
    }

    #[tokio::test]
    async fn test_append_hook_outputs_as_system_message_injects_context() {
        let mut engine = make_engine(vec![], vec![]);
        let hook_dir = std::env::temp_dir().join(format!(
            "yode-user-prompt-hook-test-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&hook_dir).unwrap();
        let mut hook_mgr = crate::hooks::HookManager::new(hook_dir);
        hook_mgr.register(crate::hooks::HookDefinition {
            command: "echo prompt context".into(),
            events: vec!["user_prompt_submit".into()],
            tool_filter: None,
            timeout_secs: 5,
            can_block: false,
        });
        engine.set_hook_manager(hook_mgr);

        let ctx = HookContext {
            event: HookEvent::UserPromptSubmit.to_string(),
            session_id: engine.context().session_id.clone(),
            working_dir: engine.context().working_dir_compat().display().to_string(),
            tool_name: None,
            tool_input: None,
            tool_output: None,
            error: None,
            user_prompt: Some("hello".to_string()),
            metadata: None,
        };
        engine
            .append_hook_outputs_as_system_message(
                HookEvent::UserPromptSubmit,
                ctx,
                "System Auto-Context via user_prompt_submit hooks",
            )
            .await;

        assert!(engine.messages.iter().any(|msg| {
            msg.content
                .as_deref()
                .unwrap_or_default()
                .contains("prompt context")
        }));
    }

    #[tokio::test]
    async fn test_pre_tool_use_hook_can_modify_input() {
        let mut engine = make_engine(vec![Arc::new(MockPathTool)], vec![]);
        let hook_dir =
            std::env::temp_dir().join(format!("yode-modify-hook-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&hook_dir).unwrap();
        let mut hook_mgr = crate::hooks::HookManager::new(hook_dir);
        hook_mgr.register(crate::hooks::HookDefinition {
            command: "printf '%s' '{\"updatedInput\":{\"path\":\"new.txt\"}}'".into(),
            events: vec!["pre_tool_use".into()],
            tool_filter: Some(vec!["mock_path".into()]),
            timeout_secs: 5,
            can_block: false,
        });
        engine.set_hook_manager(hook_mgr);

        let tool_call = ToolCall {
            id: "tc1".into(),
            name: "mock_path".into(),
            arguments: "{\"path\":\"old.txt\"}".into(),
        };
        let (event_tx, _event_rx) = mpsc::unbounded_channel();
        let (_confirm_tx, mut confirm_rx) = mpsc::unbounded_channel();

        let result = engine
            .handle_tool_call(&tool_call, &event_tx, &mut confirm_rx, None)
            .await
            .unwrap();

        assert_eq!(result.result.content, "path=new.txt");
    }

    #[tokio::test]
    async fn test_permission_hook_metadata_uses_effective_input_snapshot() {
        let mut engine = make_engine(vec![Arc::new(MockPathTool)], vec!["mock_path".into()]);
        let hook_dir = std::env::temp_dir().join(format!(
            "yode-permission-hook-metadata-test-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&hook_dir).unwrap();
        let dump_path = hook_dir.join("permission-context.json");
        let mut hook_mgr = crate::hooks::HookManager::new(hook_dir.clone());
        hook_mgr.register(crate::hooks::HookDefinition {
            command: format!(
                "printf '%s' \"$YODE_HOOK_CONTEXT\" > {}",
                dump_path.display()
            ),
            events: vec!["permission_request".into()],
            tool_filter: Some(vec!["mock_path".into()]),
            timeout_secs: 5,
            can_block: false,
        });
        engine.set_hook_manager(hook_mgr);

        let tool_call = ToolCall {
            id: "tc1".into(),
            name: "mock_path".into(),
            arguments: "{\"path\":\"old.txt\"}".into(),
        };
        let (event_tx, _event_rx) = mpsc::unbounded_channel();
        let (confirm_tx, mut confirm_rx) = mpsc::unbounded_channel();
        confirm_tx.send(ConfirmResponse::Allow).unwrap();

        let _ = engine
            .handle_tool_call(&tool_call, &event_tx, &mut confirm_rx, None)
            .await
            .unwrap();

        let value: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dump_path).unwrap()).unwrap();
        let metadata = value.get("metadata").and_then(|v| v.as_object()).unwrap();
        assert_eq!(
            metadata
                .get("effective_input_snapshot")
                .and_then(|v| v.get("path"))
                .and_then(|v| v.as_str()),
            Some("old.txt")
        );
        assert_eq!(
            metadata
                .get("original_input_snapshot")
                .and_then(|v| v.get("path"))
                .and_then(|v| v.as_str()),
            Some("old.txt")
        );
        assert_eq!(
            metadata
                .get("input_changed_by_hook")
                .and_then(|v| v.as_bool()),
            Some(false)
        );
    }

    #[tokio::test]
    async fn test_session_end_hook_context_includes_runtime_metadata() {
        let mut engine = make_engine(vec![], vec![]);
        let hook_dir = std::env::temp_dir().join(format!(
            "yode-session-end-hook-test-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&hook_dir).unwrap();
        let dump_path = hook_dir.join("session-end-context.json");
        let mut hook_mgr = crate::hooks::HookManager::new(hook_dir.clone());
        hook_mgr.register(crate::hooks::HookDefinition {
            command: format!(
                "printf '%s' \"$YODE_HOOK_CONTEXT\" > {}",
                dump_path.display()
            ),
            events: vec!["session_end".into()],
            tool_filter: None,
            timeout_secs: 5,
            can_block: false,
        });
        engine.set_hook_manager(hook_mgr);
        engine.messages = vec![
            Message::system("system"),
            Message::user("hello"),
            Message::assistant("world"),
        ];

        engine.finalize_session_hooks("shutdown").await;

        let value: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dump_path).unwrap()).unwrap();
        let metadata = value.get("metadata").unwrap();
        let runtime = metadata.get("runtime").and_then(|v| v.as_object()).unwrap();
        assert_eq!(
            metadata.get("reason").and_then(|v| v.as_str()),
            Some("shutdown")
        );
        assert!(runtime.contains_key("live_session_memory_path"));
        assert!(runtime.contains_key("tracked_failed_tool_results"));
        let memory_flush = metadata
            .get("memory_flush")
            .and_then(|v| v.as_object())
            .unwrap();
        assert!(memory_flush.contains_key("path"));
        assert!(memory_flush.contains_key("update_count"));
    }

    #[test]
    fn test_live_session_memory_refresh_writes_snapshot() {
        let mut engine = make_engine(vec![], vec![]);
        let project_root = engine.context().working_dir_compat();
        let big = "x".repeat(9_000);
        engine.messages = vec![
            Message::system("system"),
            Message::user(format!("Need to debug resume flow {}", big)),
            Message::assistant("I traced the issue to persisted message snapshots."),
        ];
        engine.tool_call_count = 3;
        engine
            .files_modified
            .push(project_root.join("src/main.rs").display().to_string());

        engine.maybe_refresh_live_session_memory(None);

        let live_path = crate::session_memory::live_session_memory_path(&project_root);
        let content = std::fs::read_to_string(live_path).unwrap();
        assert!(content.contains("Session Snapshot"));
        assert!(content.contains("persisted message snapshots"));
        assert!(engine.session_memory_initialized);
        assert!(engine.last_session_memory_tool_count >= 3);
        let runtime = engine.runtime_state();
        assert_eq!(runtime.session_memory_update_count, 1);
    }

    #[test]
    fn test_record_response_usage_tracks_prompt_cache_telemetry() {
        let mut engine = make_engine(vec![], vec![]);
        let (tx, _rx) = mpsc::unbounded_channel();

        engine.reset_prompt_cache_turn_runtime();
        engine.record_response_usage(
            &yode_llm::types::Usage {
                prompt_tokens: 1_200,
                completion_tokens: 180,
                total_tokens: 1_380,
                cache_write_tokens: 300,
                cache_read_tokens: 200,
            },
            &tx,
        );

        let usage = engine.cost_tracker().usage().clone();
        assert_eq!(usage.input_tokens, 700);
        assert_eq!(usage.output_tokens, 180);
        assert_eq!(usage.cache_write_tokens, 300);
        assert_eq!(usage.cache_read_tokens, 200);

        let runtime = engine.runtime_state();
        assert_eq!(runtime.prompt_cache.last_turn_prompt_tokens, Some(1_200));
        assert_eq!(runtime.prompt_cache.last_turn_completion_tokens, Some(180));
        assert_eq!(runtime.prompt_cache.last_turn_cache_write_tokens, Some(300));
        assert_eq!(runtime.prompt_cache.last_turn_cache_read_tokens, Some(200));
        assert_eq!(runtime.prompt_cache.reported_turns, 1);
        assert_eq!(runtime.prompt_cache.cache_write_turns, 1);
        assert_eq!(runtime.prompt_cache.cache_read_turns, 1);
        assert_eq!(runtime.prompt_cache.cache_write_tokens_total, 300);
        assert_eq!(runtime.prompt_cache.cache_read_tokens_total, 200);
    }

    #[test]
    fn test_system_prompt_runtime_state_tracks_segment_breakdown() {
        let engine = make_engine(vec![], vec![]);
        let runtime = engine.runtime_state();

        assert!(runtime.system_prompt_estimated_tokens > 0);
        assert!(runtime.system_prompt_segments.len() >= 2);
        assert!(runtime
            .system_prompt_segments
            .iter()
            .any(|segment| segment.label == "Base prompt"));
        assert!(runtime
            .system_prompt_segments
            .iter()
            .any(|segment| segment.label == "Environment"));
    }

    #[test]
    fn test_compaction_cause_histogram_tracks_counts() {
        let mut engine = make_engine(vec![], vec![]);

        engine.record_compaction_cause("skipped_below_threshold");
        engine.record_compaction_cause("skipped_below_threshold");
        engine.record_compaction_cause("success_manual");

        let runtime = engine.runtime_state();
        assert_eq!(
            runtime.compaction_cause_histogram.get("skipped_below_threshold"),
            Some(&2)
        );
        assert_eq!(
            runtime.compaction_cause_histogram.get("success_manual"),
            Some(&1)
        );
    }

    #[tokio::test]
    async fn test_tool_runtime_state_and_artifact_are_recorded() {
        let mut engine = make_engine(vec![], vec![]);
        engine.reset_tool_turn_runtime();
        engine.record_tool_progress_summary("write_file", 2, Some("writing".to_string()));
        let batch_id = engine.register_parallel_batch(2);

        let tool_call = ToolCall {
            id: "tc-tool-runtime".into(),
            name: "write_file".into(),
            arguments: r#"{"file_path":"src/lib.rs","content":"fn main() {}\n"}"#.into(),
        };
        let raw_result = ToolResult::success_with_metadata(
            "Successfully wrote 12 bytes".to_string(),
            serde_json::json!({
                "file_path": "src/lib.rs",
                "line_count": 1,
                "diff_preview": {
                    "removed": [],
                    "added": ["fn main() {}"],
                    "more_removed": 0,
                    "more_added": 0
                }
            }),
        );

        let _final = engine
            .finalize_tool_result(
                &tool_call,
                raw_result,
                Some("2026-04-09 10:00:00".to_string()),
                42,
                2,
                Some(batch_id),
            )
            .await;

        let runtime = engine.runtime_state();
        assert_eq!(runtime.current_turn_tool_calls, 1);
        assert_eq!(runtime.current_turn_tool_progress_events, 2);
        assert_eq!(runtime.current_turn_parallel_batches, 1);
        assert_eq!(runtime.tool_traces.len(), 1);
        assert_eq!(runtime.tool_traces[0].tool_name, "write_file");
        assert!(runtime.tool_traces[0].diff_preview.is_some());

        engine.complete_tool_turn_artifact();
        let runtime = engine.runtime_state();
        assert!(runtime.last_tool_turn_artifact_path.is_some());
        let path = runtime.last_tool_turn_artifact_path.unwrap();
        assert!(std::path::Path::new(&path).exists());
    }

    #[test]
    fn test_session_end_flush_writes_snapshot_without_threshold() {
        let mut engine = make_engine(vec![], vec![]);
        let project_root = engine.context().working_dir_compat();
        engine.messages = vec![
            Message::system("system"),
            Message::user("Short session"),
            Message::assistant("But still worth persisting on exit."),
        ];

        engine.flush_live_session_memory_on_shutdown();

        let live_path = crate::session_memory::live_session_memory_path(&project_root);
        let content = std::fs::read_to_string(live_path).unwrap();
        assert!(content.contains("Session Snapshot"));
        assert!(content.contains("Short session"));
    }

    #[test]
    fn test_restore_messages_rebuilds_artifact_runtime_state() {
        let mut engine = make_engine(vec![], vec![]);
        let project_root = engine.context().working_dir_compat();
        let transcript_dir = project_root.join(".yode").join("transcripts");
        std::fs::create_dir_all(&transcript_dir).unwrap();
        let transcript_path = transcript_dir.join("abc12345-compact-20260101-100000.md");
        std::fs::write(
            &transcript_path,
            "# Compaction Transcript\n\n- Session: abc\n- Mode: manual\n- Timestamp: 2026-01-01 10:00:00\n- Removed messages: 7\n- Tool results truncated: 2\n- Failed tool results: 1\n- Session memory path: .yode/memory/session.md\n\n## Summary Anchor\n\n```text\nRecovered summary\n```\n",
        )
        .unwrap();

        let live_path = crate::session_memory::live_session_memory_path(&project_root);
        std::fs::create_dir_all(live_path.parent().unwrap()).unwrap();
        std::fs::write(&live_path, "# Session Snapshot\n\nplaceholder").unwrap();

        engine.restore_messages(vec![Message::user("resume")]);
        let runtime = engine.runtime_state();
        assert_eq!(runtime.last_compaction_mode.as_deref(), Some("manual"));
        assert_eq!(
            runtime.last_compaction_at.as_deref(),
            Some("2026-01-01 10:00:00")
        );
        assert_eq!(
            runtime.last_compaction_summary_excerpt.as_deref(),
            Some("Recovered summary")
        );
        let transcript_path_str = transcript_path.display().to_string();
        assert_eq!(
            runtime.last_compaction_transcript_path.as_deref(),
            Some(transcript_path_str.as_str())
        );
        let live_path_str = live_path.display().to_string();
        assert_eq!(
            runtime.last_session_memory_update_path.as_deref(),
            Some(live_path_str.as_str())
        );
    }
}
