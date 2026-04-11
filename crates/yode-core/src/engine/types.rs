use std::collections::BTreeMap;

use yode_llm::types::{ChatResponse, ToolCall};
use yode_tools::registry::ToolPoolSnapshot;
use yode_tools::tool::{ToolProgress, ToolResult};

use crate::tool_runtime::{ToolResultTruncationView, ToolRuntimeCallView};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ProjectKind {
    Rust,
    Node,
    Python,
    Mixed,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RecoveryState {
    Normal,
    ReanchorRequired,
    SingleStepMode,
    NeedUserGuidance,
}

/// Events emitted by the engine for the UI to consume.
#[derive(Debug, Clone)]
pub enum EngineEvent {
    Thinking,
    UsageUpdate(yode_llm::types::Usage),
    TextDelta(String),
    ReasoningDelta(String),
    TextComplete(String),
    ReasoningComplete(String),
    ToolCallStart {
        id: String,
        name: String,
        arguments: String,
    },
    ToolConfirmRequired {
        id: String,
        name: String,
        arguments: String,
    },
    ToolProgress {
        id: String,
        name: String,
        progress: ToolProgress,
    },
    ToolResult {
        id: String,
        name: String,
        result: ToolResult,
    },
    TurnComplete(ChatResponse),
    Error(String),
    Retrying {
        error_message: String,
        attempt: u32,
        max_attempts: u32,
        delay_secs: u64,
    },
    AskUser {
        id: String,
        question: String,
    },
    Done,
    SubAgentStart {
        description: String,
    },
    SubAgentComplete {
        result: String,
    },
    PlanModeEntered,
    PlanApprovalRequired {
        plan_content: String,
    },
    PlanModeExited,
    ContextCompressed {
        mode: String,
        removed: usize,
        tool_results_truncated: usize,
        summary: Option<String>,
        session_memory_path: Option<String>,
        transcript_path: Option<String>,
    },
    CostUpdate {
        estimated_cost: f64,
        input_tokens: u64,
        output_tokens: u64,
        cache_write_tokens: u64,
        cache_read_tokens: u64,
    },
    BudgetExceeded {
        cost: f64,
        limit: f64,
    },
    SuggestionReady {
        suggestion: String,
    },
    SessionMemoryUpdated {
        path: String,
        generated_summary: bool,
    },
    UpdateAvailable(String),
    UpdateDownloading,
    UpdateDownloaded(String),
}

/// Response to a confirmation request.
#[derive(Debug, Clone)]
pub enum ConfirmResponse {
    Allow,
    Deny,
}

#[derive(Debug, Clone)]
pub struct EngineRuntimeState {
    pub query_source: String,
    pub autocompact_disabled: bool,
    pub compaction_failures: u32,
    pub total_compactions: u32,
    pub auto_compactions: u32,
    pub manual_compactions: u32,
    pub last_compaction_breaker_reason: Option<String>,
    pub context_window_tokens: usize,
    pub compaction_threshold_tokens: usize,
    pub estimated_context_tokens: usize,
    pub message_count: usize,
    pub live_session_memory_initialized: bool,
    pub live_session_memory_updating: bool,
    pub live_session_memory_path: String,
    pub session_tool_calls_total: u32,
    pub last_compaction_mode: Option<String>,
    pub last_compaction_at: Option<String>,
    pub last_compaction_summary_excerpt: Option<String>,
    pub last_compaction_session_memory_path: Option<String>,
    pub last_compaction_transcript_path: Option<String>,
    pub last_session_memory_update_at: Option<String>,
    pub last_session_memory_update_path: Option<String>,
    pub last_session_memory_generated_summary: bool,
    pub session_memory_update_count: u32,
    pub tracked_failed_tool_results: usize,
    pub hook_total_executions: u32,
    pub hook_timeout_count: u32,
    pub hook_execution_error_count: u32,
    pub hook_nonzero_exit_count: u32,
    pub hook_wake_notification_count: u32,
    pub last_hook_failure_event: Option<String>,
    pub last_hook_failure_command: Option<String>,
    pub last_hook_failure_reason: Option<String>,
    pub last_hook_failure_at: Option<String>,
    pub last_hook_timeout_command: Option<String>,
    pub last_compaction_prompt_tokens: Option<u32>,
    pub avg_compaction_prompt_tokens: Option<u32>,
    pub compaction_cause_histogram: BTreeMap<String, u32>,
    pub system_prompt_estimated_tokens: usize,
    pub system_prompt_segments: Vec<SystemPromptSegmentRuntimeState>,
    pub prompt_cache: PromptCacheRuntimeState,
    pub last_turn_duration_ms: Option<u64>,
    pub last_turn_stop_reason: Option<String>,
    pub last_turn_artifact_path: Option<String>,
    pub last_stream_watchdog_stage: Option<String>,
    pub stream_retry_reason_histogram: BTreeMap<String, u32>,
    pub recovery_state: String,
    pub recovery_single_step_count: u32,
    pub recovery_reanchor_count: u32,
    pub recovery_need_user_guidance_count: u32,
    pub last_failed_signature: Option<String>,
    pub recovery_breadcrumbs: Vec<String>,
    pub last_recovery_artifact_path: Option<String>,
    pub last_permission_tool: Option<String>,
    pub last_permission_action: Option<String>,
    pub last_permission_explanation: Option<String>,
    pub last_permission_artifact_path: Option<String>,
    pub recent_permission_denials: Vec<String>,
    pub tool_pool: ToolPoolSnapshot,
    pub current_turn_tool_calls: u32,
    pub current_turn_tool_output_bytes: usize,
    pub current_turn_tool_progress_events: u32,
    pub current_turn_parallel_batches: u32,
    pub current_turn_parallel_calls: u32,
    pub current_turn_max_parallel_batch_size: usize,
    pub current_turn_truncated_results: u32,
    pub current_turn_budget_notice_emitted: bool,
    pub current_turn_budget_warning_emitted: bool,
    pub tool_budget_notice_count: u32,
    pub tool_budget_warning_count: u32,
    pub last_tool_budget_warning: Option<String>,
    pub tool_progress_event_count: u32,
    pub last_tool_progress_message: Option<String>,
    pub last_tool_progress_tool: Option<String>,
    pub last_tool_progress_at: Option<String>,
    pub parallel_tool_batch_count: u32,
    pub parallel_tool_call_count: u32,
    pub max_parallel_batch_size: usize,
    pub tool_truncation_count: u32,
    pub last_tool_truncation_reason: Option<String>,
    pub latest_repeated_tool_failure: Option<String>,
    pub read_file_history: Vec<String>,
    pub command_tool_duplication_hints: Vec<String>,
    pub last_tool_turn_completed_at: Option<String>,
    pub last_tool_turn_artifact_path: Option<String>,
    pub tool_error_type_counts: BTreeMap<String, u32>,
    pub tool_trace_scope: String,
    pub tool_traces: Vec<ToolRuntimeCallView>,
}

#[derive(Debug, Clone, Default)]
pub struct PromptCacheRuntimeState {
    pub last_turn_prompt_tokens: Option<u32>,
    pub last_turn_completion_tokens: Option<u32>,
    pub last_turn_cache_write_tokens: Option<u32>,
    pub last_turn_cache_read_tokens: Option<u32>,
    pub reported_turns: u32,
    pub cache_write_turns: u32,
    pub cache_read_turns: u32,
    pub cache_write_tokens_total: u64,
    pub cache_read_tokens_total: u64,
}

#[derive(Debug, Clone, Default)]
pub struct SystemPromptSegmentRuntimeState {
    pub label: String,
    pub chars: usize,
    pub estimated_tokens: usize,
}

#[derive(Debug, Clone, Default)]
pub(super) struct SystemPromptBuild {
    pub prompt: String,
    pub segments: Vec<SystemPromptSegmentRuntimeState>,
    pub estimated_tokens: usize,
}

#[derive(Debug, Clone, Default)]
pub(super) struct ToolExecutionTrace {
    pub call_id: String,
    pub tool_name: String,
    pub started_at: Option<String>,
    pub duration_ms: u64,
    pub input_bytes: usize,
    pub output_bytes: usize,
    pub progress_updates: u32,
    pub success: bool,
    pub error_type: Option<String>,
    pub parallel_batch: Option<u32>,
    pub truncation: Option<ToolResultTruncationView>,
    pub repeated_failure_count: u32,
    pub metadata_summary: Option<String>,
    pub diff_preview: Option<String>,
    pub output_preview: String,
}

impl ToolExecutionTrace {
    pub(super) fn to_view(&self) -> ToolRuntimeCallView {
        ToolRuntimeCallView {
            call_id: self.call_id.clone(),
            tool_name: self.tool_name.clone(),
            started_at: self.started_at.clone(),
            duration_ms: self.duration_ms,
            input_bytes: self.input_bytes,
            output_bytes: self.output_bytes,
            progress_updates: self.progress_updates,
            success: self.success,
            error_type: self.error_type.clone(),
            parallel_batch: self.parallel_batch,
            truncation: self.truncation.clone(),
            repeated_failure_count: self.repeated_failure_count,
            metadata_summary: self.metadata_summary.clone(),
            diff_preview: self.diff_preview.clone(),
            output_preview: self.output_preview.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct ToolExecutionOutcome {
    pub tool_call: ToolCall,
    pub result: ToolResult,
    pub started_at: Option<String>,
    pub duration_ms: u64,
    pub progress_updates: u32,
    pub last_progress_message: Option<String>,
    pub parallel_batch: Option<u32>,
}

#[derive(Debug, Default)]
pub(super) struct SharedMemoryStatus {
    pub last_session_memory_update_at: Option<String>,
    pub last_session_memory_update_path: Option<String>,
    pub last_session_memory_generated_summary: bool,
    pub session_memory_update_count: u32,
}

#[derive(Debug)]
pub(super) struct TranscriptArtifactRuntimeState {
    pub mode: Option<String>,
    pub timestamp: Option<String>,
    pub summary_excerpt: Option<String>,
    pub session_memory_path: Option<String>,
}

pub(super) fn latest_transcript_runtime_state(
    project_root: &std::path::Path,
) -> Option<(std::path::PathBuf, TranscriptArtifactRuntimeState)> {
    let dir = project_root.join(".yode").join("transcripts");
    let mut entries = std::fs::read_dir(&dir)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("md"))
        .collect::<Vec<_>>();
    entries.sort_by(|a, b| b.file_name().cmp(&a.file_name()));
    let path = entries.into_iter().next()?;
    let content = std::fs::read_to_string(&path).ok()?;

    let mut mode = None;
    let mut timestamp = None;
    let mut session_memory_path = None;
    for line in content.lines().take(16) {
        if let Some(value) = line.strip_prefix("- Mode: ") {
            mode = Some(value.to_string());
        } else if let Some(value) = line.strip_prefix("- Timestamp: ") {
            timestamp = Some(value.to_string());
        } else if let Some(value) = line.strip_prefix("- Session memory path: ") {
            session_memory_path = Some(value.to_string());
        }
    }

    let summary_excerpt = content
        .find("## Summary Anchor")
        .and_then(|start| content[start..].find("```text").map(|fence| (start, fence)))
        .and_then(|(start, fence)| {
            let block = &content[start..];
            let after_fence = &block[fence + "```text".len()..];
            after_fence.find("```").map(|end| after_fence[..end].trim())
        })
        .filter(|summary| !summary.is_empty())
        .map(|summary| {
            let excerpt: String = summary.chars().take(160).collect();
            if summary.chars().count() > 160 {
                format!("{}...", excerpt)
            } else {
                excerpt
            }
        });

    Some((
        path,
        TranscriptArtifactRuntimeState {
            mode,
            timestamp,
            summary_excerpt,
            session_memory_path,
        },
    ))
}
