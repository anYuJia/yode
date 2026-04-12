use yode_core::engine::EngineRuntimeState;
use yode_tools::RuntimeTask;

use crate::runtime_timeline::render_runtime_timeline_markdown;

pub(crate) fn write_runtime_task_inventory_artifact(
    project_root: &std::path::Path,
    session_id: &str,
    tasks: Vec<yode_tools::RuntimeTask>,
) -> Option<String> {
    if tasks.is_empty() {
        return None;
    }
    let dir = project_root.join(".yode").join("status");
    std::fs::create_dir_all(&dir).ok()?;
    let short_session = session_id.chars().take(8).collect::<String>();
    let path = dir.join(format!("{}-runtime-tasks.md", short_session));
    let mut body = format!("# Runtime Task Inventory\n\n- Total tasks: {}\n\n", tasks.len());
    for task in tasks {
        body.push_str(&format!(
            "## {}\n\n- Kind: {}\n- Status: {:?}\n- Description: {}\n- Output: {}\n- Transcript: {}\n\n",
            task.id,
            task.kind,
            task.status,
            task.description,
            task.output_path,
            task.transcript_path.as_deref().unwrap_or("none"),
        ));
    }
    std::fs::write(&path, body).ok()?;
    Some(path.display().to_string())
}

pub(crate) fn write_runtime_timeline_artifact(
    project_root: &std::path::Path,
    session_id: &str,
    state: &EngineRuntimeState,
    tasks: &[RuntimeTask],
) -> Option<String> {
    let dir = project_root.join(".yode").join("status");
    std::fs::create_dir_all(&dir).ok()?;
    let short_session = session_id.chars().take(8).collect::<String>();
    let path = dir.join(format!("{}-runtime-timeline.md", short_session));
    std::fs::write(&path, render_runtime_timeline_markdown(state, tasks, 25)).ok()?;
    Some(path.display().to_string())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use yode_core::engine::{EngineRuntimeState, PromptCacheRuntimeState};
    use yode_core::tool_runtime::ToolRuntimeCallView;
    use yode_tools::registry::ToolPoolSnapshot;
    use yode_tools::{RuntimeTask, RuntimeTaskStatus};

    use super::{write_runtime_task_inventory_artifact, write_runtime_timeline_artifact};

    fn test_runtime_state() -> EngineRuntimeState {
        EngineRuntimeState {
            query_source: "User".to_string(),
            autocompact_disabled: false,
            compaction_failures: 0,
            total_compactions: 0,
            auto_compactions: 0,
            manual_compactions: 0,
            last_compaction_breaker_reason: None,
            context_window_tokens: 0,
            compaction_threshold_tokens: 0,
            estimated_context_tokens: 0,
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
            tool_traces: Vec::<ToolRuntimeCallView>::new(),
        }
    }

    #[test]
    fn writes_runtime_task_inventory_markdown() {
        let dir = std::env::temp_dir().join(format!(
            "yode-runtime-artifacts-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let path = write_runtime_task_inventory_artifact(
            &dir,
            "session-1234",
            vec![RuntimeTask {
                id: "task-1".to_string(),
                kind: "bash".to_string(),
                source_tool: "bash".to_string(),
                description: "run tests".to_string(),
                status: RuntimeTaskStatus::Completed,
                attempt: 1,
                retry_of: None,
                output_path: "/tmp/task.log".to_string(),
                transcript_path: Some("/tmp/task.md".to_string()),
                created_at: "2026-01-01 00:00:00".to_string(),
                started_at: None,
                completed_at: None,
                last_progress: None,
                last_progress_at: None,
                progress_history: Vec::new(),
                error: None,
            }],
        )
        .unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("# Runtime Task Inventory"));
        assert!(content.contains("task-1"));
        assert!(content.contains("/tmp/task.md"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn writes_runtime_timeline_markdown() {
        let dir = std::env::temp_dir().join(format!(
            "yode-runtime-timeline-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let path =
            write_runtime_timeline_artifact(&dir, "session-1234", &test_runtime_state(), &[])
                .unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("# Runtime Timeline"));
        assert!(content.contains("no runtime events recorded"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
