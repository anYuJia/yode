use crate::commands::context::CommandContext;
use crate::commands::{ArgCompletionSource, ArgDef, Command, CommandCategory, CommandMeta, CommandOutput, CommandResult};

pub struct ReviewCommand {
    meta: CommandMeta,
}

impl ReviewCommand {
    pub fn new() -> Self {
        Self {
            meta: CommandMeta {
                name: "review",
                description: "Prefill a verification-agent review prompt into the input box",
                aliases: &[],
                args: vec![ArgDef {
                    name: "focus".to_string(),
                    required: false,
                    hint: "[focus]".to_string(),
                    completions: ArgCompletionSource::Static(vec![
                        "tests".to_string(),
                        "changed files".to_string(),
                        "runtime behavior".to_string(),
                        "regressions".to_string(),
                    ]),
                }],
                category: CommandCategory::Development,
                hidden: false,
            },
        }
    }
}

impl Command for ReviewCommand {
    fn meta(&self) -> &CommandMeta {
        &self.meta
    }

    fn execute(&self, args: &str, ctx: &mut CommandContext) -> CommandResult {
        let focus = if args.trim().is_empty() {
            "current changes".to_string()
        } else {
            args.trim().to_string()
        };
        let prompt = format!(
            "Use `verification_agent` to review the current workspace changes.\nGoal: verify the implementation is correct and identify regressions.\nFocus: {}.\nReport findings first, ordered by severity. If no issues are found, state that explicitly and mention residual risk or missing coverage.",
            focus
        );
        ctx.input.set_text(&prompt);
        Ok(CommandOutput::Message(
            "Loaded a verification-agent review prompt into the input box.".to_string(),
        ))
    }
}
