use crate::commands::context::CommandContext;
use crate::commands::{Command, CommandCategory, CommandMeta, CommandOutput, CommandResult};

pub struct TimeCommand {
    meta: CommandMeta,
}

impl TimeCommand {
    pub fn new() -> Self {
        Self {
            meta: CommandMeta {
                name: "time",
                description: "Show session timing info",
                aliases: &[],
                args: vec![],
                category: CommandCategory::Utility,
                hidden: false,
            },
        }
    }
}

impl Command for TimeCommand {
    fn meta(&self) -> &CommandMeta {
        &self.meta
    }

    fn execute(&self, _args: &str, ctx: &mut CommandContext) -> CommandResult {
        let elapsed = ctx.session_start.elapsed();
        let hours = elapsed.as_secs() / 3600;
        let mins = (elapsed.as_secs() % 3600) / 60;
        let secs = elapsed.as_secs() % 60;
        let turn_info = if let Some(turn_start) = ctx.turn_started_at {
            let turn_elapsed = turn_start.elapsed();
            format!("\n  Current turn:    {}s", turn_elapsed.as_secs())
        } else {
            String::new()
        };
        Ok(CommandOutput::Message(format!(
            "Session timing:\n  Session duration: {}h {:02}m {:02}s\n  Messages:        {}\n  Tool calls:      {}{}",
            hours,
            mins,
            secs,
            ctx.chat_entries.len(),
            ctx.session.tool_call_count,
            turn_info,
        )))
    }
}
