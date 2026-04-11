use crate::commands::{CommandOutput, CommandResult};

use super::tasks::TaskFilter;

pub(super) fn render_task_list(tasks: Vec<yode_tools::RuntimeTask>) -> CommandResult {
    if tasks.is_empty() {
        return Ok(CommandOutput::Message(
            "No background runtime tasks recorded.".to_string(),
        ));
    }
    let mut lines = vec![format!("Runtime tasks ({}):", tasks.len())];
    for task in tasks {
        lines.push(format!(
            "  {} [{}:{}] {}{}{}",
            task.id,
            task.kind,
            match task.status {
                yode_tools::RuntimeTaskStatus::Pending => "pending",
                yode_tools::RuntimeTaskStatus::Running => "running",
                yode_tools::RuntimeTaskStatus::Completed => "completed",
                yode_tools::RuntimeTaskStatus::Failed => "failed",
                yode_tools::RuntimeTaskStatus::Cancelled => "cancelled",
            },
            task.description,
            if task.attempt > 1 {
                format!(
                    " (attempt {}, retry of {})",
                    task.attempt,
                    task.retry_of.as_deref().unwrap_or("unknown")
                )
            } else {
                String::new()
            },
            task.last_progress
                .as_ref()
                .map(|progress| format!(" — {}", progress))
                .unwrap_or_default()
        ));
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
    let output_preview = std::fs::read_to_string(&task.output_path)
        .ok()
        .map(|content| {
            let lines = content.lines().collect::<Vec<_>>();
            let preview_start = lines.len().saturating_sub(8);
            lines[preview_start..].join("\n")
        })
        .unwrap_or_else(|| "(unavailable)".to_string());
    let progress_history = if task.progress_history.is_empty() {
        "none".to_string()
    } else {
        task.progress_history
            .iter()
            .map(|progress| format!("    - {}", progress))
            .collect::<Vec<_>>()
            .join("\n")
    };
    Ok(CommandOutput::Message(format!(
        "Task {}:\n  Kind:        {}\n  Source tool: {}\n  Status:      {:?}\n  Description: {}\n  Attempt:     {}{}\n  Created:     {}\n  Started:     {}\n  Completed:   {}\n  Progress:    {}\n  Progress at: {}\n  Error:       {}\n  Output:      {}\n  Transcript:  {}\n  Recent progress:\n{}\n\n  Output preview:\n{}\n\nUse `/tasks read {}` for the full tail.",
        task.id,
        task.kind,
        task.source_tool,
        task.status,
        task.description,
        task.attempt,
        task
            .retry_of
            .as_ref()
            .map(|id| format!(" (retry of {})", id))
            .unwrap_or_default(),
        task.created_at,
        task.started_at.as_deref().unwrap_or("none"),
        task.completed_at.as_deref().unwrap_or("none"),
        task.last_progress.as_deref().unwrap_or("none"),
        task.last_progress_at.as_deref().unwrap_or("none"),
        task.error.as_deref().unwrap_or("none"),
        task.output_path,
        task.transcript_path.as_deref().unwrap_or("none"),
        progress_history,
        output_preview,
        task.id,
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
