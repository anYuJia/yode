use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};

use crate::builtin::review_common::{persist_review_artifact, review_findings_count};
use crate::tool::{SubAgentOptions, Tool, ToolCapabilities, ToolContext, ToolResult};

pub struct VerificationAgentTool;

#[async_trait]
impl Tool for VerificationAgentTool {
    fn name(&self) -> &str {
        "verification_agent"
    }

    fn user_facing_name(&self) -> &str {
        "Verification Agent"
    }

    fn activity_description(&self, params: &Value) -> String {
        let focus = params
            .get("focus")
            .and_then(|value| value.as_str())
            .unwrap_or("changes");
        format!("Verifying {}", focus)
    }

    fn description(&self) -> &str {
        "Launch a dedicated verification sub-agent to inspect current changes, run validation steps, and report risks or regressions."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "goal": {
                    "type": "string",
                    "description": "What the implementation was supposed to achieve."
                },
                "focus": {
                    "type": "string",
                    "description": "Specific area to verify, such as tests, runtime behavior, or changed files."
                },
                "instructions": {
                    "type": "string",
                    "description": "Optional extra instructions for the verification pass."
                },
                "run_in_background": {
                    "type": "boolean",
                    "default": false,
                    "description": "Whether to run the verification agent in the background."
                }
            },
            "required": ["goal"]
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
        let goal = params
            .get("goal")
            .and_then(|value| value.as_str())
            .ok_or_else(|| anyhow::anyhow!("'goal' parameter is required"))?;
        let focus = params
            .get("focus")
            .and_then(|value| value.as_str())
            .unwrap_or("current changes");
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
            "You are a dedicated verification agent.\n\nGoal:\n{}\n\nFocus:\n{}\n\nInstructions:\n{}\n\nRequirements:\n- Inspect the current workspace state before concluding.\n- Prefer targeted read-only inspection first.\n- Run validation commands when they materially improve confidence.\n- Report findings first, ordered by severity.\n- If no issues are found, say so explicitly and mention any residual risk or missing coverage.",
            goal,
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
                    description: format!("verify {}", focus),
                    subagent_type: Some("verification".to_string()),
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
                .and_then(|dir| persist_review_artifact(dir, "verification", focus, &result).ok())
                .map(|path| path.display().to_string())
        } else {
            None
        };
        let findings_count = review_findings_count(&result);

        Ok(ToolResult::success_with_metadata(
            result,
            json!({
                "goal": goal,
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
    use super::VerificationAgentTool;
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
            Box::pin(async { Ok("verification ok".to_string()) })
        }
    }

    #[tokio::test]
    async fn verification_agent_uses_sub_agent_runner() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let mut ctx = ToolContext::empty();
        ctx.sub_agent_runner = Some(Arc::new(MockRunner {
            seen: Arc::clone(&seen),
        }));

        let tool = VerificationAgentTool;
        let result = tool
            .execute(
                serde_json::json!({
                    "goal": "verify a bug fix",
                    "focus": "tests",
                    "run_in_background": true
                }),
                &ctx,
            )
            .await
            .unwrap();

        assert!(!result.is_error);
        assert!(result.content.contains("verification ok"));
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 1);
        assert!(seen[0].0.contains("verify a bug fix"));
        assert!(seen[0].1.allowed_tools.iter().any(|tool| tool == "task_output"));
        assert!(seen[0].1.run_in_background);
    }
}
