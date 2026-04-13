use crate::commands::context::CommandContext;
use crate::commands::{
    ArgCompletionSource, ArgDef, Command, CommandCategory, CommandMeta, CommandOutput,
    CommandResult,
};
use super::coordinate_workspace::{coordinator_dry_run_prompt, write_coordinator_stub_artifact};

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
        ctx.input.set_text(&coordinator_dry_run_prompt(&goal));
        let artifact = write_coordinator_stub_artifact(
            std::path::Path::new(&ctx.session.working_dir),
            &ctx.session.session_id,
            &goal,
        );
        Ok(CommandOutput::Message(format!(
            "Loaded a coordinator-agent prompt into the input box.\nArtifact: {}",
            artifact.unwrap_or_else(|| "none".to_string())
        )))
    }
}
