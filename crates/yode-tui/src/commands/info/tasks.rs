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
                        hint: "<task-id|stop>".into(),
                        completions: ArgCompletionSource::None,
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
            [id] => {
                let Some(task) = engine.runtime_task_snapshot(id) else {
                    return Err(format!("Task '{}' not found.", id));
                };
                Ok(CommandOutput::Message(format!(
                    "Task {}:\n  Kind:        {}\n  Source tool: {}\n  Status:      {:?}\n  Description: {}\n  Created:     {}\n  Started:     {}\n  Completed:   {}\n  Progress:    {}\n  Error:       {}\n  Output:      {}",
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
                )))
            }
            _ => Err("Usage: /tasks | /tasks <task-id> | /tasks stop <task-id>".into()),
        }
    }
}
