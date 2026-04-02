use crate::commands::{
    Command, CommandCategory, CommandMeta, CommandOutput, CommandResult,
};
use crate::commands::context::CommandContext;

pub struct ProvidersCommand {
    meta: CommandMeta,
}

impl ProvidersCommand {
    pub fn new() -> Self {
        Self {
            meta: CommandMeta {
                name: "providers",
                description: "List all available providers and their models",
                aliases: &[],
                args: vec![],
                category: CommandCategory::Model,
                hidden: false,
            },
        }
    }
}

impl Command for ProvidersCommand {
    fn meta(&self) -> &CommandMeta {
        &self.meta
    }

    fn execute(&self, _args: &str, ctx: &mut CommandContext) -> CommandResult {
        let mut lines = String::from("Available providers:\n");
        for (name, models) in ctx.all_provider_models {
            let marker = if *name == *ctx.provider_name {
                " *"
            } else {
                "  "
            };
            let model_str = if models.is_empty() {
                "(unrestricted)".to_string()
            } else {
                models.join(", ")
            };
            lines.push_str(&format!("{} {:<15} — {}\n", marker, name, model_str));
        }
        Ok(CommandOutput::Message(lines))
    }
}
