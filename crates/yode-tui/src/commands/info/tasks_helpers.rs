use std::path::Path;

use yode_tools::{RuntimeTask, RuntimeTaskStatus};

use super::artifact_preview::preview_markdown;

pub(super) fn task_status_label(status: &RuntimeTaskStatus) -> &'static str {
    match status {
        RuntimeTaskStatus::Pending => "pending",
        RuntimeTaskStatus::Running => "running",
        RuntimeTaskStatus::Completed => "completed",
        RuntimeTaskStatus::Failed => "failed",
        RuntimeTaskStatus::Cancelled => "cancelled",
    }
}

pub(super) fn task_latest_activity_at(task: &RuntimeTask) -> &str {
    task.completed_at
        .as_deref()
        .or(task.last_progress_at.as_deref())
        .or(task.started_at.as_deref())
        .unwrap_or(task.created_at.as_str())
}

pub(super) fn sort_tasks_by_latest_activity(tasks: &mut [RuntimeTask]) {
    tasks.sort_by(|left, right| {
        task_latest_activity_at(right)
            .cmp(task_latest_activity_at(left))
            .then_with(|| right.id.cmp(&left.id))
    });
}

pub(super) fn group_tasks_by_source_tool(tasks: Vec<RuntimeTask>) -> Vec<(String, Vec<RuntimeTask>)> {
    let mut groups: Vec<(String, Vec<RuntimeTask>)> = Vec::new();
    for task in tasks {
        if let Some((_, grouped)) = groups
            .iter_mut()
            .find(|(source_tool, _)| source_tool == &task.source_tool)
        {
            grouped.push(task);
        } else {
            groups.push((task.source_tool.clone(), vec![task]));
        }
    }
    groups
}

pub(super) fn task_retry_chain_summary(task: &RuntimeTask) -> String {
    if task.attempt > 1 {
        format!(
            "attempt {} (retry of {})",
            task.attempt,
            task.retry_of.as_deref().unwrap_or("unknown")
        )
    } else {
        format!("attempt {}", task.attempt)
    }
}

pub(super) fn task_failure_cause_summary(task: &RuntimeTask) -> String {
    match task.status {
        RuntimeTaskStatus::Failed => task
            .error
            .as_deref()
            .map(|error| format!("failed: {}", compact_text(error, 120)))
            .unwrap_or_else(|| "failed".to_string()),
        RuntimeTaskStatus::Cancelled => "cancelled".to_string(),
        RuntimeTaskStatus::Completed => "completed".to_string(),
        RuntimeTaskStatus::Running => task
            .last_progress
            .as_deref()
            .map(|progress| format!("running: {}", compact_text(progress, 120)))
            .unwrap_or_else(|| "running".to_string()),
        RuntimeTaskStatus::Pending => "pending".to_string(),
    }
}

pub(super) fn task_artifact_backlink_summary(task: &RuntimeTask) -> String {
    let mut parts = vec![format!("output={}", compact_path(&task.output_path))];
    if let Some(path) = task.transcript_path.as_deref() {
        parts.push(format!("transcript={}", compact_path(path)));
    }
    parts.join(" | ")
}

pub(super) fn task_transcript_preview(task: &RuntimeTask) -> Option<String> {
    let path = Path::new(task.transcript_path.as_deref()?);
    preview_markdown(path, "## Summary Anchor").or_else(|| preview_markdown(path, "## Messages"))
}

pub(super) fn task_timeline_lines(task: &RuntimeTask) -> Vec<String> {
    let mut lines = vec![format!("{} | created", task.created_at)];
    if let Some(at) = task.started_at.as_deref() {
        lines.push(format!("{} | started", at));
    }
    if let Some(at) = task.last_progress_at.as_deref() {
        lines.push(format!(
            "{} | progress: {}",
            at,
            compact_text(task.last_progress.as_deref().unwrap_or("updated"), 100)
        ));
    }
    if let Some(at) = task.completed_at.as_deref() {
        lines.push(format!(
            "{} | {}",
            at,
            task_failure_cause_summary(task)
        ));
    }
    lines
}

pub(super) fn task_output_preview(
    task: &RuntimeTask,
    max_lines: usize,
) -> (String, usize, usize) {
    let content = std::fs::read_to_string(&task.output_path).ok();
    match content {
        Some(content) => {
            let lines = content.lines().collect::<Vec<_>>();
            let preview_start = lines.len().saturating_sub(max_lines);
            (lines[preview_start..].join("\n"), preview_start + 1, lines.len())
        }
        None => ("(unavailable)".to_string(), 0, 0),
    }
}

pub(super) fn task_cancel_summary(task: &RuntimeTask, cancellation_requested: bool) -> String {
    if cancellation_requested {
        format!(
            "Cancellation requested for {} [{}:{}] {} / {} / {}",
            task.id,
            task.kind,
            task_status_label(&task.status),
            compact_text(&task.description, 100),
            task_retry_chain_summary(task),
            task_artifact_backlink_summary(task)
        )
    } else {
        format!(
            "Task {} is already {}. {} / {}",
            task.id,
            task_status_label(&task.status),
            task_retry_chain_summary(task),
            task_artifact_backlink_summary(task)
        )
    }
}

fn compact_path(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(path)
        .to_string()
}

fn compact_text(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    format!("{}...", text.chars().take(max_chars).collect::<String>())
}

#[cfg(test)]
mod tests {
    use yode_tools::{RuntimeTask, RuntimeTaskStatus};

    use super::{
        group_tasks_by_source_tool, sort_tasks_by_latest_activity, task_artifact_backlink_summary,
        task_retry_chain_summary, task_timeline_lines, task_transcript_preview,
    };

    fn make_task(
        id: &str,
        source_tool: &str,
        status: RuntimeTaskStatus,
        created_at: &str,
        last_progress_at: Option<&str>,
    ) -> RuntimeTask {
        RuntimeTask {
            id: id.to_string(),
            kind: if source_tool == "spawn_agent" {
                "agent".to_string()
            } else {
                "bash".to_string()
            },
            source_tool: source_tool.to_string(),
            description: format!("desc {}", id),
            status,
            attempt: 1,
            retry_of: None,
            output_path: format!("/tmp/{}.log", id),
            transcript_path: Some(format!("/tmp/{}.md", id)),
            created_at: created_at.to_string(),
            started_at: Some(created_at.to_string()),
            completed_at: None,
            last_progress: Some("building".to_string()),
            last_progress_at: last_progress_at.map(str::to_string),
            progress_history: vec!["building".to_string()],
            error: None,
        }
    }

    #[test]
    fn sort_tasks_prefers_freshest_activity() {
        let mut tasks = vec![
            make_task(
                "task-1",
                "bash",
                RuntimeTaskStatus::Running,
                "2026-01-01 00:00:00",
                Some("2026-01-01 00:00:02"),
            ),
            make_task(
                "task-2",
                "spawn_agent",
                RuntimeTaskStatus::Running,
                "2026-01-01 00:00:00",
                Some("2026-01-01 00:00:03"),
            ),
        ];
        sort_tasks_by_latest_activity(&mut tasks);
        assert_eq!(tasks[0].id, "task-2");
    }

    #[test]
    fn groups_tasks_by_source_tool_in_freshest_order() {
        let groups = group_tasks_by_source_tool(vec![
            make_task(
                "task-2",
                "spawn_agent",
                RuntimeTaskStatus::Running,
                "2026-01-01 00:00:00",
                Some("2026-01-01 00:00:03"),
            ),
            make_task(
                "task-1",
                "bash",
                RuntimeTaskStatus::Running,
                "2026-01-01 00:00:00",
                Some("2026-01-01 00:00:02"),
            ),
        ]);
        assert_eq!(groups[0].0, "spawn_agent");
        assert_eq!(groups[1].0, "bash");
    }

    #[test]
    fn retry_chain_summary_formats_attempts() {
        let mut task = make_task(
            "task-1",
            "bash",
            RuntimeTaskStatus::Running,
            "2026-01-01 00:00:00",
            None,
        );
        task.attempt = 3;
        task.retry_of = Some("task-0".to_string());
        assert_eq!(task_retry_chain_summary(&task), "attempt 3 (retry of task-0)");
    }

    #[test]
    fn artifact_summary_compacts_to_filenames() {
        let task = make_task(
            "task-1",
            "bash",
            RuntimeTaskStatus::Running,
            "2026-01-01 00:00:00",
            None,
        );
        assert_eq!(task_artifact_backlink_summary(&task), "output=task-1.log | transcript=task-1.md");
    }

    #[test]
    fn task_timeline_lines_cover_task_phases() {
        let mut task = make_task(
            "task-1",
            "bash",
            RuntimeTaskStatus::Failed,
            "2026-01-01 00:00:00",
            Some("2026-01-01 00:00:02"),
        );
        task.completed_at = Some("2026-01-01 00:00:03".to_string());
        task.error = Some("timeout".to_string());
        let lines = task_timeline_lines(&task);
        assert!(lines[0].contains("created"));
        assert!(lines.iter().any(|line| line.contains("started")));
        assert!(lines.iter().any(|line| line.contains("progress")));
        assert!(lines.iter().any(|line| line.contains("failed: timeout")));
    }

    #[test]
    fn transcript_preview_reads_summary_anchor() {
        let dir = std::env::temp_dir().join(format!(
            "yode-task-preview-{}",
            uuid::Uuid::new_v4()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let transcript = dir.join("task.md");
        std::fs::write(
            &transcript,
            "# Runtime Task\n\n## Summary Anchor\n\n```text\npreview line\n```\n",
        )
        .unwrap();
        let mut task = make_task(
            "task-1",
            "bash",
            RuntimeTaskStatus::Running,
            "2026-01-01 00:00:00",
            None,
        );
        task.transcript_path = Some(transcript.display().to_string());
        assert_eq!(task_transcript_preview(&task).as_deref(), Some("preview line"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
