use anyhow::Context;
use anyhow::Result;
use async_trait::async_trait;
use serde_json::json;
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
            .map(std::path::PathBuf::from);

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
            description: description.clone(),
            subagent_type,
            model,
            run_in_background,
            isolation,
            cwd,
            allowed_tools,
        };

        match runner.run_sub_agent(prompt, options).await {
            Ok(result) => {
                let artifact_path = if !run_in_background {
                    ctx.working_dir
                        .as_deref()
                        .and_then(|dir| persist_sub_agent_artifact(dir, &description, &result).ok())
                        .map(|path| path.display().to_string())
                } else {
                    None
                };

                Ok(ToolResult::success_with_metadata(
                    if let Some(path) = &artifact_path {
                        format!(
                            "{}\n\nSub-agent artifact: {}",
                            result,
                            path
                        )
                    } else {
                        result
                    },
                    json!({
                        "description": description,
                        "run_in_background": run_in_background,
                        "subagent_artifact_path": artifact_path,
                    }),
                ))
            }
            Err(e) => Ok(ToolResult::error(format!("Sub-agent failed: {}", e))),
        }
    }
}

fn persist_sub_agent_artifact(
    working_dir: &std::path::Path,
    description: &str,
    body: &str,
) -> Result<std::path::PathBuf> {
    let dir = working_dir.join(".yode").join("agent-results");
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("Failed to create agent artifact dir: {}", dir.display()))?;

    let timestamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let path = dir.join(format!("agent-{}.md", timestamp));
    let content = format!(
        "# Sub-Agent Result\n\n- Description: {}\n- Timestamp: {}\n\n## Result\n\n```text\n{}\n```\n",
        description,
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
        body.trim()
    );
    std::fs::write(&path, content)
        .with_context(|| format!("Failed to write sub-agent artifact: {}", path.display()))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::AgentTool;
    use crate::tool::{SubAgentOptions, SubAgentRunner, Tool, ToolContext};
    use serde_json::json;
    use std::pin::Pin;
    use std::sync::Arc;

    struct MockRunner;

    impl SubAgentRunner for MockRunner {
        fn run_sub_agent(
            &self,
            _prompt: String,
            _options: SubAgentOptions,
        ) -> Pin<Box<dyn std::future::Future<Output = anyhow::Result<String>> + Send + '_>> {
            Box::pin(async { Ok("sub-agent done".to_string()) })
        }
    }

    #[tokio::test]
    async fn agent_tool_persists_sync_artifact() {
        let dir = tempfile::tempdir().unwrap();
        let mut ctx = ToolContext::empty();
        ctx.working_dir = Some(dir.path().to_path_buf());
        ctx.sub_agent_runner = Some(Arc::new(MockRunner));

        let tool = AgentTool;
        let result = tool
            .execute(
                json!({
                    "description": "inspect code",
                    "prompt": "inspect the workspace",
                    "run_in_background": false
                }),
                &ctx,
            )
            .await
            .unwrap();

        assert!(!result.is_error);
        let artifact_path = result
            .metadata
            .as_ref()
            .and_then(|meta| meta.get("subagent_artifact_path"))
            .and_then(|value| value.as_str())
            .unwrap();
        assert!(artifact_path.contains(".yode/agent-results/agent-"));
        assert!(std::path::Path::new(artifact_path).exists());
        assert!(result.content.contains("Sub-agent artifact:"));
    }
}
