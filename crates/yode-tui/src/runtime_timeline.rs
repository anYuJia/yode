use std::cmp::Ordering;
use std::path::Path;
use std::time::SystemTime;

use chrono::{DateTime, Local};
use yode_core::engine::EngineRuntimeState;
use yode_tools::{RuntimeTask, RuntimeTaskStatus};

use crate::commands::artifact_nav::{
    latest_agent_team_artifact, latest_agent_team_monitor_artifact, latest_hook_deferred_artifact,
    latest_hook_deferred_state_artifact, latest_permission_governance_artifact,
    latest_remote_live_session_artifact, latest_remote_live_session_state_artifact,
    latest_remote_session_transcript_sync_artifact,
};
use crate::runtime_display::{
    fold_recovery_breadcrumbs, format_permission_decision_summary, format_tool_progress_summary,
};
use crate::ui::status_summary::{
    context_window_summary_text, runtime_status_snapshot_from_parts, session_runtime_summary_text,
    tool_runtime_summary_text, RuntimeStatusSnapshot,
};

#[derive(Debug, Clone, PartialEq, Eq)]
struct RuntimeTimelineEntry {
    at: Option<String>,
    detail: String,
}

#[allow(dead_code)]
pub(crate) fn build_runtime_timeline_lines(
    state: &EngineRuntimeState,
    tasks: &[RuntimeTask],
    max_items: usize,
) -> Vec<String> {
    build_runtime_timeline_lines_with_project_root(None, state, tasks, max_items)
}

pub(crate) fn build_runtime_timeline_lines_with_project_root(
    project_root: Option<&Path>,
    state: &EngineRuntimeState,
    tasks: &[RuntimeTask],
    max_items: usize,
) -> Vec<String> {
    let mut entries = tasks
        .iter()
        .filter_map(task_timeline_entry)
        .collect::<Vec<_>>();

    if let Some(at) = state.last_tool_turn_completed_at.as_deref() {
        entries.push(RuntimeTimelineEntry {
            at: Some(at.to_string()),
            detail: format!(
                "tool turn completed: artifact={} / session calls={}",
                state
                    .last_tool_turn_artifact_path
                    .as_deref()
                    .unwrap_or("none"),
                state.session_tool_calls_total
            ),
        });
    }
    if let Some(entry) = artifact_timeline_entry(state.last_turn_artifact_path.as_deref(), |path| {
        format!(
            "turn completed: stop={} / artifact={}",
            state.last_turn_stop_reason.as_deref().unwrap_or("none"),
            path
        )
    }) {
        entries.push(entry);
    }

    if let Some(at) = state.last_compaction_at.as_deref() {
        entries.push(RuntimeTimelineEntry {
            at: Some(at.to_string()),
            detail: format!(
                "context compacted: {} / {}",
                state.last_compaction_mode.as_deref().unwrap_or("unknown"),
                compact_detail(
                    state
                        .last_compaction_summary_excerpt
                        .as_deref()
                        .unwrap_or("no compact summary")
                )
            ),
        });
    }

    if let Some(at) = state.last_session_memory_update_at.as_deref() {
        entries.push(RuntimeTimelineEntry {
            at: Some(at.to_string()),
            detail: format!(
                "session memory updated: {} ({})",
                state
                    .last_session_memory_update_path
                    .as_deref()
                    .unwrap_or("none"),
                if state.last_session_memory_generated_summary {
                    "summary"
                } else {
                    "snapshot"
                }
            ),
        });
    }

    let tool_progress_summary = format_tool_progress_summary(
        state.last_tool_progress_tool.as_deref(),
        state.last_tool_progress_message.as_deref(),
        state.last_tool_progress_at.as_deref(),
    );
    if tool_progress_summary != "none" {
        if let Some(at) = state.last_tool_progress_at.as_deref() {
            entries.push(RuntimeTimelineEntry {
                at: Some(at.to_string()),
                detail: format!("tool progress: {}", compact_detail(&tool_progress_summary)),
            });
        }
    }

    if let Some(at) = state.last_hook_failure_at.as_deref() {
        entries.push(RuntimeTimelineEntry {
            at: Some(at.to_string()),
            detail: format!(
                "hook failure: {} [{}] {}",
                state.last_hook_failure_command.as_deref().unwrap_or("unknown"),
                state.last_hook_failure_event.as_deref().unwrap_or("unknown"),
                compact_detail(state.last_hook_failure_reason.as_deref().unwrap_or("unknown"))
            ),
        });
    }
    if state.hook_timeout_count > 0 {
        entries.push(RuntimeTimelineEntry {
            at: state.last_hook_failure_at.clone(),
            detail: format!(
                "hook timeout: {} (count={})",
                state.last_hook_timeout_command.as_deref().unwrap_or("unknown"),
                state.hook_timeout_count
            ),
        });
    }

    let permission_summary = format_permission_decision_summary(
        state.last_permission_tool.as_deref(),
        state.last_permission_action.as_deref(),
        state.last_permission_explanation.as_deref(),
    );
    if permission_summary != "none [none] none" {
        if let Some(entry) =
            artifact_timeline_entry(state.last_permission_artifact_path.as_deref(), |path| {
                format!(
                    "permission decision: {} / artifact={}",
                    compact_detail(&permission_summary),
                    path
                )
            })
        {
            entries.push(entry);
        } else {
            entries.push(RuntimeTimelineEntry {
                at: None,
                detail: format!(
                    "permission decision: {}",
                    compact_detail(&permission_summary)
                ),
            });
        }
    }

    if state.recovery_state != "Normal" || state.last_recovery_artifact_path.is_some() {
        if let Some(entry) =
            artifact_timeline_entry(state.last_recovery_artifact_path.as_deref(), |path| {
                format!(
                    "recovery state: {} / breadcrumbs={} / artifact={}",
                    state.recovery_state,
                    fold_recovery_breadcrumbs(&state.recovery_breadcrumbs, 3),
                    path
                )
            })
        {
            entries.push(entry);
        } else {
            entries.push(RuntimeTimelineEntry {
                at: None,
                detail: format!(
                    "recovery state: {} / breadcrumbs={} / artifact={}",
                    state.recovery_state,
                    fold_recovery_breadcrumbs(&state.recovery_breadcrumbs, 3),
                    state
                        .last_recovery_artifact_path
                        .as_deref()
                        .unwrap_or("none")
                ),
            });
        }
    }

    if let Some(project_root) = project_root {
        extend_with_runtime_family_entries(&mut entries, project_root);
    }

    render_runtime_timeline_entries(entries, max_items)
}

#[allow(dead_code)]
pub(crate) fn render_runtime_timeline_markdown(
    state: &EngineRuntimeState,
    tasks: &[RuntimeTask],
    max_items: usize,
) -> String {
    let lines = build_runtime_timeline_lines(state, tasks, max_items)
        .into_iter()
        .map(|line| format!("- {}", line))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "# Runtime Timeline\n\n{}## Timeline\n\n{}\n",
        timeline_summary_markdown(None, state, tasks),
        lines
    )
}

pub(crate) fn render_runtime_timeline_markdown_with_project_root(
    project_root: &Path,
    state: &EngineRuntimeState,
    tasks: &[RuntimeTask],
    max_items: usize,
) -> String {
    let lines = build_runtime_timeline_lines_with_project_root(Some(project_root), state, tasks, max_items)
        .into_iter()
        .map(|line| format!("- {}", line))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "# Runtime Timeline\n\n{}## Timeline\n\n{}\n",
        timeline_summary_markdown(Some(project_root), state, tasks),
        lines
    )
}

fn timeline_summary_markdown(
    project_root: Option<&Path>,
    state: &EngineRuntimeState,
    tasks: &[RuntimeTask],
) -> String {
    let running_tasks = tasks
        .iter()
        .filter(|task| matches!(task.status, RuntimeTaskStatus::Running))
        .count();
    let snapshot = if let Some(project_root) = project_root {
        runtime_status_snapshot_from_parts(project_root, Some(state.clone()), running_tasks)
    } else {
        RuntimeStatusSnapshot {
            state: Some(state.clone()),
            running_tasks,
            has_team_artifact: false,
            has_live_artifact: false,
            has_defer_artifact: false,
        }
    };
    format!(
        "## Summary\n\n- Runtime: {}\n- Context: {}\n- Tools: {}\n- Tasks: total {} / running {}\n\n",
        session_runtime_summary_text(&snapshot, state.estimated_context_tokens),
        context_window_summary_text(Some(state), state.estimated_context_tokens),
        tool_runtime_summary_text(state),
        tasks.len(),
        running_tasks,
    )
}

fn task_timeline_entry(task: &RuntimeTask) -> Option<RuntimeTimelineEntry> {
    let (phase, at) = if let Some(at) = task.completed_at.clone() {
        (completed_phase(&task.status), Some(at))
    } else if let Some(at) = task.last_progress_at.clone() {
        ("task progress", Some(at))
    } else if let Some(at) = task.started_at.clone() {
        ("task started", Some(at))
    } else {
        ("task created", Some(task.created_at.clone()))
    };

    at.map(|at| RuntimeTimelineEntry {
        at: Some(at),
        detail: format!(
            "{}: {} [{}:{}{}] {}{}{}{}{}{}",
            phase,
            task.id,
            task.kind,
            task_status_label(&task.status),
            if task.source_tool != task.kind {
                format!("/{}", task.source_tool)
            } else {
                String::new()
            },
            compact_detail(&task.description),
            if task.attempt > 1 {
                format!(" / attempt {}", task.attempt)
            } else {
                String::new()
            },
            task.retry_of
                .as_ref()
                .map(|retry_of| format!(" / retry of {}", retry_of))
                .unwrap_or_default(),
            task.last_progress
                .as_ref()
                .map(|progress| format!(" — {}", compact_detail(progress)))
                .or_else(|| {
                    task.error
                        .as_ref()
                        .map(|error| format!(" / error {}", compact_detail(error)))
                })
                .unwrap_or_default(),
            task.transcript_path
                .as_ref()
                .map(|path| format!(" / transcript={}", path))
                .unwrap_or_default(),
            if task.output_path.is_empty() {
                String::new()
            } else {
                format!(" / output={}", task.output_path)
            }
        ),
    })
}

fn completed_phase(status: &RuntimeTaskStatus) -> &'static str {
    match status {
        RuntimeTaskStatus::Completed => "task completed",
        RuntimeTaskStatus::Failed => "task failed",
        RuntimeTaskStatus::Cancelled => "task cancelled",
        RuntimeTaskStatus::Pending => "task pending",
        RuntimeTaskStatus::Running => "task running",
    }
}

fn task_status_label(status: &RuntimeTaskStatus) -> &'static str {
    match status {
        RuntimeTaskStatus::Pending => "pending",
        RuntimeTaskStatus::Running => "running",
        RuntimeTaskStatus::Completed => "completed",
        RuntimeTaskStatus::Failed => "failed",
        RuntimeTaskStatus::Cancelled => "cancelled",
    }
}

fn artifact_timeline_entry(
    path: Option<&str>,
    render_detail: impl FnOnce(&str) -> String,
) -> Option<RuntimeTimelineEntry> {
    let path = path.filter(|path| !path.trim().is_empty())?;
    Some(RuntimeTimelineEntry {
        at: artifact_timestamp(path),
        detail: render_detail(path),
    })
}

fn artifact_timestamp(path: &str) -> Option<String> {
    let modified = std::fs::metadata(Path::new(path)).ok()?.modified().ok()?;
    Some(format_system_time(modified))
}

fn extend_with_runtime_family_entries(entries: &mut Vec<RuntimeTimelineEntry>, project_root: &Path) {
    for (path, label) in [
        (latest_hook_deferred_artifact(project_root), "hook deferred"),
        (
            latest_hook_deferred_state_artifact(project_root),
            "hook deferred state",
        ),
        (
            latest_permission_governance_artifact(project_root),
            "permission governance",
        ),
        (latest_agent_team_artifact(project_root), "agent team"),
        (
            latest_agent_team_monitor_artifact(project_root),
            "agent team monitor",
        ),
        (
            latest_remote_live_session_artifact(project_root),
            "remote live session",
        ),
        (
            latest_remote_live_session_state_artifact(project_root),
            "remote live session state",
        ),
        (
            latest_remote_session_transcript_sync_artifact(project_root),
            "remote transcript sync",
        ),
    ] {
        if let Some(path) = path {
            entries.push(RuntimeTimelineEntry {
                at: artifact_timestamp(&path.display().to_string()),
                detail: format!("{}: artifact={}", label, path.display()),
            });
        }
    }
    for suffix in [
        ("settings-scopes.json", "settings scopes"),
        ("managed-mcp-inventory.json", "managed mcp inventory"),
        ("tool-search-activation.json", "tool search activation"),
        ("permission-policy.json", "permission policy"),
    ] {
        if let Some(path) =
            crate::commands::artifact_nav::latest_artifact_by_suffix(&project_root.join(".yode").join("startup"), suffix.0)
        {
            entries.push(RuntimeTimelineEntry {
                at: artifact_timestamp(&path.display().to_string()),
                detail: format!("{}: artifact={}", suffix.1, path.display()),
            });
        }
    }
}

fn format_system_time(value: SystemTime) -> String {
    let dt: DateTime<Local> = DateTime::<Local>::from(value);
    dt.format("%Y-%m-%d %H:%M:%S").to_string()
}

fn render_runtime_timeline_entries(
    mut entries: Vec<RuntimeTimelineEntry>,
    max_items: usize,
) -> Vec<String> {
    if entries.is_empty() {
        return vec!["no runtime events recorded".to_string()];
    }

    entries.sort_by(|left, right| match (&left.at, &right.at) {
        (Some(left_at), Some(right_at)) => right_at.cmp(left_at),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    });

    let limit = max_items.max(1);
    let hidden = entries.len().saturating_sub(limit);
    entries.truncate(limit);

    let mut lines = entries
        .into_iter()
        .map(|entry| match entry.at {
            Some(at) => format!("{} | {}", at, entry.detail),
            None => format!("undated | {}", entry.detail),
        })
        .collect::<Vec<_>>();

    if hidden > 0 {
        lines.push(format!("+{} earlier timeline events", hidden));
    }

    lines
}

fn compact_detail(detail: &str) -> String {
    if detail.chars().count() <= 96 {
        return detail.to_string();
    }
    format!("{}...", detail.chars().take(96).collect::<String>())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use yode_core::engine::{EngineRuntimeState, PromptCacheRuntimeState};
    use yode_core::tool_runtime::ToolRuntimeCallView;
    use yode_tools::{registry::ToolPoolSnapshot, RuntimeTask, RuntimeTaskStatus};

    use super::{
        build_runtime_timeline_lines, build_runtime_timeline_lines_with_project_root,
        render_runtime_timeline_entries, task_timeline_entry, RuntimeTimelineEntry,
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
    fn timeline_entries_sort_newest_first_and_fold_older_items() {
        let lines = render_runtime_timeline_entries(
            vec![
                RuntimeTimelineEntry {
                    at: Some("2026-01-01 00:00:01".to_string()),
                    detail: "older".to_string(),
                },
                RuntimeTimelineEntry {
                    at: None,
                    detail: "undated".to_string(),
                },
                RuntimeTimelineEntry {
                    at: Some("2026-01-01 00:00:03".to_string()),
                    detail: "newer".to_string(),
                },
            ],
            2,
        );

        assert_eq!(lines[0], "2026-01-01 00:00:03 | newer");
        assert_eq!(lines[1], "2026-01-01 00:00:01 | older");
        assert_eq!(lines[2], "+1 earlier timeline events");
    }

    #[test]
    fn task_timeline_entry_prefers_latest_progress_timestamp() {
        let task = RuntimeTask {
            id: "task-1".to_string(),
            kind: "bash".to_string(),
            source_tool: "bash".to_string(),
            description: "run integration tests".to_string(),
            status: RuntimeTaskStatus::Running,
            attempt: 1,
            retry_of: None,
            output_path: "/tmp/task.log".to_string(),
            transcript_path: None,
            created_at: "2026-01-01 00:00:00".to_string(),
            started_at: Some("2026-01-01 00:01:00".to_string()),
            completed_at: None,
            last_progress: Some("halfway".to_string()),
            last_progress_at: Some("2026-01-01 00:02:00".to_string()),
            progress_history: Vec::new(),
            error: None,
        };

        let entry = task_timeline_entry(&task).expect("timeline entry");
        assert_eq!(entry.at.as_deref(), Some("2026-01-01 00:02:00"));
        assert!(entry.detail.contains("task progress: task-1"));
        assert!(entry.detail.contains("[bash:running]"));
        assert!(entry.detail.contains("halfway"));
    }

    #[test]
    fn task_completion_enrichment_includes_retry_and_artifacts() {
        let task = RuntimeTask {
            id: "task-2".to_string(),
            kind: "agent".to_string(),
            source_tool: "spawn_agent".to_string(),
            description: "verify regression coverage".to_string(),
            status: RuntimeTaskStatus::Failed,
            attempt: 2,
            retry_of: Some("task-1".to_string()),
            output_path: "/tmp/task-2.log".to_string(),
            transcript_path: Some("/tmp/task-2.md".to_string()),
            created_at: "2026-01-01 00:00:00".to_string(),
            started_at: Some("2026-01-01 00:01:00".to_string()),
            completed_at: Some("2026-01-01 00:03:00".to_string()),
            last_progress: None,
            last_progress_at: None,
            progress_history: Vec::new(),
            error: Some("timeout".to_string()),
        };

        let entry = task_timeline_entry(&task).expect("timeline entry");
        assert!(entry.detail.contains("task failed: task-2"));
        assert!(entry.detail.contains("[agent:failed/spawn_agent]"));
        assert!(entry.detail.contains("attempt 2"));
        assert!(entry.detail.contains("retry of task-1"));
        assert!(entry.detail.contains("transcript=/tmp/task-2.md"));
        assert!(entry.detail.contains("output=/tmp/task-2.log"));
        assert!(entry.detail.contains("error timeout"));
    }

    #[test]
    fn build_runtime_timeline_merges_dated_state_and_artifact_events() {
        let dir = std::env::temp_dir().join(format!(
            "yode-runtime-timeline-{}",
            uuid::Uuid::new_v4()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let recovery = dir.join("recovery.md");
        let permission = dir.join("permission.md");
        let turn = dir.join("turn.json");
        std::fs::write(&recovery, "recovery").unwrap();
        std::fs::write(&permission, "permission").unwrap();
        std::fs::write(&turn, "turn").unwrap();

        let mut state = test_runtime_state();
        state.last_compaction_at = Some("2026-01-01 00:05:00".to_string());
        state.last_compaction_mode = Some("auto".to_string());
        state.last_compaction_summary_excerpt = Some("trimmed old messages".to_string());
        state.last_hook_failure_at = Some("2026-01-01 00:04:00".to_string());
        state.last_hook_failure_event = Some("pre_tool".to_string());
        state.last_hook_failure_command = Some("scripts/pre-tool".to_string());
        state.last_hook_failure_reason = Some("exit 2".to_string());
        state.hook_timeout_count = 1;
        state.last_hook_timeout_command = Some("scripts/pre-tool".to_string());
        state.last_permission_tool = Some("bash".to_string());
        state.last_permission_action = Some("confirm".to_string());
        state.last_permission_explanation = Some("needs approval".to_string());
        state.last_permission_artifact_path = Some(permission.display().to_string());
        state.recovery_state = "SingleStepMode".to_string();
        state.last_recovery_artifact_path = Some(recovery.display().to_string());
        state.last_turn_stop_reason = Some("Stop".to_string());
        state.last_turn_artifact_path = Some(turn.display().to_string());

        let lines = build_runtime_timeline_lines(&state, &[], 8);
        assert!(lines.iter().any(|line| line.contains("context compacted: auto")));
        assert!(lines.iter().any(|line| line.contains("hook failure: scripts/pre-tool [pre_tool]")));
        assert!(lines.iter().any(|line| line.contains("hook timeout: scripts/pre-tool")));
        assert!(lines.iter().any(|line| line.contains("permission decision: bash [confirm] needs approval / artifact=")));
        assert!(lines
            .iter()
            .any(|line| line.contains("recovery state: SingleStepMode / breadcrumbs=")));
        assert!(lines.iter().any(|line| line.contains("turn completed: stop=Stop / artifact=")));
        assert!(lines.iter().all(|line| !line.starts_with("undated |")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn empty_timeline_renders_placeholder_line() {
        let lines = render_runtime_timeline_entries(Vec::new(), 4);
        assert_eq!(lines, vec!["no runtime events recorded".to_string()]);
    }

    #[test]
    fn project_root_timeline_includes_extended_runtime_families() {
        let dir = std::env::temp_dir().join(format!(
            "yode-runtime-extended-{}",
            uuid::Uuid::new_v4()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".yode").join("hooks")).unwrap();
        std::fs::create_dir_all(dir.join(".yode").join("teams")).unwrap();
        std::fs::create_dir_all(dir.join(".yode").join("remote")).unwrap();
        std::fs::create_dir_all(dir.join(".yode").join("startup")).unwrap();
        std::fs::write(dir.join(".yode").join("hooks").join("a-hook-deferred.md"), "x").unwrap();
        std::fs::write(dir.join(".yode").join("teams").join("a-agent-team-monitor.md"), "x").unwrap();
        std::fs::write(dir.join(".yode").join("remote").join("a-remote-live-session-state.json"), "{}").unwrap();
        std::fs::write(dir.join(".yode").join("startup").join("a-settings-scopes.json"), "{}").unwrap();
        std::fs::write(dir.join(".yode").join("startup").join("a-managed-mcp-inventory.json"), "{}").unwrap();
        std::fs::write(dir.join(".yode").join("startup").join("a-tool-search-activation.json"), "{}").unwrap();

        let lines = build_runtime_timeline_lines_with_project_root(Some(&dir), &test_runtime_state(), &[], 12);
        assert!(lines.iter().any(|line| line.contains("hook deferred: artifact=")));
        assert!(lines.iter().any(|line| line.contains("agent team monitor: artifact=")));
        assert!(lines.iter().any(|line| line.contains("remote live session state: artifact=")));
        assert!(lines.iter().any(|line| line.contains("settings scopes: artifact=")));
        assert!(lines.iter().any(|line| line.contains("managed mcp inventory: artifact=")));
        assert!(lines.iter().any(|line| line.contains("tool search activation: artifact=")));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
