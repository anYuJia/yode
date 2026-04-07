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

    fn user_facing_name(&self) -> &str {
        "Sub-Agent"
    }

    fn activity_description(&self, params: &Value) -> String {
        let desc = params
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("complex task");
        format!("Sub-agent working on: {}", desc)
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
                "description": {
                    "type": "string",
                    "description": "A short (3-5 word) description of the task"
                },
                "prompt": {
                    "type": "string",
                    "description": "The task for the agent to perform"
                },
                "subagent_type": {
                    "type": "string",
                    "description": "The type of specialized agent to use for this task (e.g. 'plan', 'explore', 'verification')"
                },
                "model": {
                    "type": "string",
                    "enum": ["sonnet", "opus", "haiku"],
                    "description": "Optional model override for this agent. If omitted, inherits from the parent."
                },
                "run_in_background": {
                    "type": "boolean",
                    "description": "Set to true to run this agent in the background. You will be notified when it completes."
                },
                "isolation": {
                    "type": "string",
                    "enum": ["worktree"],
                    "description": "Isolation mode. 'worktree' creates a temporary git worktree so the agent works on an isolated copy of the repo."
                },
                "cwd": {
                    "type": "string",
                    "description": "Absolute path to run the agent in. Overrides the working directory for all operations within this agent."
                }
            },
            "required": ["prompt", "description"]
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
            .ok_or_else(|| anyhow::anyhow!("'prompt' parameter is required"))?
            .to_string();

        let description = params
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("complex task")
            .to_string();

        let subagent_type = params
            .get("subagent_type")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let model = params
            .get("model")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let run_in_background = params
            .get("run_in_background")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let isolation = params
            .get("isolation")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let cwd = params
            .get("cwd")
            .and_then(|v| v.as_str())
            .map(|s| std::path::PathBuf::from(s));

        let allowed_tools: Vec<String> = params
            .get("allowed_tools") // Backward compatibility for any internal calls
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

        let options = crate::tool::SubAgentOptions {
            description,
            subagent_type,
            model,
            run_in_background,
            isolation,
            cwd,
            allowed_tools,
        };

        match runner.run_sub_agent(prompt, options).await {
            Ok(result) => Ok(ToolResult::success(result)),
            Err(e) => Ok(ToolResult::error(format!("Sub-agent failed: {}", e))),
        }
    }
}
