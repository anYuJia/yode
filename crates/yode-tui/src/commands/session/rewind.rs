use crate::commands::context::CommandContext;
use crate::commands::{Command, CommandCategory, CommandMeta, CommandResult};

pub struct RewindCommand {
    meta: CommandMeta,
}

impl RewindCommand {
    pub fn new() -> Self {
        Self {
            meta: CommandMeta {
                name: "rewind",
                description: "Preview a safe rewind to a checkpoint",
                aliases: &[],
                args: Vec::new(),
                category: CommandCategory::Session,
                hidden: false,
            },
        }
    }
}

impl Command for RewindCommand {
    fn meta(&self) -> &CommandMeta {
        &self.meta
    }

    fn execute(&self, args: &str, ctx: &mut CommandContext<'_>) -> CommandResult {
        let target = if args.trim().is_empty() {
            "latest"
        } else {
            args.trim()
        };
        super::CheckpointCommand::new().execute(&format!("rewind {}", target), ctx)
    }
}
