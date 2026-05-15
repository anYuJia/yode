use yode_core::engine::EngineRuntimeState;
use yode_tools::RuntimeTask;

use crate::runtime_timeline::render_runtime_timeline_markdown_with_project_root;
use crate::ui::status_summary::{
    context_window_summary_text, runtime_status_snapshot_from_parts, session_runtime_summary_text,
    tool_runtime_summary_text,
};

pub(crate) fn write_runtime_task_inventory_artifact(
    project_root: &std::path::Path,
    session_id: &str,
    state: Option<&EngineRuntimeState>,
    tasks: Vec<yode_tools::RuntimeTask>,
) -> Option<String> {
    if tasks.is_empty() {
        return None;
    }
    let dir = project_root.join(".yode").join("status");
    std::fs::create_dir_all(&dir).ok()?;
    let short_session = session_id.chars().take(8).collect::<String>();
    let path = dir.join(format!("{}-runtime-tasks.md", short_session));
    let mut body = String::from("# Runtime Task Inventory\n\n");
    body.push_str(&runtime_summary_markdown(project_root, state, &tasks));
    body.push_str("## Tasks\n\n");
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
    std::fs::write(
        &path,
        render_runtime_timeline_markdown_with_project_root(project_root, state, tasks, 25),
    )
    .ok()?;
    Some(path.display().to_string())
}

pub(crate) fn write_hook_failure_artifact(
    project_root: &std::path::Path,
    session_id: &str,
    state: &EngineRuntimeState,
) -> Option<String> {
    if state.hook_timeout_count == 0
        && state.hook_execution_error_count == 0
        && state.hook_nonzero_exit_count == 0
        && state.last_hook_failure_command.is_none()
    {
        return None;
    }

    let dir = project_root.join(".yode").join("status");
    std::fs::create_dir_all(&dir).ok()?;
    let short_session = session_id.chars().take(8).collect::<String>();
    let path = dir.join(format!("{}-hook-failures.md", short_session));
    let body = format!(
        "# Hook Failure Inspector\n\n{}## Hook Health\n\n- Total runs: {}\n- Timeouts: {}\n- Exec errors: {}\n- Non-zero exits: {}\n- Last failure command: {}\n- Last failure event: {}\n- Last failure reason: {}\n- Last failure at: {}\n- Last timeout command: {}\n",
        runtime_summary_markdown(project_root, Some(state), &[]),
        state.hook_total_executions,
        state.hook_timeout_count,
        state.hook_execution_error_count,
        state.hook_nonzero_exit_count,
        state.last_hook_failure_command.as_deref().unwrap_or("none"),
        state.last_hook_failure_event.as_deref().unwrap_or("none"),
        state.last_hook_failure_reason.as_deref().unwrap_or("none"),
        state.last_hook_failure_at.as_deref().unwrap_or("none"),
        state.last_hook_timeout_command.as_deref().unwrap_or("none"),
    );
    std::fs::write(&path, body).ok()?;
    Some(path.display().to_string())
}

pub(crate) fn write_prompt_cache_artifact(
    project_root: &std::path::Path,
    session_id: &str,
    state: &EngineRuntimeState,
) -> Option<String> {
    let cache = &state.prompt_cache;
    if cache.reported_turns == 0
        && cache.cache_edit_turns == 0
        && cache.prompt_cache_break_count == 0
        && cache.pending_cache_edit_refs == 0
        && cache.pinned_cache_edit_refs == 0
    {
        return None;
    }

    let dir = project_root.join(".yode").join("status");
    std::fs::create_dir_all(&dir).ok()?;
    let short_session = session_id.chars().take(8).collect::<String>();
    let path = dir.join(format!("{}-prompt-cache.md", short_session));
    let pending_refs = prompt_cache_ref_summary(&cache.pending_cache_edit_ref_values);
    let pinned_refs = prompt_cache_ref_summary(&cache.pinned_cache_edit_ref_values);
    let body = format!(
        "# Prompt Cache Inspector\n\n{}## Prompt Cache\n\n- Last turn prompt tokens: {}\n- Last turn completion tokens: {}\n- Last turn cache write tokens: {}\n- Last turn cache read tokens: {}\n- Last turn cache deleted tokens: {}\n- Last turn cache edit deletions: {}\n- Reported turns: {}\n- Cache write turns: {}\n- Cache read turns: {}\n- Cache edit turns: {}\n- Cache write tokens total: {}\n- Cache read tokens total: {}\n- Cache deleted tokens total: {}\n- Cache edit deletions total: {}\n- Pending cache edit refs: {}\n- Pending cache edit ref ids: {}\n- Pinned cache edit refs: {}\n- Pinned cache edit ref ids: {}\n- Prefix hash: {}\n- System hash: {}\n- Restore hash: {}\n- Tool hash: {}\n- Message hash: {}\n- Prefix change summary: {}\n- Transition kind: {}\n- Transition reason: {}\n- Diff artifact: {}\n- Diff summary: {}\n- Prompt cache breaks: {}\n- Last break reason: {}\n- Last break at: {}\n- Expected drop reason: {}\n",
        runtime_summary_markdown(project_root, Some(state), &[]),
        cache.last_turn_prompt_tokens.unwrap_or(0),
        cache.last_turn_completion_tokens.unwrap_or(0),
        cache.last_turn_cache_write_tokens.unwrap_or(0),
        cache.last_turn_cache_read_tokens.unwrap_or(0),
        cache.last_turn_cache_deleted_tokens.unwrap_or(0),
        cache.last_turn_cache_edit_deletions.unwrap_or(0),
        cache.reported_turns,
        cache.cache_write_turns,
        cache.cache_read_turns,
        cache.cache_edit_turns,
        cache.cache_write_tokens_total,
        cache.cache_read_tokens_total,
        cache.cache_deleted_tokens_total,
        cache.cache_edit_deletions_total,
        cache.pending_cache_edit_refs,
        pending_refs,
        cache.pinned_cache_edit_refs,
        pinned_refs,
        cache
            .last_prompt_cache_prefix_hash
            .as_deref()
            .unwrap_or("none"),
        cache
            .last_prompt_cache_system_hash
            .as_deref()
            .unwrap_or("none"),
        cache
            .last_prompt_cache_restore_hash
            .as_deref()
            .unwrap_or("none"),
        cache
            .last_prompt_cache_tool_hash
            .as_deref()
            .unwrap_or("none"),
        cache
            .last_prompt_cache_message_hash
            .as_deref()
            .unwrap_or("none"),
        cache
            .last_prompt_cache_change_summary
            .as_deref()
            .unwrap_or("none"),
        cache
            .last_prompt_cache_transition_kind
            .as_deref()
            .unwrap_or("none"),
        cache
            .last_prompt_cache_transition_reason
            .as_deref()
            .unwrap_or("none"),
        cache
            .last_prompt_cache_diff_artifact_path
            .as_deref()
            .unwrap_or("none"),
        cache
            .last_prompt_cache_diff_summary
            .as_deref()
            .unwrap_or("none"),
        cache.prompt_cache_break_count,
        cache
            .last_prompt_cache_break_reason
            .as_deref()
            .unwrap_or("none"),
        cache
            .last_prompt_cache_break_at
            .as_deref()
            .unwrap_or("none"),
        cache
            .last_prompt_cache_expected_drop_reason
            .as_deref()
            .unwrap_or("none"),
    );
    std::fs::write(&path, body).ok()?;
    Some(path.display().to_string())
}

pub(crate) fn write_prompt_cache_state_artifact(
    project_root: &std::path::Path,
    session_id: &str,
    state: &EngineRuntimeState,
) -> Option<String> {
    let cache = &state.prompt_cache;
    if cache.reported_turns == 0
        && cache.cache_edit_turns == 0
        && cache.prompt_cache_break_count == 0
        && cache.pending_cache_edit_refs == 0
        && cache.pinned_cache_edit_refs == 0
    {
        return None;
    }

    let dir = project_root.join(".yode").join("status");
    std::fs::create_dir_all(&dir).ok()?;
    let short_session = session_id.chars().take(8).collect::<String>();
    let path = dir.join(format!("{}-prompt-cache-state.json", short_session));
    let payload = serde_json::json!({
        "query_source": state.query_source,
        "reported_turns": cache.reported_turns,
        "cache_write_turns": cache.cache_write_turns,
        "cache_read_turns": cache.cache_read_turns,
        "cache_edit_turns": cache.cache_edit_turns,
        "cache_write_tokens_total": cache.cache_write_tokens_total,
        "cache_read_tokens_total": cache.cache_read_tokens_total,
        "cache_deleted_tokens_total": cache.cache_deleted_tokens_total,
        "cache_edit_deletions_total": cache.cache_edit_deletions_total,
        "last_turn_prompt_tokens": cache.last_turn_prompt_tokens,
        "last_turn_completion_tokens": cache.last_turn_completion_tokens,
        "last_turn_cache_write_tokens": cache.last_turn_cache_write_tokens,
        "last_turn_cache_read_tokens": cache.last_turn_cache_read_tokens,
        "last_turn_cache_deleted_tokens": cache.last_turn_cache_deleted_tokens,
        "last_turn_cache_edit_deletions": cache.last_turn_cache_edit_deletions,
        "pending_cache_edit_refs": cache.pending_cache_edit_refs,
        "pinned_cache_edit_refs": cache.pinned_cache_edit_refs,
        "pending_cache_edit_ref_values": cache.pending_cache_edit_ref_values,
        "pinned_cache_edit_ref_values": cache.pinned_cache_edit_ref_values,
        "last_prompt_cache_prefix_hash": cache.last_prompt_cache_prefix_hash,
        "last_prompt_cache_system_hash": cache.last_prompt_cache_system_hash,
        "last_prompt_cache_restore_hash": cache.last_prompt_cache_restore_hash,
        "last_prompt_cache_tool_hash": cache.last_prompt_cache_tool_hash,
        "last_prompt_cache_message_hash": cache.last_prompt_cache_message_hash,
        "last_prompt_cache_change_summary": cache.last_prompt_cache_change_summary,
        "last_prompt_cache_transition_kind": cache.last_prompt_cache_transition_kind,
        "last_prompt_cache_transition_reason": cache.last_prompt_cache_transition_reason,
        "last_prompt_cache_diff_artifact_path": cache.last_prompt_cache_diff_artifact_path,
        "last_prompt_cache_diff_summary": cache.last_prompt_cache_diff_summary,
        "prompt_cache_break_count": cache.prompt_cache_break_count,
        "last_prompt_cache_break_reason": cache.last_prompt_cache_break_reason,
        "last_prompt_cache_break_at": cache.last_prompt_cache_break_at,
        "last_prompt_cache_expected_drop_reason": cache.last_prompt_cache_expected_drop_reason,
        "updated_at": chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
    });
    std::fs::write(&path, serde_json::to_string_pretty(&payload).ok()?).ok()?;
    Some(path.display().to_string())
}

pub(crate) fn write_prompt_cache_event_artifact(
    project_root: &std::path::Path,
    session_id: &str,
    state: &EngineRuntimeState,
) -> Option<String> {
    let cache = &state.prompt_cache;
    if cache.reported_turns == 0
        && cache.cache_edit_turns == 0
        && cache.prompt_cache_break_count == 0
    {
        return None;
    }

    let dir = project_root.join(".yode").join("status");
    std::fs::create_dir_all(&dir).ok()?;
    let short_session = session_id.chars().take(8).collect::<String>();
    let path = dir.join(format!("{}-prompt-cache-events.md", short_session));
    let event = format!(
        "- {} | reported={} read={} write={} deleted_tok={} edit_del={} pending={} pinned={} breaks={} status={} change={} transition={} expected_drop={} break_reason={}\n",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
        cache.reported_turns,
        cache.last_turn_cache_read_tokens.unwrap_or(0),
        cache.last_turn_cache_write_tokens.unwrap_or(0),
        cache.last_turn_cache_deleted_tokens.unwrap_or(0),
        cache.last_turn_cache_edit_deletions.unwrap_or(0),
        cache.pending_cache_edit_refs,
        cache.pinned_cache_edit_refs,
        cache.prompt_cache_break_count,
        prompt_cache_last_turn_status(cache),
        cache
            .last_prompt_cache_change_summary
            .as_deref()
            .unwrap_or("none"),
        cache
            .last_prompt_cache_transition_kind
            .as_deref()
            .unwrap_or("none"),
        cache
            .last_prompt_cache_expected_drop_reason
            .as_deref()
            .unwrap_or("none"),
        cache
            .last_prompt_cache_break_reason
            .as_deref()
            .unwrap_or("none")
    );

    let mut body = if path.exists() {
        std::fs::read_to_string(&path).ok()?
    } else {
        "# Prompt Cache Event Timeline\n\n".to_string()
    };

    if body
        .lines()
        .last()
        .is_some_and(|line| line == event.trim_end())
    {
        return Some(path.display().to_string());
    }

    body.push_str(&event);
    std::fs::write(&path, body).ok()?;
    Some(path.display().to_string())
}

pub(crate) fn write_media_compact_event_artifact(
    project_root: &std::path::Path,
    session_id: &str,
    state: &EngineRuntimeState,
) -> Option<String> {
    if state.microcompact_media_removed_total == 0 {
        return None;
    }

    let dir = project_root.join(".yode").join("status");
    std::fs::create_dir_all(&dir).ok()?;
    let short_session = session_id.chars().take(8).collect::<String>();
    let path = dir.join(format!("{}-media-compact-events.md", short_session));
    let event = format!(
        "- {} | last_removed={} last_saved_chars={} total_removed={} total_saved_chars={}\n",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
        state.last_microcompact_media_removed,
        state.last_microcompact_media_saved_chars,
        state.microcompact_media_removed_total,
        state.microcompact_media_saved_chars_total,
    );

    let mut body = if path.exists() {
        std::fs::read_to_string(&path).ok()?
    } else {
        "# Media Compact Event Timeline\n\n".to_string()
    };

    let event_without_time = event
        .split_once(" | ")
        .map(|(_, tail)| tail.trim_end())
        .unwrap_or(event.trim_end());
    let last_without_time = body
        .lines()
        .last()
        .and_then(|line| line.split_once(" | ").map(|(_, tail)| tail));
    if last_without_time == Some(event_without_time) {
        return Some(path.display().to_string());
    }

    body.push_str(&event);
    std::fs::write(&path, body).ok()?;
    Some(path.display().to_string())
}

pub(crate) fn write_prompt_cache_break_artifact(
    project_root: &std::path::Path,
    session_id: &str,
    state: &EngineRuntimeState,
) -> Option<String> {
    let cache = &state.prompt_cache;
    if cache.prompt_cache_break_count == 0 {
        return None;
    }

    let dir = project_root.join(".yode").join("status");
    std::fs::create_dir_all(&dir).ok()?;
    let short_session = session_id.chars().take(8).collect::<String>();
    let path = dir.join(format!("{}-prompt-cache-break.json", short_session));
    let payload = serde_json::json!({
        "session_id": session_id,
        "prompt_cache_break_count": cache.prompt_cache_break_count,
        "last_break_reason": cache.last_prompt_cache_break_reason,
        "last_break_at": cache.last_prompt_cache_break_at,
        "last_change_summary": cache.last_prompt_cache_change_summary,
        "last_transition_kind": cache.last_prompt_cache_transition_kind,
        "last_transition_reason": cache.last_prompt_cache_transition_reason,
        "last_prefix_hash": cache.last_prompt_cache_prefix_hash,
        "last_system_hash": cache.last_prompt_cache_system_hash,
        "last_restore_hash": cache.last_prompt_cache_restore_hash,
        "last_tool_hash": cache.last_prompt_cache_tool_hash,
        "last_message_hash": cache.last_prompt_cache_message_hash,
        "expected_drop_reason": cache.last_prompt_cache_expected_drop_reason,
        "pending_cache_edit_refs": cache.pending_cache_edit_refs,
        "pinned_cache_edit_refs": cache.pinned_cache_edit_refs,
        "pending_cache_edit_ref_values": cache.pending_cache_edit_ref_values,
        "pinned_cache_edit_ref_values": cache.pinned_cache_edit_ref_values,
        "last_turn_cache_read_tokens": cache.last_turn_cache_read_tokens,
        "last_turn_cache_write_tokens": cache.last_turn_cache_write_tokens,
        "last_turn_cache_deleted_tokens": cache.last_turn_cache_deleted_tokens,
        "last_turn_cache_edit_deletions": cache.last_turn_cache_edit_deletions,
        "cache_read_tokens_total": cache.cache_read_tokens_total,
        "cache_write_tokens_total": cache.cache_write_tokens_total,
        "cache_deleted_tokens_total": cache.cache_deleted_tokens_total,
        "cache_edit_deletions_total": cache.cache_edit_deletions_total,
    });
    std::fs::write(&path, serde_json::to_string_pretty(&payload).ok()?).ok()?;
    Some(path.display().to_string())
}

fn prompt_cache_last_turn_status(cache: &yode_core::engine::PromptCacheRuntimeState) -> String {
    let Some(_) = cache.last_turn_prompt_tokens else {
        return "none".to_string();
    };
    let write = cache.last_turn_cache_write_tokens.unwrap_or(0);
    let read = cache.last_turn_cache_read_tokens.unwrap_or(0);
    let mut status = match (write > 0, read > 0) {
        (true, true) => "hit+write".to_string(),
        (true, false) => "miss+write".to_string(),
        (false, true) => "hit".to_string(),
        (false, false) => "miss".to_string(),
    };
    if cache.last_turn_cache_edit_deletions.unwrap_or(0) > 0
        || cache.last_turn_cache_deleted_tokens.unwrap_or(0) > 0
    {
        status.push_str("+edit");
    }
    status
}

fn prompt_cache_ref_summary(values: &[String]) -> String {
    if values.is_empty() {
        return "none".to_string();
    }

    let mut refs = values.to_vec();
    refs.sort();
    refs.dedup();
    let extra = refs.len().saturating_sub(6);
    refs.truncate(6);
    let mut summary = refs.join(", ");
    if extra > 0 {
        summary.push_str(&format!(", +{} more", extra));
    }
    summary
}

fn runtime_summary_markdown(
    project_root: &std::path::Path,
    state: Option<&EngineRuntimeState>,
    tasks: &[RuntimeTask],
) -> String {
    let running_tasks = tasks
        .iter()
        .filter(|task| matches!(task.status, yode_tools::RuntimeTaskStatus::Running))
        .count();
    let mut lines = vec!["## Summary".to_string(), String::new()];
    if let Some(state) = state {
        let snapshot =
            runtime_status_snapshot_from_parts(project_root, Some(state.clone()), running_tasks);
        lines.push(format!(
            "- Runtime: {}",
            session_runtime_summary_text(&snapshot, state.estimated_context_tokens)
        ));
        lines.push(format!(
            "- Context: {}",
            context_window_summary_text(Some(state), state.estimated_context_tokens)
        ));
        if state.microcompact_media_removed_total > 0 {
            lines.push(format!(
                "- Media compact: last {} / total {} removed, saved ~{} chars",
                state.last_microcompact_media_removed,
                state.microcompact_media_removed_total,
                state.microcompact_media_saved_chars_total
            ));
        }
        lines.push(format!("- Tools: {}", tool_runtime_summary_text(state)));
    }
    lines.push(format!(
        "- Tasks: total {} / running {}",
        tasks.len(),
        running_tasks
    ));
    lines.push(String::new());
    lines.push(String::new());
    lines.join("\n")
}

pub(crate) fn write_task_workspace_bundle_artifact(
    project_root: &std::path::Path,
    session_id: &str,
    task: &RuntimeTask,
) -> Option<String> {
    let dir = project_root.join(".yode").join("status");
    std::fs::create_dir_all(&dir).ok()?;
    let short_session = session_id.chars().take(8).collect::<String>();
    let path = dir.join(format!("{}-{}-bundle.md", short_session, task.id));
    let output_preview = std::fs::read_to_string(&task.output_path)
        .ok()
        .map(|content| {
            let lines = content.lines().collect::<Vec<_>>();
            let start = lines.len().saturating_sub(20);
            lines[start..].join("\n")
        })
        .unwrap_or_else(|| "(unavailable)".to_string());
    let transcript_preview = task
        .transcript_path
        .as_deref()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .unwrap_or_else(|| "(none)".to_string());
    let body = format!(
        "# Task Workspace Bundle\n\n- Task: {}\n- Kind: {}\n- Source tool: {}\n- Status: {:?}\n- Attempt: {}\n- Retry of: {}\n- Output: {}\n- Transcript: {}\n\n## Output Preview\n\n```text\n{}\n```\n\n## Transcript Preview\n\n```text\n{}\n```\n",
        task.id,
        task.kind,
        task.source_tool,
        task.status,
        task.attempt,
        task.retry_of.as_deref().unwrap_or("none"),
        task.output_path,
        task.transcript_path.as_deref().unwrap_or("none"),
        output_preview,
        transcript_preview,
    );
    std::fs::write(&path, body).ok()?;
    Some(path.display().to_string())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use yode_core::engine::{EngineRuntimeState, PromptCacheRuntimeState};
    use yode_core::tool_runtime::ToolRuntimeCallView;
    use yode_tools::registry::ToolPoolSnapshot;
    use yode_tools::{RuntimeTask, RuntimeTaskStatus};

    use super::{
        write_hook_failure_artifact, write_media_compact_event_artifact,
        write_prompt_cache_artifact, write_prompt_cache_break_artifact,
        write_prompt_cache_event_artifact, write_prompt_cache_state_artifact,
        write_runtime_task_inventory_artifact, write_runtime_timeline_artifact,
        write_task_workspace_bundle_artifact,
    };

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
            stop_hook_continue_count: 0,
            last_stop_hook_continue_reason: None,
            last_hook_failure_event: None,
            last_hook_failure_command: None,
            last_hook_failure_reason: None,
            last_hook_failure_at: None,
            last_hook_timeout_command: None,
            last_compaction_prompt_tokens: None,
            last_post_compaction_estimated_tokens: None,
            last_post_compaction_threshold_tokens: None,
            last_post_compaction_will_retrigger: None,
            avg_compaction_prompt_tokens: None,
            compaction_cause_histogram: BTreeMap::new(),
            last_microcompact_media_removed: 0,
            last_microcompact_media_saved_chars: 0,
            microcompact_media_removed_total: 0,
            microcompact_media_saved_chars_total: 0,
            system_prompt_estimated_tokens: 0,
            system_prompt_segments: Vec::new(),
            prompt_cache: PromptCacheRuntimeState::default(),
            cost: Default::default(),
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
        let dir =
            std::env::temp_dir().join(format!("yode-runtime-artifacts-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let path = write_runtime_task_inventory_artifact(
            &dir,
            "session-1234",
            Some(&test_runtime_state()),
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
        assert!(content.contains("## Summary"));
        assert!(content.contains("- Runtime:"));
        assert!(content.contains("task-1"));
        assert!(content.contains("/tmp/task.md"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn writes_runtime_timeline_markdown() {
        let dir =
            std::env::temp_dir().join(format!("yode-runtime-timeline-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut state = test_runtime_state();
        state.last_microcompact_media_removed = 2;
        state.microcompact_media_removed_total = 5;
        state.microcompact_media_saved_chars_total = 4096;

        let path = write_runtime_timeline_artifact(&dir, "session-1234", &state, &[]).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("# Runtime Timeline"));
        assert!(content.contains("## Summary"));
        assert!(content.contains("- Media compact: last 2 / total 5 removed, saved ~4096 chars"));
        assert!(content.contains("## Timeline"));
        assert!(content.contains("media microcompact: last=2 removed"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn writes_hook_failure_markdown() {
        let dir = std::env::temp_dir().join(format!("yode-hook-failures-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut state = test_runtime_state();
        state.hook_timeout_count = 1;
        state.last_hook_failure_command = Some("scripts/pre-tool".to_string());
        state.last_hook_failure_event = Some("pre_tool".to_string());
        state.last_hook_failure_reason = Some("exit 2".to_string());

        let path = write_hook_failure_artifact(&dir, "session-1234", &state).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("# Hook Failure Inspector"));
        assert!(content.contains("## Summary"));
        assert!(content.contains("scripts/pre-tool"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn writes_prompt_cache_markdown() {
        let dir = std::env::temp_dir().join(format!("yode-prompt-cache-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut state = test_runtime_state();
        state.prompt_cache.reported_turns = 2;
        state.prompt_cache.cache_read_turns = 1;
        state.prompt_cache.last_turn_cache_read_tokens = Some(1200);
        state.prompt_cache.prompt_cache_break_count = 1;
        state.prompt_cache.last_prompt_cache_break_reason = Some("cache read dropped".to_string());
        state.prompt_cache.pinned_cache_edit_ref_values =
            vec!["tc1".to_string(), "tc2".to_string()];

        let path = write_prompt_cache_artifact(&dir, "session-1234", &state).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("# Prompt Cache Inspector"));
        assert!(content.contains("- Prompt cache breaks: 1"));
        assert!(content.contains("- Pinned cache edit ref ids: tc1, tc2"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn writes_prompt_cache_state_json() {
        let dir =
            std::env::temp_dir().join(format!("yode-prompt-cache-state-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut state = test_runtime_state();
        state.prompt_cache.reported_turns = 2;
        state.prompt_cache.last_turn_prompt_tokens = Some(1200);
        state.prompt_cache.last_prompt_cache_change_summary = Some("stable".to_string());
        state.prompt_cache.pending_cache_edit_ref_values = vec!["tc9".to_string()];

        let path = write_prompt_cache_state_artifact(&dir, "session-1234", &state).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("\"reported_turns\": 2"));
        assert!(content.contains("\"last_prompt_cache_change_summary\": \"stable\""));
        assert!(content.contains("\"pending_cache_edit_ref_values\": ["));
        assert!(content.contains("\"tc9\""));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn writes_prompt_cache_events_markdown() {
        let dir =
            std::env::temp_dir().join(format!("yode-prompt-cache-events-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut state = test_runtime_state();
        state.prompt_cache.reported_turns = 2;
        state.prompt_cache.last_turn_prompt_tokens = Some(1200);
        state.prompt_cache.last_turn_cache_read_tokens = Some(900);
        state.prompt_cache.pending_cache_edit_refs = 1;

        let path = write_prompt_cache_event_artifact(&dir, "session-1234", &state).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("# Prompt Cache Event Timeline"));
        assert!(content.contains("reported=2"));
        assert!(content.contains("change="));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn writes_media_compact_events_markdown() {
        let dir =
            std::env::temp_dir().join(format!("yode-media-compact-events-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut state = test_runtime_state();
        state.last_microcompact_media_removed = 2;
        state.last_microcompact_media_saved_chars = 2048;
        state.microcompact_media_removed_total = 5;
        state.microcompact_media_saved_chars_total = 4096;

        let path = write_media_compact_event_artifact(&dir, "session-1234", &state).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("# Media Compact Event Timeline"));
        assert!(content.contains("last_removed=2"));
        assert!(content.contains("total_saved_chars=4096"));

        let same = write_media_compact_event_artifact(&dir, "session-1234", &state).unwrap();
        assert_eq!(same, path);
        let deduped = std::fs::read_to_string(&path).unwrap();
        assert_eq!(
            deduped
                .lines()
                .filter(|line| line.contains("total_removed=5"))
                .count(),
            1
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn writes_prompt_cache_break_json() {
        let dir =
            std::env::temp_dir().join(format!("yode-prompt-cache-break-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut state = test_runtime_state();
        state.prompt_cache.prompt_cache_break_count = 1;
        state.prompt_cache.last_prompt_cache_break_reason = Some("cache read dropped".to_string());

        let path = write_prompt_cache_break_artifact(&dir, "session-1234", &state).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("\"prompt_cache_break_count\": 1"));
        assert!(content.contains("cache read dropped"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn writes_task_workspace_bundle_markdown() {
        let dir = std::env::temp_dir().join(format!("yode-task-bundle-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let output = dir.join("task.log");
        let transcript = dir.join("task.md");
        std::fs::write(&output, "hello").unwrap();
        std::fs::write(&transcript, "world").unwrap();

        let task = RuntimeTask {
            id: "task-1".to_string(),
            kind: "bash".to_string(),
            source_tool: "bash".to_string(),
            description: "run tests".to_string(),
            status: RuntimeTaskStatus::Completed,
            attempt: 1,
            retry_of: None,
            output_path: output.display().to_string(),
            transcript_path: Some(transcript.display().to_string()),
            created_at: "2026-01-01 00:00:00".to_string(),
            started_at: None,
            completed_at: None,
            last_progress: None,
            last_progress_at: None,
            progress_history: Vec::new(),
            error: None,
        };

        let path = write_task_workspace_bundle_artifact(&dir, "session-1234", &task).unwrap();
        let content = std::fs::read_to_string(path).unwrap();
        assert!(content.contains("# Task Workspace Bundle"));
        assert!(content.contains("hello"));
        assert!(content.contains("world"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
