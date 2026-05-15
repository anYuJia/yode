mod bootstrap_runtime;
mod compaction_runtime;
mod hooks_runtime;
mod intelligence_runtime;
mod llm_runtime;
mod nonstream_turn_runtime;
mod recovery_runtime;
mod request_runtime;
mod retry;
mod runtime_support;
mod session_state;
mod stream_retry_runtime;
mod streaming_turn_runtime;
mod subagent_runner;
mod system_prompt_runtime;
mod tool_execution_runtime;
mod tool_pool_runtime;
mod tool_result;
#[path = "tool_telemetry/mod.rs"]
mod tool_telemetry;
mod turn_output_runtime;
mod turn_setup_runtime;
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
use tokio::sync::{mpsc, Mutex};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

use yode_agent::AgentTeamManager;
use yode_llm::provider::LlmProvider;
use yode_llm::types::{ChatRequest, ChatResponse, Message, Role, StreamEvent, ToolCall};
use yode_tools::registry::ToolRegistry;
use yode_tools::runtime_tasks::{RuntimeTask, RuntimeTaskNotification, RuntimeTaskStore};
use yode_tools::state::TaskStore;
use yode_tools::tool::{
    McpResourceProvider, ToolContext, ToolErrorType, ToolResult, UserQuery, WorktreeState,
};
use yode_tools::validation;

use crate::constants::thresholds::{
    MAX_CONSECUTIVE_COMPACTION_FAILURES,
    SESSION_MEMORY_CHAR_DELTA as SESSION_MEMORY_CHAR_DELTA_THRESHOLD,
    SESSION_MEMORY_INIT_CHARS as SESSION_MEMORY_INIT_CHAR_THRESHOLD,
    SESSION_MEMORY_TOOL_DELTA as SESSION_MEMORY_TOOL_DELTA_THRESHOLD, TOOL_BUDGET_NOTICE,
    TOOL_BUDGET_WARNING, TOOL_GOAL_REMINDER, TOOL_REFLECT_INTERVAL,
};
use crate::constants::timeouts::{
    LLM_REQUEST_SECS as LLM_TIMEOUT_SECS, PARALLEL_TOOL_SECS as PARALLEL_TOOL_TIMEOUT_SECS,
    STREAMING_HEARTBEAT_SECS, STREAMING_STALL_SECS, STREAMING_TURN_HARD_SECS,
    TOOL_EXECUTION_SECS as TOOL_EXECUTION_TIMEOUT_SECS,
};
use crate::context::{AgentContext, EffortLevel, QuerySource};
use crate::context_manager::{CompressionReport, ContextManager};
use crate::cost_tracker::CostTracker;
use crate::db::{Database, SessionArtifacts};
use crate::error::EngineError;
use crate::hooks::{HookContext, HookEvent, HookManager};
use crate::instructions::{load_instruction_context, load_memory_context};
use crate::permission::{CommandClassifier, CommandRiskLevel, PermissionAction, PermissionManager};
use crate::session_memory::{
    best_compaction_memory_excerpt, build_live_snapshot, clear_live_session_memory,
    live_session_memory_path, persist_compaction_memory, persist_live_session_memory,
    persist_live_session_memory_summary, render_live_session_memory_prompt,
};
use crate::transcript::write_compaction_transcript;
use retry::{
    classify_error, max_retries_for, retry_delay, summarize_retry_error_message,
    total_attempts_for, ErrorKind,
};
use subagent_runner::SubAgentRunnerImpl;
use tool_result::{
    annotate_tool_result_runtime_metadata, convert_tool_definitions,
    dynamic_total_tool_results_limit, set_tool_runtime_truncation_metadata, truncate_tool_result,
};
use types::{
    latest_transcript_runtime_state, CostRuntimeState, ProjectKind, RecoveryState,
    SharedMemoryStatus, SystemPromptBuild, ToolExecutionOutcome, ToolExecutionTrace,
};
pub use types::{
    CompactBoundaryRuntimeState, ConfirmResponse, ContextCollapseRuntimeState, EngineEvent,
    EngineRuntimeState, PlanRuntimeState, PromptCacheRuntimeState, RestoreBudgetEntryRuntimeState,
    RestoreBudgetRuntimeState, SystemPromptSegmentRuntimeState,
};

/// The core agent engine that drives the conversation loop.
pub struct AgentEngine {
    provider: Arc<dyn LlmProvider>,
    tools: Arc<ToolRegistry>,
    permissions: PermissionManager,
    context: AgentContext,
    messages: Vec<Message>,
    system_prompt: String,
    db: Option<Database>,
    /// Shared task store for the todo tool.
    task_store: Arc<Mutex<TaskStore>>,
    /// Shared runtime task store for background bash/sub-agent work.
    runtime_task_store: Arc<Mutex<RuntimeTaskStore>>,
    /// Shared live multi-agent/team runtime manager.
    team_runtime_manager: Arc<Mutex<AgentTeamManager>>,
    /// Shared worktree state for enter/exit worktree tools.
    worktree_state: Arc<Mutex<WorktreeState>>,
    /// Channel for ask_user questions (engine → TUI).
    ask_user_tx: Option<mpsc::UnboundedSender<UserQuery>>,
    /// Channel for ask_user answers (TUI → engine).
    ask_user_rx: Option<Arc<Mutex<mpsc::UnboundedReceiver<String>>>>,
    /// Shared provider for MCP resource list/read tools.
    mcp_resource_provider: Option<Arc<dyn McpResourceProvider>>,
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
    hook_manager: Option<Arc<HookManager>>,
    /// Files the agent has already read in this turn (path → line count).
    files_read: std::collections::HashMap<String, usize>,
    /// Read-file paths in last-access order for post-compact restoration.
    recent_file_reads: Vec<String>,
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
    /// Start time for the current top-level user turn.
    current_turn_started_at: Option<std::time::Instant>,
    /// Duration of the most recently completed top-level turn.
    last_turn_duration_ms: Option<u64>,
    /// Stop reason of the most recently completed top-level turn.
    last_turn_stop_reason: Option<String>,
    /// Artifact path for the most recently completed top-level turn.
    last_turn_artifact_path: Option<String>,
    /// Last stream watchdog stage label emitted by the receive loop.
    last_stream_watchdog_stage: Option<String>,
    /// Retry reasons seen across streaming retries.
    stream_retry_reason_histogram: BTreeMap<String, u32>,
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
    /// Stop reactive compaction from spiraling within a single top-level turn.
    reactive_compact_attempted: bool,
    /// Stop media-strip retries from looping within a single top-level turn.
    reactive_media_strip_attempted: bool,
    /// Stop hooks may ask the agent loop to continue once before finalizing a turn.
    stop_hook_continue_attempted: bool,
    /// Number of times a stop hook requested another agent step in this session.
    stop_hook_continue_count: u32,
    /// Most recent stop-hook continuation reason.
    last_stop_hook_continue_reason: Option<String>,
    /// Tool result cache references deleted via Anthropic cache editing.
    cached_microcompact_deleted_refs: Vec<String>,
    /// Cache edit refs discovered this turn but not yet pinned by a successful response.
    pending_cache_edit_refs: Vec<String>,
    /// Cache edit refs that should be re-sent on future requests.
    pinned_cache_edit_refs: Vec<String>,
    /// Virtual post-compact restore blocks that are re-injected at request build time.
    post_compact_restore_blocks: Vec<String>,
    /// Prefix hash for the request currently in flight.
    pending_prompt_cache_prefix_hash: Option<String>,
    /// Previous successful prompt-cache prefix hash.
    last_prompt_cache_prefix_hash: Option<String>,
    pending_prompt_cache_system_hash: Option<String>,
    pending_prompt_cache_restore_hash: Option<String>,
    pending_prompt_cache_tool_hash: Option<String>,
    pending_prompt_cache_message_hash: Option<String>,
    last_prompt_cache_system_hash: Option<String>,
    last_prompt_cache_restore_hash: Option<String>,
    last_prompt_cache_tool_hash: Option<String>,
    last_prompt_cache_message_hash: Option<String>,
    pending_prompt_cache_system_text: Option<String>,
    pending_prompt_cache_restore_text: Option<String>,
    pending_prompt_cache_tool_text: Option<String>,
    pending_prompt_cache_message_text: Option<String>,
    last_prompt_cache_system_text: Option<String>,
    last_prompt_cache_restore_text: Option<String>,
    last_prompt_cache_tool_text: Option<String>,
    last_prompt_cache_message_text: Option<String>,
    /// Whether the current request expects cache read to drop.
    pending_prompt_cache_expected_drop_reason: Option<String>,
    forced_prompt_cache_expected_drop_reason: Option<String>,
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
    /// Stable record for the most recent compact boundary.
    last_compact_boundary: Option<CompactBoundaryRuntimeState>,
    /// Prompt tokens that triggered the most recent compaction.
    last_compaction_prompt_tokens: Option<u32>,
    /// Estimated message + restore token footprint immediately after the most recent compaction.
    last_post_compaction_estimated_tokens: Option<u32>,
    /// Threshold used to decide whether the most recent post-compaction context would compact again.
    last_post_compaction_threshold_tokens: Option<u32>,
    /// Whether the most recent post-compaction context was still above the auto-compact threshold.
    last_post_compaction_will_retrigger: Option<bool>,
    /// Shared restore budget used by the most recent post-compact restore blocks.
    last_restore_budget: Option<RestoreBudgetRuntimeState>,
    /// Running total for compaction-trigger prompt token telemetry.
    compaction_prompt_tokens_total: u64,
    /// Sample count for compaction-trigger prompt token telemetry.
    compaction_prompt_token_samples: u32,
    /// Histogram of compaction outcomes and skip reasons.
    compaction_cause_histogram: BTreeMap<String, u32>,
    /// Old media attachments removed by the latest microcompact pass.
    last_microcompact_media_removed: u32,
    /// Estimated characters saved by the latest media microcompact pass.
    last_microcompact_media_saved_chars: u64,
    /// Old media attachments removed across the current session.
    microcompact_media_removed_total: u64,
    /// Estimated characters saved by media microcompact across the current session.
    microcompact_media_saved_chars_total: u64,
    context_collapse_operations: u32,
    last_context_collapse_at: Option<String>,
    last_context_collapse_artifact_path: Option<String>,
    last_context_collapse_saved_chars: u64,
    context_collapse_saved_chars_total: u64,
    /// Prompt cache telemetry accumulated across turns.
    prompt_cache_runtime: PromptCacheRuntimeState,
    /// Estimated token footprint for the current system prompt.
    system_prompt_estimated_tokens: usize,
    /// Segment breakdown for the current system prompt.
    system_prompt_segments: Vec<SystemPromptSegmentRuntimeState>,
}

fn ordered_recent_read_files(
    recent_file_reads: &[String],
    files_read: &std::collections::HashMap<String, usize>,
) -> Vec<String> {
    let mut ordered = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for path in recent_file_reads.iter().rev() {
        if files_read.contains_key(path) && seen.insert(path.clone()) {
            ordered.push(path.clone());
        }
    }
    for path in files_read.keys() {
        if seen.insert(path.clone()) {
            ordered.push(path.clone());
        }
    }
    ordered
}

/// Convert yode-tools ToolDefinition to yode-llm ToolDefinition.
impl AgentEngine {
    fn now_timestamp() -> String {
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
    }
}
#[cfg(test)]
mod tests;
