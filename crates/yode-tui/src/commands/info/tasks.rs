use crate::commands::context::CommandContext;
use crate::commands::{
    ArgCompletionSource, ArgDef, Command, CommandCategory, CommandMeta, CommandOutput,
    CommandResult,
};
use super::tasks_render::{
    parse_task_filter, render_task_detail, render_task_list, task_matches_filter,
};

pub struct TasksCommand {
    meta: CommandMeta,
}

#[derive(Debug, Clone)]
pub(super) enum TaskFilter {
    Status(yode_tools::RuntimeTaskStatus),
    Kind(&'static str),
}

impl TasksCommand {
    pub fn new() -> Self {
        Self {
            meta: CommandMeta {
                name: "tasks",
                description: "List, inspect, or cancel background runtime tasks",
                aliases: &[],
                args: vec![
                    ArgDef {
                        name: "task".into(),
                        required: false,
                        hint: "<task-id|stop|read|list|latest>".into(),
                        completions: ArgCompletionSource::Static(vec![
                            "stop".to_string(),
                            "read".to_string(),
                            "follow".to_string(),
                            "list".to_string(),
                            "latest".to_string(),
                            "running".to_string(),
                            "failed".to_string(),
                            "completed".to_string(),
                            "cancelled".to_string(),
                            "pending".to_string(),
                            "bash".to_string(),
                            "agent".to_string(),
                        ]),
                    },
                    ArgDef {
                        name: "task-id".into(),
                        required: false,
                        hint: "<task-id|latest>".into(),
                        completions: ArgCompletionSource::Static(vec!["latest".to_string()]),
                    },
                ],
                category: CommandCategory::Info,
                hidden: false,
            },
        }
    }
}

impl Command for TasksCommand {
    fn meta(&self) -> &CommandMeta {
        &self.meta
    }

    fn execute(&self, args: &str, ctx: &mut CommandContext) -> CommandResult {
        let parts = args.split_whitespace().collect::<Vec<_>>();
        let Ok(engine) = ctx.engine.try_lock() else {
            return Err("Engine is busy, try again.".into());
        };
        let latest_task_id = || {
            engine
                .runtime_tasks_snapshot()
                .into_iter()
                .last()
                .map(|task| task.id)
        };
        let latest_task_id_by_filter = |filter: TaskFilter| {
            engine
                .runtime_tasks_snapshot()
                .into_iter()
                .filter(|task| task_matches_filter(task, &filter))
                .last()
                .map(|task| task.id)
        };

        match parts.as_slice() {
            [] | ["list"] => render_task_list(engine.runtime_tasks_snapshot()),
            ["list", filter] => {
                let filter = parse_task_filter(filter)
                    .ok_or_else(|| format!("Unknown task filter '{}'.", filter))?;
                let tasks = engine
                    .runtime_tasks_snapshot()
                    .into_iter()
                    .filter(|task| task_matches_filter(task, &filter))
                    .collect::<Vec<_>>();
                render_task_list(tasks)
            }
            [filter] if parse_task_filter(filter).is_some() => {
                let filter = parse_task_filter(filter).unwrap();
                let tasks = engine
                    .runtime_tasks_snapshot()
                    .into_iter()
                    .filter(|task| task_matches_filter(task, &filter))
                    .collect::<Vec<_>>();
                render_task_list(tasks)
            }
            ["latest"] => {
                let id = latest_task_id().ok_or_else(|| "No runtime task available.".to_string())?;
                render_task_detail(&engine, &id)
            }
            ["latest", filter] => {
                let filter = parse_task_filter(filter)
                    .ok_or_else(|| format!("Unknown task filter '{}'.", filter))?;
                let id = latest_task_id_by_filter(filter)
                    .ok_or_else(|| "No runtime task matched that filter.".to_string())?;
                render_task_detail(&engine, &id)
            }
            ["stop", id] => {
                let id = if *id == "latest" {
                    latest_task_id().ok_or_else(|| "No runtime task available.".to_string())?
                } else {
                    id.to_string()
                };
                if engine.cancel_runtime_task(&id) {
                    Ok(CommandOutput::Message(format!(
                        "Cancellation requested for task {}.",
                        id
                    )))
                } else {
                    Err(format!("Task '{}' not found or cannot be cancelled.", id))
                }
            }
            ["read", id] => {
                let id = if *id == "latest" {
                    latest_task_id().ok_or_else(|| "No runtime task available.".to_string())?
                } else {
                    id.to_string()
                };
                let Some(task) = engine.runtime_task_snapshot(&id) else {
                    return Err(format!("Task '{}' not found.", id));
                };
                let content = std::fs::read_to_string(&task.output_path).map_err(|err| {
                    format!(
                        "Failed to read task output {} ({}): {}",
                        task.id, task.output_path, err
                    )
                })?;
                let lines = content.lines().collect::<Vec<_>>();
                let preview_start = lines.len().saturating_sub(40);
                let preview = lines[preview_start..].join("\n");
                Ok(CommandOutput::Message(format!(
                    "Task output {}\nPath: {}\nTranscript: {}\nShowing lines {}-{} of {}\n\n{}",
                    task.id,
                    task.output_path,
                    task.transcript_path.as_deref().unwrap_or("none"),
                    preview_start + 1,
                    lines.len(),
                    lines.len(),
                    preview
                )))
            }
            ["follow", id] => {
                let id = if *id == "latest" {
                    latest_task_id().ok_or_else(|| "No runtime task available.".to_string())?
                } else {
                    id.to_string()
                };
                ctx.input.set_text(&format!(
                    "Use `task_output` with task_id=\"{}\", follow=true, and timeout_secs=120. Summarize the final status and important output.",
                    id
                ));
                Ok(CommandOutput::Message(format!(
                    "Loaded a task_output follow prompt for task {}.",
                    id
                )))
            }
            [id] => render_task_detail(&engine, id),
            _ => Err("Usage: /tasks | /tasks list [filter] | /tasks latest [filter] | /tasks <task-id> | /tasks read <task-id> | /tasks follow <task-id> | /tasks stop <task-id>".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_task_filter, task_matches_filter, TaskFilter};

    #[test]
    fn task_filter_parses_status_and_kind() {
        assert!(matches!(
            parse_task_filter("running"),
            Some(TaskFilter::Status(yode_tools::RuntimeTaskStatus::Running))
        ));
        assert!(matches!(
            parse_task_filter("bash"),
            Some(TaskFilter::Kind("bash"))
        ));
        assert!(parse_task_filter("unknown").is_none());
    }

    #[test]
    fn task_filter_matches_runtime_tasks() {
        let task = yode_tools::RuntimeTask {
            id: "task-1".to_string(),
            kind: "bash".to_string(),
            source_tool: "bash".to_string(),
            description: "demo".to_string(),
            status: yode_tools::RuntimeTaskStatus::Running,
            attempt: 1,
            retry_of: None,
            output_path: "/tmp/task.log".to_string(),
            transcript_path: Some("/tmp/transcript.md".to_string()),
            created_at: "2026-01-01 00:00:00".to_string(),
            started_at: Some("2026-01-01 00:00:01".to_string()),
            completed_at: None,
            last_progress: Some("building".to_string()),
            last_progress_at: Some("2026-01-01 00:00:02".to_string()),
            progress_history: vec!["building".to_string()],
            error: None,
        };

        assert!(task_matches_filter(
            &task,
            &TaskFilter::Status(yode_tools::RuntimeTaskStatus::Running)
        ));
        assert!(task_matches_filter(&task, &TaskFilter::Kind("bash")));
        assert!(!task_matches_filter(
            &task,
            &TaskFilter::Status(yode_tools::RuntimeTaskStatus::Failed)
        ));
    }
}
