use std::cmp::Ordering;

use yode_core::engine::EngineRuntimeState;
use yode_tools::{RuntimeTask, RuntimeTaskStatus};

use crate::runtime_display::{
    format_permission_decision_summary, format_tool_progress_summary,
};

#[derive(Debug, Clone, PartialEq, Eq)]
struct RuntimeTimelineEntry {
    at: Option<String>,
    detail: String,
}

pub(crate) fn build_runtime_timeline_lines(
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

    let permission_summary = format_permission_decision_summary(
        state.last_permission_tool.as_deref(),
        state.last_permission_action.as_deref(),
        state.last_permission_explanation.as_deref(),
    );
    if permission_summary != "none [none] none" {
        entries.push(RuntimeTimelineEntry {
            at: None,
            detail: format!(
                "permission decision: {}",
                compact_detail(&permission_summary)
            ),
        });
    }

    if state.recovery_state != "Normal" || state.last_recovery_artifact_path.is_some() {
        entries.push(RuntimeTimelineEntry {
            at: None,
            detail: format!(
                "recovery state: {} / artifact={}",
                state.recovery_state,
                state
                    .last_recovery_artifact_path
                    .as_deref()
                    .unwrap_or("none")
            ),
        });
    }

    render_runtime_timeline_entries(entries, max_items)
}

fn task_timeline_entry(task: &RuntimeTask) -> Option<RuntimeTimelineEntry> {
    let at = task
        .completed_at
        .clone()
        .or_else(|| task.last_progress_at.clone())
        .or_else(|| task.started_at.clone())
        .or_else(|| Some(task.created_at.clone()));

    at.map(|at| RuntimeTimelineEntry {
        at: Some(at),
        detail: format!(
            "task {} [{}:{}] {}{}",
            task.id,
            task.kind,
            task_status_label(&task.status),
            compact_detail(&task.description),
            task.last_progress
                .as_ref()
                .map(|progress| format!(" — {}", compact_detail(progress)))
                .unwrap_or_default()
        ),
    })
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
    use yode_tools::{RuntimeTask, RuntimeTaskStatus};

    use super::{render_runtime_timeline_entries, task_timeline_entry, RuntimeTimelineEntry};

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
        assert!(entry.detail.contains("[bash:running]"));
        assert!(entry.detail.contains("halfway"));
    }

    #[test]
    fn empty_timeline_renders_placeholder_line() {
        let lines = render_runtime_timeline_entries(Vec::new(), 4);
        assert_eq!(lines, vec!["no runtime events recorded".to_string()]);
    }
}
