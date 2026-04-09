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
                description: "Prefill review or verification agent prompts into the input box",
                aliases: &[],
                args: vec![
                    ArgDef {
                        name: "mode".to_string(),
                        required: false,
                        hint: "[background|verify|focus]".to_string(),
                        completions: ArgCompletionSource::Static(vec![
                            "background".to_string(),
                            "verify".to_string(),
                            "tests".to_string(),
                            "changed files".to_string(),
                            "runtime behavior".to_string(),
                            "regressions".to_string(),
                        ]),
                    },
                    ArgDef {
                        name: "value".to_string(),
                        required: false,
                        hint: "[focus|goal]".to_string(),
                        completions: ArgCompletionSource::None,
                    },
                ],
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
        let trimmed = args.trim();
        let parts = trimmed.split_whitespace().collect::<Vec<_>>();

        match parts.as_slice() {
            ["background", focus @ ..] => {
                let focus = if focus.is_empty() {
                    "current changes".to_string()
                } else {
                    focus.join(" ")
                };
                ctx.input.set_text(&format!(
                    "Use `review_changes` with focus=\"{}\" and run_in_background=true. Report findings first when the task finishes.",
                    focus
                ));
                Ok(CommandOutput::Message(
                    "Loaded a background review-agent prompt into the input box.".to_string(),
                ))
            }
            ["verify", goal @ ..] => {
                let goal = if goal.is_empty() {
                    "verify the current implementation is correct".to_string()
                } else {
                    goal.join(" ")
                };
                ctx.input.set_text(&format!(
                    "Use `verification_agent` with goal=\"{}\" and focus=\"current workspace changes\". Report findings first, ordered by severity.",
                    goal
                ));
                Ok(CommandOutput::Message(
                    "Loaded a verification-agent prompt into the input box.".to_string(),
                ))
            }
            _ => {
                let focus = if trimmed.is_empty() {
                    "current changes".to_string()
                } else {
                    trimmed.to_string()
                };
                ctx.input.set_text(&format!(
                    "Use `review_changes` to review the current workspace changes.\nFocus: {}.\nReport findings first, ordered by severity. If no issues are found, state that explicitly and mention residual risk or missing coverage.",
                    focus
                ));
                Ok(CommandOutput::Message(
                    "Loaded a review-agent prompt into the input box.".to_string(),
                ))
            }
        }
    }
}
