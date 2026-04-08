use yode_core::EffortLevel;

use crate::commands::context::CommandContext;
use crate::commands::{
    ArgCompletionSource, ArgDef, Command, CommandCategory, CommandMeta, CommandOutput,
    CommandResult,
};

pub struct EffortCommand {
    meta: CommandMeta,
}

impl EffortCommand {
    pub fn new() -> Self {
        Self {
            meta: CommandMeta {
                name: "effort",
                description: "Show or set the effort level",
                aliases: &[],
                args: vec![ArgDef {
                    name: "level".into(),
                    required: false,
                    hint: "min|low|medium|high|max".into(),
                    completions: ArgCompletionSource::Static(vec![
                        "min".into(),
                        "low".into(),
                        "medium".into(),
                        "high".into(),
                        "max".into(),
                    ]),
                }],
                category: CommandCategory::Model,
                hidden: false,
            },
        }
    }
}

impl Command for EffortCommand {
    fn meta(&self) -> &CommandMeta {
        &self.meta
    }

    fn execute(&self, args: &str, ctx: &mut CommandContext) -> CommandResult {
        if args.is_empty() {
            // Show current effort level
            if let Ok(eng) = ctx.engine.try_lock() {
                Ok(CommandOutput::Message(format!(
                    "Current effort level: {}",
                    eng.effort()
                )))
            } else {
                Err("Could not acquire engine lock".into())
            }
        } else {
            // Set effort level
            match args.parse::<EffortLevel>() {
                Ok(level) => {
                    if let Ok(mut eng) = ctx.engine.try_lock() {
                        eng.set_effort(level);
                    }
                    Ok(CommandOutput::Message(format!(
                        "Effort level set to: {}",
                        level
                    )))
                }
                Err(e) => Err(e),
            }
        }
    }
}
