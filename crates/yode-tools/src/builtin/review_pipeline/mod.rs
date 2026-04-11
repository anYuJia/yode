mod execution;
#[cfg(test)]
mod tests;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};

use crate::tool::{Tool, ToolCapabilities, ToolContext, ToolResult};

use self::execution::execute_review_pipeline;

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
        execute_review_pipeline(params, ctx).await
    }
}
