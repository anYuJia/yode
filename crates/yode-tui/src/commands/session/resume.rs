use crate::commands::context::CommandContext;
use crate::commands::{Command, CommandCategory, CommandMeta, CommandOutput, CommandResult};

pub struct ResumeCommand {
    meta: CommandMeta,
}

impl ResumeCommand {
    pub fn new() -> Self {
        Self {
            meta: CommandMeta {
                name: "resume",
                description: "Show or prepare a session resume command",
                aliases: &[],
                args: Vec::new(),
                category: CommandCategory::Session,
                hidden: false,
            },
        }
    }
}

impl Command for ResumeCommand {
    fn meta(&self) -> &CommandMeta {
        &self.meta
    }

    fn execute(&self, args: &str, ctx: &mut CommandContext<'_>) -> CommandResult {
        let session_id = args.trim();
        if session_id.is_empty() {
            return super::SessionsCommand::new().execute("", ctx);
        }

        let command = format!("yode --resume {}", session_id);
        Ok(CommandOutput::Message(format!(
            "Resume this session from a new shell with:\n  {}",
            command
        )))
    }
}
