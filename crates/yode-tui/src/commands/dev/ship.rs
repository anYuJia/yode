use crate::commands::context::CommandContext;
use crate::commands::{
    ArgCompletionSource, ArgDef, Command, CommandCategory, CommandMeta, CommandOutput,
    CommandResult,
};

pub struct ShipCommand {
    meta: CommandMeta,
}

impl ShipCommand {
    pub fn new() -> Self {
        Self {
            meta: CommandMeta {
                name: "ship",
                description: "Prefill a review_then_commit prompt into the input box",
                aliases: &[],
                args: vec![ArgDef {
                    name: "message".to_string(),
                    required: false,
                    hint: "[commit message]".to_string(),
                    completions: ArgCompletionSource::None,
                }],
                category: CommandCategory::Development,
                hidden: false,
            },
        }
    }
}

impl Command for ShipCommand {
    fn meta(&self) -> &CommandMeta {
        &self.meta
    }

    fn execute(&self, args: &str, ctx: &mut CommandContext) -> CommandResult {
        let message = if args.trim().is_empty() {
            "describe the current change".to_string()
        } else {
            args.trim().to_string()
        };
        ctx.input.set_text(&format!(
            "Use `review_then_commit` with message=\"{}\" and focus=\"current workspace changes\". If review finds issues, stop and summarize them instead of committing.",
            message
        ));
        Ok(CommandOutput::Message(
            "Loaded a review-then-commit prompt into the input box.".to_string(),
        ))
    }
}
