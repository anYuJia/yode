use crate::runtime_display::{
    format_permission_decision_summary, format_tool_progress_summary,
};
use crate::runtime_timeline::build_runtime_timeline_lines_with_project_root;
use crate::ui::status_summary::{
    context_window_summary_text, runtime_status_snapshot_from_parts,
    session_runtime_summary_text, tool_runtime_summary_text,
};

pub(crate) fn render_diagnostics_overview(
    project_root: &std::path::Path,
    state: &yode_core::engine::EngineRuntimeState,
    tasks: &[yode_tools::RuntimeTask],
) -> String {
    let running_tasks = tasks
        .iter()
        .filter(|task| matches!(task.status, yode_tools::RuntimeTaskStatus::Running))
        .count();
    let runtime_snapshot =
        runtime_status_snapshot_from_parts(project_root, Some(state.clone()), running_tasks);
    let runtime_summary =
        session_runtime_summary_text(&runtime_snapshot, state.estimated_context_tokens);
    let context_summary =
        context_window_summary_text(Some(state), state.estimated_context_tokens);
    let tool_summary = tool_runtime_summary_text(state);
    let recent_denials = if state.recent_permission_denials.is_empty() {
        "none".to_string()
    } else {
        state.recent_permission_denials.join(" | ")
    };
    let tool_errors = if state.tool_error_type_counts.is_empty() {
        "none".to_string()
    } else {
        state
            .tool_error_type_counts
            .iter()
            .map(|(kind, count)| format!("{}={}", kind, count))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let permission_summary = format_permission_decision_summary(
        state.last_permission_tool.as_deref(),
        state.last_permission_action.as_deref(),
        state.last_permission_explanation.as_deref(),
    );
    let tool_progress_summary = format_tool_progress_summary(
        state.last_tool_progress_tool.as_deref(),
        state.last_tool_progress_message.as_deref(),
        state.last_tool_progress_at.as_deref(),
    );
    let timeline = build_runtime_timeline_lines_with_project_root(Some(project_root), state, tasks, 6)
        .into_iter()
        .map(|line| format!("  - {}", line))
        .collect::<Vec<_>>()
        .join("\n");
    let startup_settings = crate::commands::info::startup_artifacts::latest_settings_scopes(project_root)
        .map(|summary| {
            summary
                .scopes
                .into_iter()
                .map(|scope| format!("{}:{} mcp={} rules={}", scope.scope, scope.permission_default_mode.unwrap_or_else(|| "inherit".to_string()), scope.mcp_server_count, scope.permission_rule_count))
                .collect::<Vec<_>>()
                .join(" | ")
        })
        .unwrap_or_else(|| "none".to_string());
    let managed_mcp = crate::commands::info::startup_artifacts::latest_managed_mcp_inventory(project_root)
        .map(|summary| {
            format!(
                "effective={} configured={} connected={} tools={} failures={}",
                summary.effective_server_count,
                summary.configured_server_count,
                summary.connected_server_count,
                summary.mcp_tool_count,
                summary.failure_count
            )
        })
        .unwrap_or_else(|| "none".to_string());
    let team_state = crate::commands::artifact_nav::latest_agent_team_state_artifact(project_root)
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "none".to_string());
    let remote_live = crate::commands::artifact_nav::latest_remote_live_session_state_artifact(project_root)
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "none".to_string());
    let hook_defer = crate::commands::artifact_nav::latest_hook_deferred_state_artifact(project_root)
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "none".to_string());

    format!(
        "Diagnostics overview:\n  Runtime summary: {}\n  Context summary: {}\n  Tool summary:    {}\n\nContext:\n  Query source:   {}\n  Compact count:  {} (auto {}, manual {})\n  Breaker reason: {}\n  Compact tokens: {}\n\nMemory:\n  Live memory:    {}{}\n  Memory updates: {}\n  Last memory:    {}\n\nRecovery:\n  State:          {}\n  Last signature: {}\n  Permission:     {}\n  Denials:        {}\n\nTools:\n  Session calls:  {}\n  Progress:       {}\n  Parallel:       {} batches / {} calls\n  Truncations:    {}\n  Errors:         {}\n  Last artifact:  {}\n\nObservability:\n  Hook defer:     {}\n  Agent team:     {}\n  Remote live:    {}\n  Settings:       {}\n  Managed MCP:    {}\n\nTasks:\n  Total:          {}\n  Running:        {}\n\nHooks:\n  Total runs:     {}\n  Timeouts:       {}\n  Wake notices:   {}\n\nTimeline:\n{}",
        runtime_summary,
        context_summary,
        tool_summary,
        state.query_source,
        state.total_compactions,
        state.auto_compactions,
        state.manual_compactions,
        state
            .last_compaction_breaker_reason
            .as_deref()
            .unwrap_or("none"),
        state
            .last_compaction_prompt_tokens
            .map(|value| value.to_string())
            .unwrap_or_else(|| "none".to_string()),
        if state.live_session_memory_initialized {
            "warm"
        } else {
            "cold"
        },
        if state.live_session_memory_updating {
            " (updating)"
        } else {
            ""
        },
        state.session_memory_update_count,
        state
            .last_session_memory_update_path
            .as_deref()
            .unwrap_or("none"),
        state.recovery_state,
        state.last_failed_signature.as_deref().unwrap_or("none"),
        permission_summary,
        recent_denials,
        state.session_tool_calls_total,
        format!(
            "{} ({})",
            state.tool_progress_event_count,
            tool_progress_summary
        ),
        state.parallel_tool_batch_count,
        state.parallel_tool_call_count,
        state.tool_truncation_count,
        tool_errors,
        state
            .last_tool_turn_artifact_path
            .as_deref()
            .unwrap_or("none"),
        hook_defer,
        team_state,
        remote_live,
        startup_settings,
        managed_mcp,
        tasks.len(),
        running_tasks,
        state.hook_total_executions,
        state.hook_timeout_count,
        state.hook_wake_notification_count,
        timeline,
    )
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use yode_core::engine::{EngineRuntimeState, PromptCacheRuntimeState};
    use yode_core::tool_runtime::ToolRuntimeCallView;
    use yode_tools::registry::ToolPoolSnapshot;

    use super::render_diagnostics_overview;

    fn state() -> EngineRuntimeState {
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
    fn diagnostics_overview_mentions_extended_observability_artifacts() {
        let dir = std::env::temp_dir().join(format!(
            "yode-diagnostics-extended-{}",
            uuid::Uuid::new_v4()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".yode").join("hooks")).unwrap();
        std::fs::create_dir_all(dir.join(".yode").join("teams")).unwrap();
        std::fs::create_dir_all(dir.join(".yode").join("remote")).unwrap();
        std::fs::create_dir_all(dir.join(".yode").join("startup")).unwrap();
        std::fs::write(dir.join(".yode").join("hooks").join("a-hook-deferred-state.json"), "{}").unwrap();
        std::fs::write(dir.join(".yode").join("teams").join("a-agent-team-state.json"), "{}").unwrap();
        std::fs::write(dir.join(".yode").join("remote").join("a-remote-live-session-state.json"), "{}").unwrap();
        std::fs::write(dir.join(".yode").join("startup").join("a-settings-scopes.json"), r#"{"scopes":[]}"#).unwrap();
        std::fs::write(dir.join(".yode").join("startup").join("a-managed-mcp-inventory.json"), r#"{"effective_server_count":1,"configured_server_count":1,"connected_server_count":1,"mcp_tool_count":2,"failure_count":0}"#).unwrap();
        let rendered = render_diagnostics_overview(&dir, &state(), &[]);
        assert!(rendered.contains("Hook defer:"));
        assert!(rendered.contains("Agent team:"));
        assert!(rendered.contains("Remote live:"));
        assert!(rendered.contains("Managed MCP:"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn diagnostics_overview_includes_shared_runtime_summaries() {
        let rendered = render_diagnostics_overview(std::path::Path::new("/tmp"), &state(), &[]);
        assert!(rendered.contains("Runtime summary:"));
        assert!(rendered.contains("Context summary:"));
        assert!(rendered.contains("Tool summary:"));
    }
}
