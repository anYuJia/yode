mod execution;
mod rendering;
#[cfg(test)]
mod tests;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};

use crate::tool::{Tool, ToolCapabilities, ToolContext, ToolResult};

use self::execution::execute_task_output;

pub struct TaskOutputTool;

#[async_trait]
impl Tool for TaskOutputTool {
    fn name(&self) -> &str {
        "task_output"
    }

    fn user_facing_name(&self) -> &str {
        "Task Output"
    }

    fn activity_description(&self, params: &Value) -> String {
        let task_id = params
            .get("task_id")
            .and_then(|value| value.as_str())
            .unwrap_or("latest");
        format!("Reading task output: {}", task_id)
    }

    fn description(&self) -> &str {
        "Read output from a background runtime task started earlier by bash or agent. Use this instead of polling with bash."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "task_id": {
                    "type": "string",
                    "description": "Task ID from /tasks or prior tool metadata. If omitted, uses the latest runtime task."
                },
                "offset": {
                    "type": "integer",
                    "description": "Line number to start from (1-based, inclusive). Defaults to the last 200 lines."
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of lines to return. Defaults to 200."
                },
                "follow": {
                    "type": "boolean",
                    "default": false,
                    "description": "If true and the task is still running, wait until it finishes or timeout_secs elapses before reading output."
                },
                "timeout_secs": {
                    "type": "integer",
                    "default": 60,
                    "description": "Maximum seconds to wait when follow=true."
                }
            }
        })
    }

    fn capabilities(&self) -> ToolCapabilities {
        ToolCapabilities {
            requires_confirmation: false,
            supports_auto_execution: true,
            read_only: true,
        }
    }

    async fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolResult> {
        execute_task_output(params, ctx).await
    }
}
