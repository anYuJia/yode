use crate::commands::{CommandOutput, CommandResult};

use super::tasks::TaskFilter;
use super::tasks_helpers::{
    group_tasks_by_source_tool, sort_tasks_by_latest_activity, task_artifact_backlink_summary,
    task_failure_cause_summary, task_latest_activity_at, task_output_preview,
    task_retry_chain_summary, task_status_label, task_timeline_lines, task_transcript_preview,
};

pub(super) fn render_task_list(tasks: Vec<yode_tools::RuntimeTask>) -> CommandResult {
    if tasks.is_empty() {
        return Ok(CommandOutput::Message(
            "No background runtime tasks recorded.".to_string(),
        ));
    }
    let mut tasks = tasks;
    sort_tasks_by_latest_activity(&mut tasks);
    let mut lines = vec![format!("Runtime tasks ({}):", tasks.len())];
    for (source_tool, grouped_tasks) in group_tasks_by_source_tool(tasks) {
        lines.push(format!("  Source tool: {} ({})", source_tool, grouped_tasks.len()));
        for task in grouped_tasks {
            lines.push(format!(
                "    {} [{}:{}] {} @ {} / {} / {}",
                task.id,
                task.kind,
                task_status_label(&task.status),
                task.description,
                task_latest_activity_at(&task),
                task_retry_chain_summary(&task),
                task_failure_cause_summary(&task)
            ));
        }
    }
    Ok(CommandOutput::Messages(lines))
}

pub(super) fn render_task_detail(
    engine: &yode_core::engine::AgentEngine,
    id: &str,
) -> CommandResult {
    let Some(task) = engine.runtime_task_snapshot(id) else {
        return Err(format!("Task '{}' not found.", id));
    };
    let (output_preview, preview_start, total_lines) = task_output_preview(&task, 10);
    let progress_history = if task.progress_history.is_empty() {
        "none".to_string()
    } else {
        task.progress_history
            .iter()
            .map(|progress| format!("    - {}", progress))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let timeline = task_timeline_lines(&task)
        .into_iter()
        .map(|line| format!("    - {}", line))
        .collect::<Vec<_>>()
        .join("\n");
    let transcript_preview = task_transcript_preview(&task)
        .map(|preview| format!("    - {}", preview))
        .unwrap_or_else(|| "    - none".to_string());
    Ok(CommandOutput::Message(format!(
        "Task {}:\n  Kind:           {}\n  Source tool:    {}\n  Status:         {}\n  Description:    {}\n  Retry chain:    {}\n  Freshest:       {}\n  Failure:        {}\n  Artifacts:      {}\n  Output:         {}\n  Transcript:     {}\n\n  Timeline:\n{}\n\n  Transcript preview:\n{}\n\n  Recent progress:\n{}\n\n  Output tail:    lines {}-{} of {}\n{}\n\nUse `/tasks read {}` for the full tail.",
        task.id,
        task.kind,
        task.source_tool,
        task_status_label(&task.status),
        task.description,
        task_retry_chain_summary(&task),
        task_latest_activity_at(&task),
        task_failure_cause_summary(&task),
        task_artifact_backlink_summary(&task),
        task.output_path,
        task.transcript_path.as_deref().unwrap_or("none"),
        timeline,
        transcript_preview,
        progress_history,
        preview_start,
        total_lines,
        total_lines,
        output_preview,
        task.id,
    )))
}

pub(super) fn render_task_output(task: &yode_tools::RuntimeTask) -> CommandResult {
    let (output_preview, preview_start, total_lines) = task_output_preview(task, 40);
    let timeline = task_timeline_lines(task)
        .into_iter()
        .map(|line| format!("  - {}", line))
        .collect::<Vec<_>>()
        .join("\n");
    let transcript_preview = task_transcript_preview(task)
        .map(|preview| format!("  - {}", preview))
        .unwrap_or_else(|| "  - none".to_string());
    Ok(CommandOutput::Message(format!(
        "Task output {}:\n  Status:        {} [{}:{}]\n  Retry chain:   {}\n  Freshest:      {}\n  Failure:       {}\n  Artifacts:     {}\n  Output path:   {}\n  Transcript:    {}\n\nTimeline:\n{}\n\nTranscript preview:\n{}\n\nOutput tail: lines {}-{} of {}\n\n{}",
        task.id,
        task_status_label(&task.status),
        task.kind,
        task.source_tool,
        task_retry_chain_summary(task),
        task_latest_activity_at(task),
        task_failure_cause_summary(task),
        task_artifact_backlink_summary(task),
        task.output_path,
        task.transcript_path.as_deref().unwrap_or("none"),
        timeline,
        transcript_preview,
        preview_start,
        total_lines,
        total_lines,
        output_preview,
    )))
}

pub(super) fn parse_task_filter(value: &str) -> Option<TaskFilter> {
    match value {
        "pending" => Some(TaskFilter::Status(yode_tools::RuntimeTaskStatus::Pending)),
        "running" => Some(TaskFilter::Status(yode_tools::RuntimeTaskStatus::Running)),
        "completed" => Some(TaskFilter::Status(yode_tools::RuntimeTaskStatus::Completed)),
        "failed" => Some(TaskFilter::Status(yode_tools::RuntimeTaskStatus::Failed)),
        "cancelled" => Some(TaskFilter::Status(yode_tools::RuntimeTaskStatus::Cancelled)),
        "bash" => Some(TaskFilter::Kind("bash")),
        "agent" => Some(TaskFilter::Kind("agent")),
        _ => None,
    }
}

pub(super) fn task_matches_filter(task: &yode_tools::RuntimeTask, filter: &TaskFilter) -> bool {
    match filter {
        TaskFilter::Status(status) => task.status == *status,
        TaskFilter::Kind(kind) => task.kind == *kind,
    }
}

#[cfg(test)]
mod tests {
    use yode_tools::{RuntimeTask, RuntimeTaskStatus};

    use super::{render_task_list, render_task_output};

    fn make_task(
        id: &str,
        source_tool: &str,
        status: RuntimeTaskStatus,
        last_progress_at: &str,
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
            created_at: "2026-01-01 00:00:00".to_string(),
            started_at: Some("2026-01-01 00:00:01".to_string()),
            completed_at: None,
            last_progress: Some("building".to_string()),
            last_progress_at: Some(last_progress_at.to_string()),
            progress_history: vec!["building".to_string()],
            error: None,
        }
    }

    #[test]
    fn task_list_groups_by_source_tool() {
        let output = render_task_list(vec![
            make_task(
                "task-2",
                "spawn_agent",
                RuntimeTaskStatus::Running,
                "2026-01-01 00:00:03",
            ),
            make_task(
                "task-1",
                "bash",
                RuntimeTaskStatus::Running,
                "2026-01-01 00:00:02",
            ),
        ])
        .unwrap();
        match output {
            crate::commands::CommandOutput::Messages(lines) => {
                assert!(lines.iter().any(|line| line.contains("Source tool: spawn_agent")));
                assert!(lines.iter().any(|line| line.contains("Source tool: bash")));
            }
            _ => panic!("expected message list"),
        }
    }

    #[test]
    fn task_output_render_splits_sections() {
        let dir = std::env::temp_dir().join(format!("yode-task-output-{}", uuid::Uuid::new_v4()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let output_path = dir.join("task.log");
        let transcript_path = dir.join("task.md");
        std::fs::write(&output_path, "line1\nline2\nline3\n").unwrap();
        std::fs::write(
            &transcript_path,
            "# Runtime Task\n\n## Summary Anchor\n\n```text\npreview\n```\n",
        )
        .unwrap();
        let task = RuntimeTask {
            id: "task-1".to_string(),
            kind: "bash".to_string(),
            source_tool: "bash".to_string(),
            description: "run tests".to_string(),
            status: RuntimeTaskStatus::Running,
            attempt: 2,
            retry_of: Some("task-0".to_string()),
            output_path: output_path.display().to_string(),
            transcript_path: Some(transcript_path.display().to_string()),
            created_at: "2026-01-01 00:00:00".to_string(),
            started_at: Some("2026-01-01 00:00:01".to_string()),
            completed_at: None,
            last_progress: Some("building".to_string()),
            last_progress_at: Some("2026-01-01 00:00:02".to_string()),
            progress_history: vec!["building".to_string()],
            error: None,
        };

        let output = render_task_output(&task).unwrap();
        match output {
            crate::commands::CommandOutput::Message(body) => {
                assert!(body.contains("Timeline:"));
                assert!(body.contains("Transcript preview:"));
                assert!(body.contains("Output tail:"));
                assert!(body.contains("attempt 2 (retry of task-0)"));
            }
            _ => panic!("expected message"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
