use crate::commands::context::CommandContext;
use crate::commands::{
    ArgCompletionSource, ArgDef, Command, CommandCategory, CommandMeta, CommandOutput,
    CommandResult,
};

pub struct TasksCommand {
    meta: CommandMeta,
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
                        hint: "<task-id|stop|read>".into(),
                        completions: ArgCompletionSource::Static(vec![
                            "stop".to_string(),
                            "read".to_string(),
                        ]),
                    },
                    ArgDef {
                        name: "task-id".into(),
                        required: false,
                        hint: "<task-id>".into(),
                        completions: ArgCompletionSource::None,
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

        match parts.as_slice() {
            [] => {
                let tasks = engine.runtime_tasks_snapshot();
                if tasks.is_empty() {
                    return Ok(CommandOutput::Message(
                        "No background runtime tasks recorded.".to_string(),
                    ));
                }
                let mut lines = vec![format!("Runtime tasks ({}):", tasks.len())];
                for task in tasks {
                    lines.push(format!(
                        "  {} [{}:{}] {}",
                        task.id,
                        task.kind,
                        match task.status {
                            yode_tools::RuntimeTaskStatus::Pending => "pending",
                            yode_tools::RuntimeTaskStatus::Running => "running",
                            yode_tools::RuntimeTaskStatus::Completed => "completed",
                            yode_tools::RuntimeTaskStatus::Failed => "failed",
                            yode_tools::RuntimeTaskStatus::Cancelled => "cancelled",
                        },
                        task.description
                    ));
                }
                Ok(CommandOutput::Messages(lines))
            }
            ["stop", id] => {
                if engine.cancel_runtime_task(id) {
                    Ok(CommandOutput::Message(format!(
                        "Cancellation requested for task {}.",
                        id
                    )))
                } else {
                    Err(format!("Task '{}' not found or cannot be cancelled.", id))
                }
            }
            ["read", id] => {
                let Some(task) = engine.runtime_task_snapshot(id) else {
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
                    "Task output {}\nPath: {}\nShowing lines {}-{} of {}\n\n{}",
                    task.id,
                    task.output_path,
                    preview_start + 1,
                    lines.len(),
                    lines.len(),
                    preview
                )))
            }
            [id] => {
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
                Ok(CommandOutput::Message(format!(
                    "Task {}:\n  Kind:        {}\n  Source tool: {}\n  Status:      {:?}\n  Description: {}\n  Created:     {}\n  Started:     {}\n  Completed:   {}\n  Progress:    {}\n  Error:       {}\n  Output:      {}\n\n  Output preview:\n{}\n\nUse `/tasks read {}` for the full tail.",
                    task.id,
                    task.kind,
                    task.source_tool,
                    task.status,
                    task.description,
                    task.created_at,
                    task.started_at.as_deref().unwrap_or("none"),
                    task.completed_at.as_deref().unwrap_or("none"),
                    task.last_progress.as_deref().unwrap_or("none"),
                    task.error.as_deref().unwrap_or("none"),
                    task.output_path,
                    output_preview,
                    task.id,
                )))
            }
            _ => Err("Usage: /tasks | /tasks <task-id> | /tasks read <task-id> | /tasks stop <task-id>".into()),
        }
    }
}
