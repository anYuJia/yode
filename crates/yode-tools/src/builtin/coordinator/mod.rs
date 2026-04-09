use anyhow::Result;
use async_trait::async_trait;
use futures::future::join_all;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::tool::{SubAgentOptions, Tool, ToolCapabilities, ToolContext, ToolResult};

#[derive(Debug, Deserialize)]
struct Workstream {
    description: String,
    prompt: String,
    #[serde(default)]
    subagent_type: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    run_in_background: Option<bool>,
    #[serde(default)]
    allowed_tools: Vec<String>,
}

pub struct CoordinateAgentsTool;

#[async_trait]
impl Tool for CoordinateAgentsTool {
    fn name(&self) -> &str {
        "coordinate_agents"
    }

    fn user_facing_name(&self) -> &str {
        "Coordinator"
    }

    fn activity_description(&self, params: &Value) -> String {
        let count = params
            .get("workstreams")
            .and_then(|value| value.as_array())
            .map(|items| items.len())
            .unwrap_or(0);
        format!("Coordinating {} workstreams", count)
    }

    fn description(&self) -> &str {
        "Launch multiple sub-agents for independent workstreams and aggregate their results. This is a minimal coordinator-mode style helper."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "goal": {
                    "type": "string",
                    "description": "Overall goal that the coordinator is trying to achieve."
                },
                "workstreams": {
                    "type": "array",
                    "description": "Independent workstreams to delegate.",
                    "items": {
                        "type": "object",
                        "properties": {
                            "description": { "type": "string" },
                            "prompt": { "type": "string" },
                            "subagent_type": { "type": "string" },
                            "model": { "type": "string" },
                            "run_in_background": { "type": "boolean" },
                            "allowed_tools": {
                                "type": "array",
                                "items": { "type": "string" }
                            }
                        },
                        "required": ["description", "prompt"]
                    }
                }
            },
            "required": ["goal", "workstreams"]
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
            .ok_or_else(|| anyhow::anyhow!("'goal' parameter is required"))?
            .to_string();
        let workstreams: Vec<Workstream> = serde_json::from_value(
            params
                .get("workstreams")
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("'workstreams' parameter is required"))?,
        )?;

        if workstreams.is_empty() {
            return Ok(ToolResult::error("No workstreams provided.".to_string()));
        }

        let runner = ctx
            .sub_agent_runner
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Sub-agent runner not available"))?;

        let futures = workstreams.iter().map(|workstream| {
            let prompt = format!(
                "Coordinator goal:\n{}\n\nWorkstream:\n{}\n\nTask:\n{}",
                goal, workstream.description, workstream.prompt
            );
            runner.run_sub_agent(
                prompt,
                SubAgentOptions {
                    description: workstream.description.clone(),
                    subagent_type: workstream.subagent_type.clone(),
                    model: workstream.model.clone(),
                    run_in_background: workstream.run_in_background.unwrap_or(true),
                    isolation: None,
                    cwd: None,
                    allowed_tools: workstream.allowed_tools.clone(),
                },
            )
        });

        let results = join_all(futures).await;
        let rendered = results
            .into_iter()
            .enumerate()
            .map(|(index, result)| match result {
                Ok(output) => json!({
                    "index": index + 1,
                    "status": "ok",
                    "output": output,
                }),
                Err(err) => json!({
                    "index": index + 1,
                    "status": "error",
                    "output": format!("{}", err),
                }),
            })
            .collect::<Vec<_>>();

        Ok(ToolResult::success_with_metadata(
            serde_json::to_string_pretty(&rendered)?,
            json!({
                "goal": goal,
                "workstream_count": workstreams.len(),
                "results": rendered,
            }),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::CoordinateAgentsTool;
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
            Box::pin(async { Ok("done".to_string()) })
        }
    }

    #[tokio::test]
    async fn coordinate_agents_runs_multiple_workstreams() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let mut ctx = ToolContext::empty();
        ctx.sub_agent_runner = Some(Arc::new(MockRunner {
            seen: Arc::clone(&seen),
        }));

        let tool = CoordinateAgentsTool;
        let result = tool
            .execute(
                serde_json::json!({
                    "goal": "ship the feature",
                    "workstreams": [
                        {
                            "description": "review",
                            "prompt": "review the patch"
                        },
                        {
                            "description": "verify",
                            "prompt": "run validation"
                        }
                    ]
                }),
                &ctx,
            )
            .await
            .unwrap();

        assert!(!result.is_error);
        assert!(result.content.contains("\"status\": \"ok\""));
        assert_eq!(seen.lock().unwrap().len(), 2);
    }
}
