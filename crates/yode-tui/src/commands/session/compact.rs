use crate::commands::context::CommandContext;
use crate::commands::{
    ArgCompletionSource, ArgDef, Command, CommandCategory, CommandMeta, CommandOutput,
    CommandResult,
};

pub struct CompactCommand {
    meta: CommandMeta,
}

impl CompactCommand {
    pub fn new() -> Self {
        Self {
            meta: CommandMeta {
                name: "compact",
                description: "Compact chat history to recent entries",
                aliases: &[],
                args: vec![ArgDef {
                    name: "keep_last".to_string(),
                    required: false,
                    hint: "[keep_last=20]".to_string(),
                    completions: ArgCompletionSource::None,
                }],
                category: CommandCategory::Session,
                hidden: false,
            },
        }
    }
}

impl Command for CompactCommand {
    fn meta(&self) -> &CommandMeta {
        &self.meta
    }

    fn execute(&self, _args: &str, ctx: &mut CommandContext) -> CommandResult {
        if ctx.chat_entries.len() > 20 {
            let start = ctx.chat_entries.len() - 20;
            *ctx.chat_entries = ctx.chat_entries[start..].to_vec();
        }
        Ok(CommandOutput::Message("History compacted.".to_string()))
    }
}
