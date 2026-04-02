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

    fn execute(&self, _args: &str, ctx: &mut CommandContext) -> CommandResult {
        let categories = ctx.cmd_registry.by_category();
        let mut help = String::from("Available commands:\n");

        for (category, commands) in &categories {
            help.push_str(&format!("\n  {}:\n", category.label()));
            for cmd in commands {
                let meta = cmd.meta();
                let aliases = if meta.aliases.is_empty() {
                    String::new()
                } else {
                    format!(" ({})", meta.aliases.iter().map(|a| format!("/{}", a)).collect::<Vec<_>>().join(", "))
                };
                help.push_str(&format!("    /{:<14} {}{}\n", meta.name, meta.description, aliases));
            }
        }

        help.push_str("\nType /keys for keyboard shortcut reference.");
        Ok(CommandOutput::Message(help))
    }
}
