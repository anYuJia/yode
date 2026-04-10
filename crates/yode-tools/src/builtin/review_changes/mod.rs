use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};

use crate::builtin::review_common::{
    persist_review_artifact, persist_review_status, review_findings_count,
};
use crate::tool::{SubAgentOptions, Tool, ToolCapabilities, ToolContext, ToolResult};

pub struct ReviewChangesTool;

#[async_trait]
impl Tool for ReviewChangesTool {
    fn name(&self) -> &str {
        "review_changes"
    }

    fn user_facing_name(&self) -> &str {
        "Review Changes"
    }

    fn activity_description(&self, params: &Value) -> String {
        let focus = params
            .get("focus")
            .and_then(|value| value.as_str())
            .unwrap_or("workspace changes");
        format!("Reviewing {}", focus)
    }

    fn description(&self) -> &str {
        "Launch a review agent focused on the current workspace changes. The agent inspects git state, changed files, and likely regressions, then reports findings first."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "focus": {
                    "type": "string",
                    "description": "Specific review focus, such as tests, regressions, or changed files."
                },
                "instructions": {
                    "type": "string",
                    "description": "Optional extra review instructions."
                },
                "run_in_background": {
                    "type": "boolean",
                    "default": false,
                    "description": "Whether to run the review in the background."
                }
            }
        })
    }

    fn capabilities(&self) -> ToolCapabilities {
        ToolCapabilities {
            requires_confirmation: false,
            supports_auto_execution: true,
            read_only: false,
        }
    }

    async fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolResult> {
        let focus = params
            .get("focus")
            .and_then(|value| value.as_str())
            .unwrap_or("current workspace changes");
        let instructions = params
            .get("instructions")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        let run_in_background = params
            .get("run_in_background")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);

        let runner = ctx
            .sub_agent_runner
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Sub-agent runner not available"))?;

        let prompt = format!(
            "You are a dedicated review agent for current workspace changes.\n\nFocus:\n{}\n\nInstructions:\n{}\n\nReview protocol:\n- Start by checking repository state and changed files.\n- Use git_status / git_diff / read_file / grep / project_map to understand the change set.\n- Run targeted validation commands when they materially improve confidence.\n- Findings must come first, ordered by severity.\n- Focus on bugs, regressions, risky assumptions, and missing tests.\n- If no issues are found, say so explicitly and mention residual risk or missing coverage.",
            focus,
            if instructions.is_empty() {
                "No extra instructions."
            } else {
                instructions
            }
        );

        let result = runner
            .run_sub_agent(
                prompt,
                SubAgentOptions {
                    description: format!("review {}", focus),
                    subagent_type: Some("review".to_string()),
                    model: None,
                    run_in_background,
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
                        "task_output".to_string(),
                    ],
                },
            )
            .await?;

        let artifact_path = if !run_in_background {
            ctx.working_dir
                .as_deref()
                .and_then(|dir| persist_review_artifact(dir, "review", focus, &result).ok())
                .inspect(|path| {
                    let _ = ctx
                        .working_dir
                        .as_deref()
                        .and_then(|dir| {
                            persist_review_status(dir, "review", focus, &result, Some(path)).ok()
                        });
                })
                .map(|path| path.display().to_string())
        } else {
            None
        };
        let findings_count = review_findings_count(&result);

        Ok(ToolResult::success_with_metadata(
            result,
            json!({
                "focus": focus,
                "run_in_background": run_in_background,
                "findings_count": findings_count,
                "review_artifact_path": artifact_path,
            }),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::ReviewChangesTool;
    use crate::tool::{SubAgentOptions, SubAgentRunner, Tool, ToolContext};
    use std::pin::Pin;
    use std::sync::{Arc, Mutex};

    struct MockRunner {
        seen: Arc<Mutex<Vec<(String, SubAgentOptions)>>>,
    }

    impl SubAgentRunner for MockRunner {
        fn run_sub_agent(
            &self,
            prompt: String,
            options: SubAgentOptions,
        ) -> Pin<Box<dyn std::future::Future<Output = anyhow::Result<String>> + Send + '_>> {
            self.seen.lock().unwrap().push((prompt, options));
            Box::pin(async { Ok("review ok".to_string()) })
        }
    }

    #[tokio::test]
    async fn review_changes_uses_sub_agent_runner() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let mut ctx = ToolContext::empty();
        ctx.sub_agent_runner = Some(Arc::new(MockRunner {
            seen: Arc::clone(&seen),
        }));

        let tool = ReviewChangesTool;
        let result = tool
            .execute(
                serde_json::json!({
                    "focus": "regressions",
                    "run_in_background": false
                }),
                &ctx,
            )
            .await
            .unwrap();

        assert!(!result.is_error);
        assert!(result.content.contains("review ok"));
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 1);
        assert!(seen[0].0.contains("review agent"));
        assert!(seen[0].1.allowed_tools.iter().any(|tool| tool == "git_diff"));
    }
}
