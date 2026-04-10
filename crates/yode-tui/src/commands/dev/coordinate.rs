use crate::commands::context::CommandContext;
use crate::commands::{
    ArgCompletionSource, ArgDef, Command, CommandCategory, CommandMeta, CommandOutput,
    CommandResult,
};

pub struct CoordinateCommand {
    meta: CommandMeta,
}

impl CoordinateCommand {
    pub fn new() -> Self {
        Self {
            meta: CommandMeta {
                name: "coordinate",
                description: "Prefill a coordinate_agents prompt into the input box",
                aliases: &[],
                args: vec![ArgDef {
                    name: "goal".to_string(),
                    required: false,
                    hint: "[goal]".to_string(),
                    completions: ArgCompletionSource::None,
                }],
                category: CommandCategory::Development,
                hidden: false,
            },
        }
    }
}

impl Command for CoordinateCommand {
    fn meta(&self) -> &CommandMeta {
        &self.meta
    }

    fn execute(&self, args: &str, ctx: &mut CommandContext) -> CommandResult {
        let goal = if args.trim().is_empty() {
            "complete the current task via multiple independent workstreams".to_string()
        } else {
            args.trim().to_string()
        };
        ctx.input.set_text(&format!(
            "Use `coordinate_agents` for goal=\"{}\" with 2-3 independent workstreams. If you need to preview execution order first, call it with dry_run=true. Use max_parallel if too many workstreams are ready in the same phase. Ask one workstream to inspect code, one to verify behavior, and one to summarize risks if useful.",
            goal
        ));
        Ok(CommandOutput::Message(
            "Loaded a coordinator-agent prompt into the input box.".to_string(),
        ))
    }
}
