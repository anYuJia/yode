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
                        "ship".to_string(),
                        "test".to_string(),
                        "verify".to_string(),
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
        ctx.input.set_text(&build_pipeline_prompt(args));
        Ok(CommandOutput::Message(
            "Loaded a review-pipeline prompt into the input box.".to_string(),
        ))
    }
}

fn build_pipeline_prompt(args: &str) -> String {
    let trimmed = args.trim();
    let parts = trimmed.split_whitespace().collect::<Vec<_>>();
    match parts.as_slice() {
        [] => {
            "Use `review_pipeline` with focus=\"current changes\". Include commit_message if you want the pipeline to commit only when review and verification are clean. Add test_command if a targeted test step is needed.".to_string()
        }
        ["ship", message @ ..] => {
            let message = if message.is_empty() {
                "describe the current change".to_string()
            } else {
                message.join(" ")
            };
            format!(
                "Use `review_pipeline` with focus=\"current workspace changes\" and commit_message=\"{}\". Commit only when review, verification, and any requested tests are clean.",
                message
            )
        }
        ["test", command @ ..] => {
            let command = if command.is_empty() {
                "cargo test".to_string()
            } else {
                command.join(" ")
            };
            format!(
                "Use `review_pipeline` with focus=\"current changes\" and test_command=\"{}\". Stop on review, verification, or test findings.",
                command
            )
        }
        ["verify", focus @ ..] => {
            let focus = if focus.is_empty() {
                "current changes".to_string()
            } else {
                focus.join(" ")
            };
            format!(
                "Use `review_pipeline` with focus=\"{}\" and verification_goal=\"verify the implementation is correct\". Do not commit; report findings first.",
                focus
            )
        }
        _ => {
            let focus = trimmed.to_string();
            format!(
                "Use `review_pipeline` with focus=\"{}\". Include commit_message if you want the pipeline to commit only when review and verification are clean. Add test_command if a targeted test step is needed.",
                focus
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::build_pipeline_prompt;

    #[test]
    fn pipeline_prompt_supports_ship_preset() {
        let prompt = build_pipeline_prompt("ship release 0.2.1");
        assert!(prompt.contains("commit_message=\"release 0.2.1\""));
        assert!(prompt.contains("review_pipeline"));
    }

    #[test]
    fn pipeline_prompt_supports_test_preset() {
        let prompt = build_pipeline_prompt("test cargo test -p yode-tools");
        assert!(prompt.contains("test_command=\"cargo test -p yode-tools\""));
    }

    #[test]
    fn pipeline_prompt_supports_verify_preset() {
        let prompt = build_pipeline_prompt("verify regressions");
        assert!(prompt.contains("verification_goal=\"verify the implementation is correct\""));
        assert!(prompt.contains("focus=\"regressions\""));
    }
}
