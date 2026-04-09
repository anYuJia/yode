use crate::commands::context::CommandContext;
use crate::commands::{
    ArgCompletionSource, ArgDef, Command, CommandCategory, CommandMeta, CommandOutput,
    CommandResult,
};

pub struct PipelineCommand {
    meta: CommandMeta,
}

impl PipelineCommand {
    pub fn new() -> Self {
        Self {
            meta: CommandMeta {
                name: "pipeline",
                description: "Prefill a review_pipeline prompt into the input box",
                aliases: &[],
                args: vec![ArgDef {
                    name: "focus".to_string(),
                    required: false,
                    hint: "[focus]".to_string(),
                    completions: ArgCompletionSource::Static(vec![
                        "current changes".to_string(),
                        "regressions".to_string(),
                        "tests".to_string(),
                    ]),
                }],
                category: CommandCategory::Development,
                hidden: false,
            },
        }
    }
}

impl Command for PipelineCommand {
    fn meta(&self) -> &CommandMeta {
        &self.meta
    }

    fn execute(&self, args: &str, ctx: &mut CommandContext) -> CommandResult {
        let focus = if args.trim().is_empty() {
            "current changes".to_string()
        } else {
            args.trim().to_string()
        };
        ctx.input.set_text(&format!(
            "Use `review_pipeline` with focus=\"{}\". Include commit_message if you want the pipeline to commit only when review and verification are clean. Add test_command if a targeted test step is needed.",
            focus
        ));
        Ok(CommandOutput::Message(
            "Loaded a review-pipeline prompt into the input box.".to_string(),
        ))
    }
}
