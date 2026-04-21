use std::path::Path;
use std::time::Duration;

use yode_core::engine::EngineRuntimeState;

pub(crate) fn format_retry_delay_summary(
    delay_secs: u64,
    attempt: u32,
    max_attempts: u32,
) -> String {
    format!("Retrying in {}s ({}/{})", delay_secs, attempt, max_attempts)
}

pub(crate) fn format_context_compressed_message(
    mode: &str,
    removed: usize,
    tool_results_truncated: usize,
    summary: Option<&str>,
    session_memory_path: Option<&str>,
    transcript_path: Option<&str>,
) -> String {
    let mut parts = vec!["Context compressed".to_string(), mode.to_string()];
    if removed > 0 {
        parts.push(format!("-{} msgs", removed));
    }
    if tool_results_truncated > 0 {
        parts.push(format!("{} tool results truncated", tool_results_truncated));
    }

    let mut content = parts.join(" · ");
    if let Some(summary) = summary.filter(|summary| !summary.trim().is_empty()) {
        content.push_str("\nsummary · ");
        content.push_str(summary.trim());
    };
    if let Some(path) = session_memory_path {
        content.push_str("\nmemory · ");
        content.push_str(path);
    }
    if let Some(path) = transcript_path {
        content.push_str("\ntranscript · ");
        content.push_str(path);
    }

    content
}

pub(crate) fn format_session_memory_update_message(path: &str, generated_summary: bool) -> String {
    format!(
        "Session memory updated · {} · {}",
        if generated_summary {
            "summary"
        } else {
            "snapshot"
        },
        path,
    )
}

pub(crate) fn format_budget_exceeded_message(cost: f64, limit: f64) -> String {
    format!("Budget exceeded · ${:.4} / ${:.2}", cost, limit)
}

pub(crate) fn format_tool_progress_summary(
    tool_name: Option<&str>,
    message: Option<&str>,
    at: Option<&str>,
) -> String {
    match (tool_name, message, at) {
        (None, None, None) => "none".to_string(),
        (Some(tool), Some(message), Some(at)) => format!("{}: {} @ {}", tool, message, at),
        (Some(tool), Some(message), None) => format!("{}: {}", tool, message),
        (Some(tool), None, Some(at)) => format!("{} @ {}", tool, at),
        (Some(tool), None, None) => tool.to_string(),
        (None, Some(message), Some(at)) => format!("{} @ {}", message, at),
        (None, Some(message), None) => message.to_string(),
        (None, None, Some(at)) => format!("updated @ {}", at),
    }
}

pub(crate) fn format_repeated_tool_failure_summary(summary: Option<&str>) -> String {
    let summary = summary.unwrap_or("none");
    if summary.chars().count() <= 120 {
        return summary.to_string();
    }
    format!("{}...", summary.chars().take(120).collect::<String>())
}

pub(crate) fn format_permission_decision_summary(
    tool: Option<&str>,
    action: Option<&str>,
    explanation: Option<&str>,
) -> String {
    format!(
        "{} [{}] {}",
        tool.unwrap_or("none"),
        action.unwrap_or("none"),
        explanation.unwrap_or("none")
    )
}

pub(crate) fn fold_recovery_breadcrumbs(breadcrumbs: &[String], max_items: usize) -> String {
    if breadcrumbs.is_empty() {
        return "none".to_string();
    }
    if breadcrumbs.len() <= max_items {
        return breadcrumbs.join(" -> ");
    }
    let tail = breadcrumbs[breadcrumbs.len() - max_items..].join(" -> ");
    format!("+{} earlier -> {}", breadcrumbs.len() - max_items, tail)
}

pub(crate) fn format_turn_artifact_status(path: Option<&str>) -> String {
    match path {
        None => "none".to_string(),
        Some(path) if Path::new(path).exists() => format!("present: {}", path),
        Some(path) => format!("missing: {}", path),
    }
}

pub(crate) fn format_turn_completed_message(
    elapsed: Duration,
    tools: u32,
    turn_input_tokens: u32,
    turn_output_tokens: u32,
    session_total_tokens: u32,
    session_tool_count: u32,
    runtime: Option<&EngineRuntimeState>,
) -> String {
    let mut content = format!(
        "Turn completed · {} · {} · {}↑ {}↓ tok",
        format_turn_elapsed(elapsed),
        tool_count_label(tools),
        format_token_count(turn_input_tokens as u64),
        format_token_count(turn_output_tokens as u64),
    );
    content.push_str("\nsession · ");
    content.push_str(&format!(
        "{} total tok · {}",
        format_token_count(session_total_tokens as u64),
        tool_count_label(session_tool_count)
    ));

    if let Some(runtime) = runtime {
        if let Some(cache_line) = format_turn_cache_summary(runtime) {
            content.push_str("\ncache · ");
            content.push_str(&cache_line);
        }
        if let Some(stop_reason) = runtime.last_turn_stop_reason.as_deref() {
            if stop_reason != "none" {
                content.push_str("\nstop · ");
                content.push_str(stop_reason);
            }
        }
        if let Some(path) = runtime.last_turn_artifact_path.as_deref() {
            content.push_str("\nartifact · ");
            content.push_str(path);
        }
    }

    content
}

fn format_turn_cache_summary(runtime: &EngineRuntimeState) -> Option<String> {
    let prompt = runtime.prompt_cache.last_turn_prompt_tokens.unwrap_or(0);
    let completion = runtime.prompt_cache.last_turn_completion_tokens.unwrap_or(0);
    let write = runtime.prompt_cache.last_turn_cache_write_tokens.unwrap_or(0);
    let read = runtime.prompt_cache.last_turn_cache_read_tokens.unwrap_or(0);

    if prompt == 0 && completion == 0 && write == 0 && read == 0 {
        return None;
    }

    let status = match (write > 0, read > 0) {
        (true, true) => "hit+write",
        (true, false) => "write",
        (false, true) => "hit",
        (false, false) => "miss",
    };

    Some(format!(
        "{} · {} prompt / {} completion · {} write / {} read",
        status,
        format_token_count(prompt as u64),
        format_token_count(completion as u64),
        format_token_count(write as u64),
        format_token_count(read as u64),
    ))
}

fn format_turn_elapsed(elapsed: Duration) -> String {
    if elapsed.as_secs() >= 60 {
        crate::app::format_duration(elapsed)
    } else {
        format!("{:.1}s", elapsed.as_secs_f64())
    }
}

fn tool_count_label(count: u32) -> String {
    format!("{} {}", count, if count == 1 { "tool" } else { "tools" })
}

fn format_token_count(value: u64) -> String {
    if value >= 1_000_000 {
        format!("{:.1}M", value as f64 / 1_000_000.0)
    } else if value >= 1_000 {
        format!("{:.1}k", value as f64 / 1_000.0)
    } else {
        value.to_string()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::time::Duration;

    use yode_core::engine::{EngineRuntimeState, PromptCacheRuntimeState};
    use yode_tools::registry::ToolPoolSnapshot;

    use super::{
        fold_recovery_breadcrumbs, format_retry_delay_summary, format_tool_progress_summary,
        format_turn_artifact_status, format_turn_completed_message,
    };

    fn runtime_state() -> EngineRuntimeState {
        EngineRuntimeState {
            query_source: "User".to_string(),
            autocompact_disabled: false,
            compaction_failures: 0,
            total_compactions: 0,
            auto_compactions: 0,
            manual_compactions: 0,
            last_compaction_breaker_reason: None,
            context_window_tokens: 128_000,
            compaction_threshold_tokens: 96_000,
            estimated_context_tokens: 64_000,
            message_count: 0,
            live_session_memory_initialized: false,
            live_session_memory_updating: false,
            live_session_memory_path: String::new(),
            session_tool_calls_total: 0,
            last_compaction_mode: None,
            last_compaction_at: None,
            last_compaction_summary_excerpt: None,
            last_compaction_session_memory_path: None,
            last_compaction_transcript_path: None,
            last_session_memory_update_at: None,
            last_session_memory_update_path: None,
            last_session_memory_generated_summary: false,
            session_memory_update_count: 0,
            tracked_failed_tool_results: 0,
            hook_total_executions: 0,
            hook_timeout_count: 0,
            hook_execution_error_count: 0,
            hook_nonzero_exit_count: 0,
            hook_wake_notification_count: 0,
            last_hook_failure_event: None,
            last_hook_failure_command: None,
            last_hook_failure_reason: None,
            last_hook_failure_at: None,
            last_hook_timeout_command: None,
            last_compaction_prompt_tokens: None,
            avg_compaction_prompt_tokens: None,
            compaction_cause_histogram: BTreeMap::new(),
            system_prompt_estimated_tokens: 0,
            system_prompt_segments: Vec::new(),
            prompt_cache: PromptCacheRuntimeState::default(),
            last_turn_duration_ms: None,
            last_turn_stop_reason: None,
            last_turn_artifact_path: None,
            last_stream_watchdog_stage: None,
            stream_retry_reason_histogram: BTreeMap::new(),
            recovery_state: "Normal".to_string(),
            recovery_single_step_count: 0,
            recovery_reanchor_count: 0,
            recovery_need_user_guidance_count: 0,
            last_failed_signature: None,
            recovery_breadcrumbs: Vec::new(),
            last_recovery_artifact_path: None,
            last_permission_tool: None,
            last_permission_action: None,
            last_permission_explanation: None,
            last_permission_artifact_path: None,
            recent_permission_denials: Vec::new(),
            tool_pool: ToolPoolSnapshot::default(),
            current_turn_tool_calls: 0,
            current_turn_tool_output_bytes: 0,
            current_turn_tool_progress_events: 0,
            current_turn_parallel_batches: 0,
            current_turn_parallel_calls: 0,
            current_turn_max_parallel_batch_size: 0,
            current_turn_truncated_results: 0,
            current_turn_budget_notice_emitted: false,
            current_turn_budget_warning_emitted: false,
            tool_budget_notice_count: 0,
            tool_budget_warning_count: 0,
            last_tool_budget_warning: None,
            tool_progress_event_count: 0,
            last_tool_progress_message: None,
            last_tool_progress_tool: None,
            last_tool_progress_at: None,
            parallel_tool_batch_count: 0,
            parallel_tool_call_count: 0,
            max_parallel_batch_size: 0,
            tool_truncation_count: 0,
            last_tool_truncation_reason: None,
            latest_repeated_tool_failure: None,
            read_file_history: Vec::new(),
            command_tool_duplication_hints: Vec::new(),
            last_tool_turn_completed_at: None,
            last_tool_turn_artifact_path: None,
            tool_error_type_counts: BTreeMap::new(),
            tool_trace_scope: "last".to_string(),
            tool_traces: Vec::new(),
        }
    }

    #[test]
    fn fold_recovery_breadcrumbs_compacts_older_entries() {
        let folded = fold_recovery_breadcrumbs(
            &[
                "parse".to_string(),
                "stream".to_string(),
                "tool".to_string(),
                "recover".to_string(),
            ],
            2,
        );
        assert_eq!(folded, "+2 earlier -> tool -> recover");
    }

    #[test]
    fn retry_delay_summary_formats_attempts() {
        assert_eq!(format_retry_delay_summary(5, 2, 5), "Retrying in 5s (2/5)");
    }

    #[test]
    fn context_compressed_message_is_compact() {
        assert_eq!(
            super::format_context_compressed_message(
                "auto",
                4,
                2,
                Some("trimmed older turns"),
                Some("/tmp/memory.md"),
                Some("/tmp/transcript.md"),
            ),
            "Context compressed · auto · -4 msgs · 2 tool results truncated\nsummary · trimmed older turns\nmemory · /tmp/memory.md\ntranscript · /tmp/transcript.md"
        );
    }

    #[test]
    fn tool_progress_summary_includes_timestamp_when_available() {
        assert_eq!(
            format_tool_progress_summary(Some("bash"), Some("running tests"), Some("10:00")),
            "bash: running tests @ 10:00"
        );
    }

    #[test]
    fn turn_artifact_status_reports_missing_paths() {
        assert_eq!(
            format_turn_artifact_status(Some("/definitely/missing/artifact.md")),
            "missing: /definitely/missing/artifact.md"
        );
    }

    #[test]
    fn session_memory_update_message_is_compact() {
        assert_eq!(
            super::format_session_memory_update_message("/tmp/live.md", true),
            "Session memory updated · summary · /tmp/live.md"
        );
    }

    #[test]
    fn budget_exceeded_message_is_compact() {
        assert_eq!(
            super::format_budget_exceeded_message(0.3456, 0.20),
            "Budget exceeded · $0.3456 / $0.20"
        );
    }

    #[test]
    fn turn_completed_message_surfaces_turn_and_runtime_details() {
        let runtime = EngineRuntimeState {
            prompt_cache: PromptCacheRuntimeState {
                last_turn_prompt_tokens: Some(1200),
                last_turn_completion_tokens: Some(180),
                last_turn_cache_write_tokens: Some(300),
                last_turn_cache_read_tokens: Some(200),
                ..Default::default()
            },
            last_turn_stop_reason: Some("Stop".to_string()),
            last_turn_artifact_path: Some("/tmp/turn.md".to_string()),
            ..runtime_state()
        };
        let message = format_turn_completed_message(
            Duration::from_millis(1450),
            3,
            1200,
            180,
            15380,
            34,
            Some(&runtime),
        );
        assert!(message.contains("Turn completed · 1.4s · 3 tools · 1.2k↑ 180↓ tok"));
        assert!(message.contains("session · 15.4k total tok · 34 tools"));
        assert!(message.contains("cache · hit+write"));
        assert!(message.contains("artifact · /tmp/turn.md"));
    }
}
