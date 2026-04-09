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
        let engine = ctx.engine.clone();
        let event_tx = ctx.engine_event_tx.clone();

        tokio::spawn(async move {
            let mut engine = engine.lock().await;
            let changed = engine.force_compact(event_tx.clone()).await;
            if !changed {
                let _ = event_tx.send(yode_core::engine::EngineEvent::Error(
                    "Compaction made no changes. Current session is too short or already below the compaction target, so no transcript was written.".to_string(),
                ));
            }
        });

        Ok(CommandOutput::Message("Compaction requested.".to_string()))
    }
}
