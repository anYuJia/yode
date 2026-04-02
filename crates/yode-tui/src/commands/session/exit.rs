use crate::commands::context::CommandContext;
use crate::commands::{Command, CommandCategory, CommandMeta, CommandOutput, CommandResult};

pub struct ExitCommand {
    meta: CommandMeta,
}

impl ExitCommand {
    pub fn new() -> Self {
        Self {
            meta: CommandMeta {
                name: "exit",
                description: "Exit Yode",
                aliases: &["quit", "q"],
                args: vec![],
                category: CommandCategory::Session,
                hidden: false,
            },
        }
    }
}

impl Command for ExitCommand {
    fn meta(&self) -> &CommandMeta {
        &self.meta
    }

    fn execute(&self, _args: &str, ctx: &mut CommandContext) -> CommandResult {
        *ctx.should_quit = true;
        Ok(CommandOutput::Silent)
    }
}
