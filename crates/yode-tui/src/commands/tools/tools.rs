use crate::commands::context::CommandContext;
use crate::commands::{Command, CommandCategory, CommandMeta, CommandOutput, CommandResult};

pub struct ToolsCommand {
    meta: CommandMeta,
}

impl ToolsCommand {
    pub fn new() -> Self {
        Self {
            meta: CommandMeta {
                name: "tools",
                description: "List registered tools",
                aliases: &[],
                args: vec![],
                category: CommandCategory::Tools,
                hidden: false,
            },
        }
    }
}
impl Command for ToolsCommand {
    fn meta(&self) -> &CommandMeta {
        &self.meta
    }

    fn execute(&self, _args: &str, ctx: &mut CommandContext<'_>) -> CommandResult {
        let defs = ctx.tools.definitions();
        let mut lines = vec![format!("Registered tools ({}):", defs.len())];
        for d in &defs {
            lines.push(format!("  {} — {}", d.name, d.description));
        }
        Ok(CommandOutput::Messages(lines))
    }
}
