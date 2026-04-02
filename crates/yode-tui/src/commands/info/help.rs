use crate::commands::context::CommandContext;
use crate::commands::{Command, CommandCategory, CommandMeta, CommandOutput, CommandResult};

pub struct HelpCommand {
    meta: CommandMeta,
}

impl HelpCommand {
    pub fn new() -> Self {
        Self {
            meta: CommandMeta {
                name: "help",
                description: "Show available commands",
                aliases: &["?"],
                args: vec![],
                category: CommandCategory::Info,
                hidden: false,
            },
        }
    }
}

impl Command for HelpCommand {
    fn meta(&self) -> &CommandMeta {
        &self.meta
    }

    fn execute(&self, _args: &str, _ctx: &mut CommandContext) -> CommandResult {
        // TODO(Task 10): Once the CommandRegistry is wired into CommandContext,
        // iterate registry.by_category() and use CommandCategory::label() for headers.
        Ok(CommandOutput::Message(
            "Use /help after integration (Task 10)".to_string(),
        ))
    }
}
