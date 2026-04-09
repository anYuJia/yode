use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};

use crate::builtin::git_commit::GitCommitTool;
use crate::builtin::review_common::{persist_review_artifact, review_output_has_findings};
use crate::builtin::review_changes::ReviewChangesTool;
use crate::builtin::test_runner::TestRunnerTool;
use crate::builtin::verification_agent::VerificationAgentTool;
use crate::tool::{Tool, ToolCapabilities, ToolContext, ToolErrorType, ToolResult};

pub struct ReviewPipelineTool;

#[async_trait]
impl Tool for ReviewPipelineTool {
    fn name(&self) -> &str {
        "review_pipeline"
    }

    fn user_facing_name(&self) -> &str {
        "Review Pipeline"
    }

    fn activity_description(&self, params: &Value) -> String {
        let focus = params
            .get("focus")
            .and_then(|value| value.as_str())
            .unwrap_or("current workspace changes");
        format!("Running review pipeline for {}", focus)
    }

    fn description(&self) -> &str {
        "Run review, verification, optional test command, and optional commit as a single pipeline. The pipeline stays conservative and stops on findings by default."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "focus": {
                    "type": "string",
                    "description": "Review and verification focus."
                },
                "review_instructions": {
                    "type": "string",
                    "description": "Extra instructions for the review pass."
                },
                "verification_goal": {
                    "type": "string",
                    "description": "Goal text for the verification pass. Defaults to verifying the current implementation."
                },
                "verification_instructions": {
                    "type": "string",
                    "description": "Extra instructions for the verification pass."
                },
                "test_command": {
                    "type": "string",
                    "description": "Optional explicit test command to run between verification and commit."
                },
                "commit_message": {
                    "type": "string",
                    "description": "Optional commit message. If absent, the pipeline stops after review/verification/tests."
                },
                "files": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Optional files to stage before committing."
                },
                "all": {
                    "type": "boolean",
                    "default": false,
                    "description": "Whether to stage all tracked modified files when committing."
                },
                "allow_findings_commit": {
                    "type": "boolean",
                    "default": false,
                    "description": "If true, commit even when review or verification reports findings."
                }
            }
        })
    }

    fn capabilities(&self) -> ToolCapabilities {
        ToolCapabilities {
            requires_confirmation: true,
            supports_auto_execution: false,
            read_only: false,
        }
    }

    async fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolResult> {
        let focus = params
            .get("focus")
            .and_then(|value| value.as_str())
            .unwrap_or("current workspace changes");
        let review_instructions = params
            .get("review_instructions")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        let verification_goal = params
            .get("verification_goal")
            .and_then(|value| value.as_str())
            .unwrap_or("verify the current implementation is correct");
        let verification_instructions = params
            .get("verification_instructions")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        let test_command = params.get("test_command").and_then(|value| value.as_str());
        let commit_message = params.get("commit_message").and_then(|value| value.as_str());
        let allow_findings_commit = params
            .get("allow_findings_commit")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);

        let review_tool = ReviewChangesTool;
        let review_result = review_tool
            .execute(
                json!({
                    "focus": focus,
                    "instructions": review_instructions,
                    "run_in_background": false,
                }),
                ctx,
            )
            .await?;
        let review_output = review_result.content.clone();
        let review_failed = review_output_has_findings(&review_output);

        let verification_tool = VerificationAgentTool;
        let verification_result = verification_tool
            .execute(
                json!({
                    "goal": verification_goal,
                    "focus": focus,
                    "instructions": verification_instructions,
                    "run_in_background": false,
                }),
                ctx,
            )
            .await?;
        let verification_output = verification_result.content.clone();
        let verification_failed = review_output_has_findings(&verification_output);

        let mut test_result = None;
        if let Some(command) = test_command {
            let runner = TestRunnerTool;
            test_result = Some(
                runner
                    .execute(json!({ "command": command }), ctx)
                    .await
                    .unwrap_or_else(|err| ToolResult::error(format!("Test runner failed: {}", err))),
            );
        }

        let should_stop_for_findings =
            (review_failed || verification_failed) && !allow_findings_commit;
        let mut commit_result = None;
        if let Some(message) = commit_message {
            if !should_stop_for_findings {
                let commit_tool = GitCommitTool;
                commit_result = Some(
                    commit_tool
                        .execute(
                            json!({
                                "message": message,
                                "files": params.get("files").cloned().unwrap_or_else(|| Value::Array(Vec::new())),
                                "all": params.get("all").cloned().unwrap_or_else(|| Value::Bool(false)),
                            }),
                            ctx,
                        )
                        .await?,
                );
            }
        }

        let summary = format!(
            "Review:\n{}\n\nVerification:\n{}\n\nTests:\n{}\n\nCommit:\n{}",
            review_output,
            verification_output,
            test_result
                .as_ref()
                .map(|result| result.content.clone())
                .unwrap_or_else(|| "not run".to_string()),
            commit_result
                .as_ref()
                .map(|result| result.content.clone())
                .unwrap_or_else(|| {
                    if commit_message.is_some() && should_stop_for_findings {
                        "skipped due to findings".to_string()
                    } else {
                        "not requested".to_string()
                    }
                })
        );

        let pipeline_artifact = ctx
            .working_dir
            .as_deref()
            .and_then(|dir| persist_review_artifact(dir, "review-pipeline", focus, &summary).ok())
            .map(|path| path.display().to_string());

        if should_stop_for_findings {
            return Ok(ToolResult {
                content: format!(
                    "Review pipeline detected findings. Commit skipped.\n\n{}",
                    summary
                ),
                is_error: true,
                error_type: Some(ToolErrorType::Validation),
                recoverable: true,
                suggestion: Some(
                    "Address review or verification findings first, or set allow_findings_commit=true to override."
                        .to_string(),
                ),
                metadata: Some(json!({
                    "focus": focus,
                    "review_output": review_output,
                    "verification_output": verification_output,
                    "pipeline_artifact_path": pipeline_artifact,
                    "commit_skipped": true,
                })),
            });
        }

        Ok(ToolResult::success_with_metadata(
            format!("Review pipeline complete.\n\n{}", summary),
            json!({
                "focus": focus,
                "review_output": review_output,
                "verification_output": verification_output,
                "pipeline_artifact_path": pipeline_artifact,
                "test_ran": test_result.is_some(),
                "committed": commit_result.is_some(),
            }),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::ReviewPipelineTool;
    use crate::tool::{SubAgentOptions, SubAgentRunner, Tool, ToolContext};
    use serde_json::json;
    use std::pin::Pin;
    use std::process::Command;
    use std::sync::{Arc, Mutex};

    struct QueueRunner {
        outputs: Arc<Mutex<Vec<String>>>,
    }

    impl SubAgentRunner for QueueRunner {
        fn run_sub_agent(
            &self,
            _prompt: String,
            _options: SubAgentOptions,
        ) -> Pin<Box<dyn std::future::Future<Output = anyhow::Result<String>> + Send + '_>> {
            let output = self.outputs.lock().unwrap().remove(0);
            Box::pin(async move { Ok(output) })
        }
    }

    fn init_repo(dir: &std::path::Path) {
        Command::new("git")
            .args(["init"])
            .current_dir(dir)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(dir)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(dir)
            .output()
            .unwrap();
    }

    #[tokio::test]
    async fn review_pipeline_commits_when_review_and_verification_are_clean() {
        let dir = tempfile::tempdir().unwrap();
        init_repo(dir.path());
        std::fs::write(dir.path().join("a.txt"), "hello").unwrap();

        let mut ctx = ToolContext::empty();
        ctx.working_dir = Some(dir.path().to_path_buf());
        ctx.sub_agent_runner = Some(Arc::new(QueueRunner {
            outputs: Arc::new(Mutex::new(vec![
                "No issues found.\nResidual risk: none.".to_string(),
                "No issues found.\nResidual risk: none.".to_string(),
            ])),
        }));

        let tool = ReviewPipelineTool;
        let result = tool
            .execute(
                json!({
                    "focus": "current changes",
                    "commit_message": "add a.txt",
                    "files": ["a.txt"]
                }),
                &ctx,
            )
            .await
            .unwrap();

        assert!(!result.is_error, "{}", result.content);
        let log = Command::new("git")
            .args(["log", "--oneline"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        let log_str = String::from_utf8_lossy(&log.stdout);
        assert!(log_str.contains("add a.txt"));
    }

    #[tokio::test]
    async fn review_pipeline_stops_on_findings() {
        let dir = tempfile::tempdir().unwrap();
        init_repo(dir.path());
        std::fs::write(dir.path().join("a.txt"), "hello").unwrap();

        let mut ctx = ToolContext::empty();
        ctx.working_dir = Some(dir.path().to_path_buf());
        ctx.sub_agent_runner = Some(Arc::new(QueueRunner {
            outputs: Arc::new(Mutex::new(vec![
                "1. Missing regression test".to_string(),
                "No issues found.".to_string(),
            ])),
        }));

        let tool = ReviewPipelineTool;
        let result = tool
            .execute(
                json!({
                    "focus": "current changes",
                    "commit_message": "add a.txt",
                    "files": ["a.txt"]
                }),
                &ctx,
            )
            .await
            .unwrap();

        assert!(result.is_error);
        let log = Command::new("git")
            .args(["log", "--oneline"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        let log_str = String::from_utf8_lossy(&log.stdout);
        assert!(!log_str.contains("add a.txt"));
    }
}
