use crate::commands::context::CommandContext;
use crate::commands::{Command, CommandCategory, CommandMeta, CommandOutput, CommandResult};

pub struct HooksCommand {
    meta: CommandMeta,
}

impl HooksCommand {
    pub fn new() -> Self {
        Self {
            meta: CommandMeta {
                name: "hooks",
                description: "Show hook runtime diagnostics",
                aliases: &[],
                args: vec![],
                category: CommandCategory::Info,
                hidden: false,
            },
        }
    }
}

impl Command for HooksCommand {
    fn meta(&self) -> &CommandMeta {
        &self.meta
    }

    fn execute(&self, _args: &str, ctx: &mut CommandContext) -> CommandResult {
        let runtime = ctx
            .engine
            .try_lock()
            .ok()
            .map(|engine| engine.runtime_state());

        let Some(state) = runtime else {
            return Ok(CommandOutput::Message(
                "Hook diagnostics unavailable: engine busy.".to_string(),
            ));
        };

        Ok(CommandOutput::Message(format!(
            "Hook diagnostics:\n  Total runs:      {}\n  Wake notices:    {}\n  Timeouts:        {}\n  Exec errors:     {}\n  Non-zero exits:  {}\n  Last failure:    {}\n  Failed at:       {}\n  Last timeout:    {}",
            state.hook_total_executions,
            state.hook_wake_notification_count,
            state.hook_timeout_count,
            state.hook_execution_error_count,
            state.hook_nonzero_exit_count,
            state
                .last_hook_failure_command
                .as_ref()
                .map(|command| {
                    format!(
                        "{} [{}]: {}",
                        command,
                        state
                            .last_hook_failure_event
                            .as_deref()
                            .unwrap_or("unknown"),
                        state
                            .last_hook_failure_reason
                            .as_deref()
                            .unwrap_or("unknown")
                    )
                })
                .unwrap_or_else(|| "none".to_string()),
            state.last_hook_failure_at.as_deref().unwrap_or("none"),
            state.last_hook_timeout_command.as_deref().unwrap_or("none"),
        )))
    }
}
