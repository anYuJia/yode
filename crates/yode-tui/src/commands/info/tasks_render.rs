use crate::commands::{CommandOutput, CommandResult};
use crate::commands::workspace_nav::{
    runtime_operator_jump_targets, task_jump_targets, workspace_breadcrumb,
    workspace_jump_inventory, workspace_selection_summary,
};
use crate::commands::workspace_text::{
    workspace_artifact_lines, workspace_bullets, workspace_preview_line, WorkspaceText,
};

use super::task_runtime_workspace::{
    grouped_task_runtime_summary, runtime_freshness_banner, task_follow_prompt,
    task_issue_template, task_notification_summary,
};
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
    let mut lines = vec![format!("Runtime tasks ({})", tasks.len())];
    for (source_tool, grouped_tasks) in group_tasks_by_source_tool(tasks) {
        lines.push(format!("Source tool: {} ({})", source_tool, grouped_tasks.len()));
        for task in grouped_tasks {
            lines.push(format!(
                "{} [{}:{}] {} @ {} / {} / {}",
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
    let transcript_preview = task_transcript_preview(&task)
        .unwrap_or_else(|| "none".to_string());
    Ok(CommandOutput::Message(
        WorkspaceText::new(format!("Task workspace {}", task.id))
            .subtitle(task.description.clone())
            .field(
                "Breadcrumb",
                workspace_breadcrumb("Tasks", Some(task.id.as_str())),
            )
            .field("Selection", workspace_selection_summary(1, 1))
            .field("Kind", task.kind.clone())
            .field("Source tool", task.source_tool.clone())
            .field("Status", task_status_label(&task.status))
            .field("Retry chain", task_retry_chain_summary(&task))
            .field("Freshest", task_latest_activity_at(&task))
            .field("Failure", task_failure_cause_summary(&task))
            .field("Artifacts", task_artifact_backlink_summary(&task))
            .field("Output", task.output_path.clone())
            .field("Transcript", task.transcript_path.as_deref().unwrap_or("none"))
            .section("Timeline", workspace_bullets(task_timeline_lines(&task)))
            .section(
                "Transcript preview",
                workspace_bullets([workspace_preview_line("Preview", Some(&transcript_preview))]),
            )
            .section(
                "Recent progress",
                if progress_history == "none" {
                    workspace_bullets(["none"])
                } else {
                    progress_history
                        .lines()
                        .map(|line| line.trim_start_matches("    - ").to_string())
                        .collect()
                },
            )
            .section(
                "Output tail",
                workspace_bullets([
                    format!("lines {}-{} of {}", preview_start, total_lines, total_lines),
                    output_preview,
                ]),
            )
            .footer(workspace_jump_inventory(task_jump_targets(
                &task.id,
                task.transcript_path.as_deref(),
            )))
            .render(),
    ))
}

pub(super) fn render_task_output(task: &yode_tools::RuntimeTask) -> CommandResult {
    let (output_preview, preview_start, total_lines) = task_output_preview(task, 40);
    let transcript_preview = task_transcript_preview(task)
        .unwrap_or_else(|| "none".to_string());
    Ok(CommandOutput::Message(
        WorkspaceText::new(format!("Task output {}", task.id))
            .subtitle(task.description.clone())
            .field(
                "Breadcrumb",
                workspace_breadcrumb("Tasks", Some(task.id.as_str())),
            )
            .field(
                "Status",
                format!(
                    "{} [{}:{}]",
                    task_status_label(&task.status),
                    task.kind,
                    task.source_tool
                ),
            )
            .field("Retry chain", task_retry_chain_summary(task))
            .field("Freshest", task_latest_activity_at(task))
            .field("Failure", task_failure_cause_summary(task))
            .field("Output path", task.output_path.clone())
            .field("Transcript", task.transcript_path.as_deref().unwrap_or("none"))
            .section(
                "Artifacts",
                workspace_artifact_lines([
                    ("output", task.output_path.clone()),
                    (
                        "transcript",
                        task.transcript_path
                            .as_deref()
                            .unwrap_or("none")
                            .to_string(),
                    ),
                ]),
            )
            .section("Timeline", workspace_bullets(task_timeline_lines(task)))
            .section(
                "Transcript preview",
                workspace_bullets([workspace_preview_line("Preview", Some(&transcript_preview))]),
            )
            .section(
                "Output tail",
                workspace_bullets([
                    format!("lines {}-{} of {}", preview_start, total_lines, total_lines),
                    output_preview,
                ]),
            )
            .footer(workspace_jump_inventory(task_jump_targets(
                &task.id,
                task.transcript_path.as_deref(),
            )))
            .render(),
    ))
}

pub(super) fn render_task_notifications(tasks: Vec<yode_tools::RuntimeTask>) -> CommandResult {
    Ok(CommandOutput::Message(
        WorkspaceText::new("Task notifications")
            .section("Recent outcomes", workspace_bullets(task_notification_summary(&tasks)))
            .render(),
    ))
}

pub(super) fn render_task_summary(
    tasks: Vec<yode_tools::RuntimeTask>,
    runtime: Option<&yode_core::engine::EngineRuntimeState>,
) -> CommandResult {
    if tasks.is_empty() {
        return Ok(CommandOutput::Message(
            "No background runtime tasks recorded.".to_string(),
        ));
    }

    let total = tasks.len();
    let running = tasks
        .iter()
        .filter(|task| matches!(task.status, yode_tools::RuntimeTaskStatus::Running))
        .count();
    let failed = tasks
        .iter()
        .filter(|task| matches!(task.status, yode_tools::RuntimeTaskStatus::Failed))
        .count();
    let latest_artifact = tasks
        .iter()
        .find_map(|task| task.transcript_path.as_deref().or(Some(task.output_path.as_str())));

    Ok(CommandOutput::Message(
        WorkspaceText::new("Task runtime workspace")
            .field("Total", total.to_string())
            .field("Running", running.to_string())
            .field("Failed", failed.to_string())
            .field("Freshness", runtime_freshness_banner(&tasks, runtime))
            .section("By kind", workspace_bullets(grouped_task_runtime_summary(&tasks)))
            .section("Notifications", workspace_bullets(task_notification_summary(&tasks)))
            .footer(workspace_jump_inventory(runtime_operator_jump_targets(
                latest_artifact,
            )))
            .render(),
    ))
}

pub(super) fn render_task_issue(task: &yode_tools::RuntimeTask) -> CommandResult {
    Ok(CommandOutput::Message(task_issue_template(task)))
}

pub(super) fn build_task_follow_prompt(task_id: &str) -> String {
    task_follow_prompt(task_id)
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
