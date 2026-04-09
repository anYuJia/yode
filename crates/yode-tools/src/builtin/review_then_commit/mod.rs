use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};

use crate::builtin::git_commit::GitCommitTool;
use crate::builtin::review_common::{persist_review_artifact, review_output_has_findings};
use crate::tool::{SubAgentOptions, Tool, ToolCapabilities, ToolContext, ToolErrorType, ToolResult};

pub struct ReviewThenCommitTool;

#[async_trait]
impl Tool for ReviewThenCommitTool {
    fn name(&self) -> &str {
        "review_then_commit"
    }

    fn user_facing_name(&self) -> &str {
        "Review Then Commit"
    }

    fn activity_description(&self, params: &Value) -> String {
        let message = params
            .get("message")
            .and_then(|value| value.as_str())
            .unwrap_or("commit");
        format!("Reviewing and committing: {}", message.lines().next().unwrap_or(message))
    }

    fn description(&self) -> &str {
        "Run a review agent on current changes and commit only if the review appears clean, unless explicitly overridden."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "message": {
                    "type": "string",
                    "description": "Git commit message to use if review passes."
                },
                "focus": {
                    "type": "string",
                    "description": "Review focus, such as regressions or tests."
                },
                "instructions": {
                    "type": "string",
                    "description": "Optional extra review instructions."
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
                    "description": "If true, commit even when the review output contains findings."
                }
            },
            "required": ["message"]
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
        let message = params
            .get("message")
            .and_then(|value| value.as_str())
            .ok_or_else(|| anyhow::anyhow!("'message' parameter is required"))?;
        let focus = params
            .get("focus")
            .and_then(|value| value.as_str())
            .unwrap_or("current workspace changes");
        let instructions = params
            .get("instructions")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        let allow_findings_commit = params
            .get("allow_findings_commit")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);

        let runner = ctx
            .sub_agent_runner
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Sub-agent runner not available"))?;

        let review_prompt = format!(
            "You are a dedicated review agent for current workspace changes.\n\nFocus:\n{}\n\nInstructions:\n{}\n\nReview protocol:\n- Check repository state and changed files first.\n- Focus on bugs, regressions, risky assumptions, and missing tests.\n- Findings must come first, ordered by severity.\n- If no issues are found, say exactly 'No issues found.' on the first line.\n- Keep the review concise but specific.",
            focus,
            if instructions.is_empty() {
                "No extra instructions."
            } else {
                instructions
            }
        );

        let review_output = runner
            .run_sub_agent(
                review_prompt,
                SubAgentOptions {
                    description: format!("review before commit {}", focus),
                    subagent_type: Some("review".to_string()),
                    model: None,
                    run_in_background: false,
                    isolation: None,
                    cwd: None,
                    allowed_tools: vec![
                        "read_file".to_string(),
                        "glob".to_string(),
                        "grep".to_string(),
                        "ls".to_string(),
                        "git_status".to_string(),
                        "git_diff".to_string(),
                        "git_log".to_string(),
                        "project_map".to_string(),
                        "test_runner".to_string(),
                        "bash".to_string(),
                    ],
                },
            )
            .await?;

        let artifact_path = ctx
            .working_dir
            .as_deref()
            .and_then(|dir| persist_review_artifact(dir, "pre-commit-review", focus, &review_output).ok())
            .map(|path| path.display().to_string());

        if review_output_has_findings(&review_output) && !allow_findings_commit {
            return Ok(ToolResult {
                content: format!(
                    "Review detected findings. Commit aborted.\n\nReview output:\n{}\n\nReview artifact: {}",
                    review_output,
                    artifact_path.as_deref().unwrap_or("none")
                ),
                is_error: true,
                error_type: Some(ToolErrorType::Validation),
                recoverable: true,
                suggestion: Some(
                    "Address the review findings first, or set allow_findings_commit=true if you intentionally want to override."
                        .to_string(),
                ),
                metadata: Some(json!({
                    "review_output": review_output,
                    "review_artifact_path": artifact_path,
                    "commit_skipped": true,
                })),
            });
        }

        let commit_tool = GitCommitTool;
        let commit_result = commit_tool
            .execute(
                json!({
                    "message": message,
                    "files": params.get("files").cloned().unwrap_or_else(|| Value::Array(Vec::new())),
                    "all": params.get("all").cloned().unwrap_or_else(|| Value::Bool(false)),
                }),
                ctx,
            )
            .await?;

        let mut metadata = commit_result
            .metadata
            .clone()
            .unwrap_or_else(|| json!({}));
        if let Some(object) = metadata.as_object_mut() {
            object.insert("review_output".to_string(), json!(review_output));
            object.insert("review_artifact_path".to_string(), json!(artifact_path));
        }

        Ok(ToolResult {
            content: format!(
                "Review passed.\n\n{}\n\nReview artifact: {}",
                commit_result.content,
                artifact_path.as_deref().unwrap_or("none")
            ),
            is_error: commit_result.is_error,
            error_type: commit_result.error_type,
            recoverable: commit_result.recoverable,
            suggestion: commit_result.suggestion,
            metadata: Some(metadata),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::ReviewThenCommitTool;
    use crate::builtin::review_common::review_output_has_findings;
    use serde_json::json;
    use crate::tool::{SubAgentOptions, SubAgentRunner, Tool, ToolContext};
    use std::pin::Pin;
    use std::process::Command;
    use std::sync::Arc;

    struct MockRunner {
        output: String,
    }

    impl SubAgentRunner for MockRunner {
        fn run_sub_agent(
            &self,
            _prompt: String,
            _options: SubAgentOptions,
        ) -> Pin<Box<dyn std::future::Future<Output = anyhow::Result<String>> + Send + '_>> {
            let output = self.output.clone();
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

    #[test]
    fn review_findings_heuristic_respects_clean_output() {
        assert!(!review_output_has_findings(
            "No issues found.\nResidual risk: none."
        ));
        assert!(review_output_has_findings("1. Missing test for edge case"));
    }

    #[tokio::test]
    async fn review_then_commit_commits_when_review_is_clean() {
        let dir = tempfile::tempdir().unwrap();
        init_repo(dir.path());
        std::fs::write(dir.path().join("a.txt"), "hello").unwrap();

        let mut ctx = ToolContext::empty();
        ctx.working_dir = Some(dir.path().to_path_buf());
        ctx.sub_agent_runner = Some(Arc::new(MockRunner {
            output: "No issues found.\nResidual risk: none.".to_string(),
        }));

        let tool = ReviewThenCommitTool;
        let result = tool
            .execute(
                json!({
                    "message": "add a.txt",
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
    async fn review_then_commit_aborts_on_findings() {
        let dir = tempfile::tempdir().unwrap();
        init_repo(dir.path());
        std::fs::write(dir.path().join("a.txt"), "hello").unwrap();

        let mut ctx = ToolContext::empty();
        ctx.working_dir = Some(dir.path().to_path_buf());
        ctx.sub_agent_runner = Some(Arc::new(MockRunner {
            output: "1. Missing regression test".to_string(),
        }));

        let tool = ReviewThenCommitTool;
        let result = tool
            .execute(
                json!({
                    "message": "add a.txt",
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
