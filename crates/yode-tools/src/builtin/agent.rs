use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;

use crate::tool::{Tool, ToolCapabilities, ToolContext, ToolResult};

pub struct AgentTool;

#[async_trait]
impl Tool for AgentTool {
    fn name(&self) -> &str {
        "agent"
    }

    fn description(&self) -> &str {
        "Launch a sub-agent to handle a complex task autonomously. The sub-agent runs with \
         its own conversation history and a subset of available tools. Use this for tasks \
         that benefit from independent exploration or parallel work."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "prompt": {
                    "type": "string",
                    "description": "The task for the sub-agent to perform"
                },
                "description": {
                    "type": "string",
                    "description": "A short (3-5 word) description of the task"
                },
                "allowed_tools": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "List of tool names the sub-agent is allowed to use. If empty, all tools are available."
                }
            },
            "required": ["prompt"]
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
        let prompt = params
            .get("prompt")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("'prompt' parameter is required"))?;

        let allowed_tools: Vec<String> = params
            .get("allowed_tools")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        let runner = ctx
            .sub_agent_runner
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Sub-agent runner not available"))?;

        match runner.run_sub_agent(prompt.to_string(), allowed_tools).await {
            Ok(result) => Ok(ToolResult::success(result)),
            Err(e) => Ok(ToolResult::error(format!("Sub-agent failed: {}", e))),
        }
    }
}
