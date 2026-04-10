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
                        "ship-all".to_string(),
                        "ship-staged".to_string(),
                        "export-gh".to_string(),
                        "staged".to_string(),
                        "all".to_string(),
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
        let parts = args.split_whitespace().collect::<Vec<_>>();
        if let ["export-gh", name @ ..] = parts.as_slice() {
            let file_name = if name.is_empty() {
                "yode-review-gate.yml".to_string()
            } else {
                let raw = name.join("-");
                if raw.ends_with(".yml") || raw.ends_with(".yaml") {
                    raw
                } else {
                    format!("{}.yml", raw)
                }
            };
            let workflow_dir = std::path::PathBuf::from(&ctx.session.working_dir)
                .join(".github")
                .join("workflows");
            std::fs::create_dir_all(&workflow_dir).map_err(|err| {
                format!(
                    "Failed to create GitHub workflow directory {}: {}",
                    workflow_dir.display(),
                    err
                )
            })?;
            let path = workflow_dir.join(&file_name);
            std::fs::write(&path, github_review_gate_workflow())
                .map_err(|err| format!("Failed to write {}: {}", path.display(), err))?;
            return Ok(CommandOutput::Message(format!(
                "Exported GitHub review gate scaffold to {}.\nCustomize the prompt, test command, and secret names before enabling it in CI.",
                path.display()
            )));
        }
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
        ["ship-all", message @ ..] => {
            let message = if message.is_empty() {
                "describe the current change".to_string()
            } else {
                message.join(" ")
            };
            format!(
                "Use `review_pipeline` with focus=\"all tracked workspace changes\", commit_message=\"{}\", and all=true. Commit only when review, verification, and any requested tests are clean.",
                message
            )
        }
        ["ship-staged", message @ ..] => {
            let message = if message.is_empty() {
                "describe the staged change".to_string()
            } else {
                message.join(" ")
            };
            format!(
                "Use `review_pipeline` with focus=\"staged changes only\" and commit_message=\"{}\". Do not set all=true and do not add extra files; commit only if the staged review and verification are clean.",
                message
            )
        }
        ["staged", focus @ ..] => {
            let focus = if focus.is_empty() {
                "staged changes only".to_string()
            } else {
                format!("staged changes: {}", focus.join(" "))
            };
            format!(
                "Use `review_pipeline` with focus=\"{}\". Do not set all=true and do not add extra files.",
                focus
            )
        }
        ["all", focus @ ..] => {
            let focus = if focus.is_empty() {
                "all tracked workspace changes".to_string()
            } else {
                format!("all tracked workspace changes: {}", focus.join(" "))
            };
            format!(
                "Use `review_pipeline` with focus=\"{}\". If a commit is requested, set all=true so all tracked modified files are staged.",
                focus
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

fn github_review_gate_workflow() -> &'static str {
    r#"name: Yode Review Gate

on:
  pull_request:

jobs:
  review:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v5

      - name: Install Yode
        run: curl -fsSL https://raw.githubusercontent.com/anYuJia/yode/main/install.sh | bash

      - name: Run review pipeline
        env:
          ANTHROPIC_API_KEY: ${{ secrets.ANTHROPIC_API_KEY }}
        run: |
          yode --chat "Use review_pipeline with focus=\"pull request changes\" and test_command=\"cargo test\". Report findings first, stay concise, and do not commit." > yode-review.txt

      - name: Upload review artifacts
        if: always()
        uses: actions/upload-artifact@v4
        with:
          name: yode-review
          path: |
            yode-review.txt
            .yode/reviews/*
"#
}

#[cfg(test)]
mod tests {
    use super::{build_pipeline_prompt, github_review_gate_workflow};

    #[test]
    fn pipeline_prompt_supports_ship_preset() {
        let prompt = build_pipeline_prompt("ship release 0.2.1");
        assert!(prompt.contains("commit_message=\"release 0.2.1\""));
        assert!(prompt.contains("review_pipeline"));
    }

    #[test]
    fn pipeline_prompt_supports_ship_all_preset() {
        let prompt = build_pipeline_prompt("ship-all release 0.2.1");
        assert!(prompt.contains("commit_message=\"release 0.2.1\""));
        assert!(prompt.contains("all=true"));
    }

    #[test]
    fn pipeline_prompt_supports_staged_preset() {
        let prompt = build_pipeline_prompt("staged release notes");
        assert!(prompt.contains("focus=\"staged changes: release notes\""));
        assert!(prompt.contains("Do not set all=true"));
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

    #[test]
    fn github_review_gate_template_mentions_review_pipeline() {
        let workflow = github_review_gate_workflow();
        assert!(workflow.contains("review_pipeline"));
        assert!(workflow.contains("upload-artifact"));
    }
}
