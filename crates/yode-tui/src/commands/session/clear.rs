use crate::commands::context::CommandContext;
use crate::commands::{ArgCompletionSource, ArgDef, Command, CommandCategory, CommandMeta, CommandOutput, CommandResult};

pub struct ClearCommand {
    meta: CommandMeta,
}

impl ClearCommand {
    pub fn new() -> Self {
        Self {
            meta: CommandMeta {
                name: "clear",
                description: "Clear chat history",
                aliases: &[],
                args: vec![ArgDef {
                    name: "context".to_string(),
                    required: false,
                    hint: "[context]".to_string(),
                    completions: ArgCompletionSource::Static(vec!["context".to_string()]),
                }],
                category: CommandCategory::Session,
                hidden: false,
            },
        }
    }
}

impl Command for ClearCommand {
    fn meta(&self) -> &CommandMeta {
        &self.meta
    }

    fn execute(&self, _args: &str, ctx: &mut CommandContext) -> CommandResult {
        ctx.chat_entries.clear();
        Ok(CommandOutput::Message("Chat history cleared.".to_string()))
    }
}
