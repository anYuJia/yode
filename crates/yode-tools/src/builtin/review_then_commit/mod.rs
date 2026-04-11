mod execution;
#[cfg(test)]
mod tests;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};

use crate::tool::{Tool, ToolCapabilities, ToolContext, ToolResult};

use self::execution::execute_review_then_commit;

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
        format!(
            "Reviewing and committing: {}",
            message.lines().next().unwrap_or(message)
        )
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
        execute_review_then_commit(params, ctx).await
    }
}
