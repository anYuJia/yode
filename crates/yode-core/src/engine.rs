mod bootstrap_runtime;
mod compaction_runtime;
mod hooks_runtime;
mod intelligence_runtime;
mod llm_runtime;
mod request_runtime;
mod recovery_runtime;
mod retry;
mod runtime_support;
mod session_state;
mod subagent_runner;
mod system_prompt_runtime;
mod turn_setup_runtime;
mod turn_output_runtime;
#[path = "tool_telemetry.rs"]
mod tool_telemetry;
mod tool_execution_runtime;
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
    fn now_timestamp() -> String {
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
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
        self.append_turn_setup_context(user_input).await;
        self.record_turn_user_input(user_input);
        self.reset_turn_runtime_state();
        self.reset_non_streaming_error_state();

        loop {
            let _ = event_tx.send(EngineEvent::Thinking);

            // Build chat request
            let request = self.build_chat_request();

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
            self.push_and_persist_assistant_message(&assistant_msg);

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

                for outcome in parallel_results {
                    self.record_completed_tool_outcome(outcome, &event_tx).await;
                }

                for tool_call in &sequential {
                    let outcome = self
                        .handle_tool_call(tool_call, &event_tx, &mut confirm_rx, None)
                        .await?;
                    self.record_completed_tool_outcome(outcome, &event_tx).await;
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
        self.append_turn_setup_context(user_input).await;
        self.record_turn_user_input(user_input);
        self.reset_turn_runtime_state();

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

            let request = self.build_chat_request();

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
                if let Some(assistant_msg) =
                    self.build_partial_stream_assistant_message(&full_text, &full_reasoning)
                {
                    self.push_and_persist_assistant_message(&assistant_msg);
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

                                let retry_request = self.build_chat_request();

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
                                    if let Some(assistant_msg) = self
                                        .build_partial_stream_assistant_message(
                                            &full_text,
                                            &full_reasoning,
                                        )
                                    {
                                        self.push_and_persist_assistant_message(&assistant_msg);
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
            self.push_and_persist_assistant_message(&assistant_msg);

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
                        self.record_completed_tool_outcome(outcome, &event_tx).await;
                    }
                }

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
                    self.record_completed_tool_outcome(outcome, &event_tx).await;
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

                self.push_and_persist_assistant_message(&resp.message);

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

}
#[cfg(test)]
mod tests;
