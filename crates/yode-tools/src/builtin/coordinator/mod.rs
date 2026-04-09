use std::collections::{HashMap, HashSet};

use anyhow::Result;
use async_trait::async_trait;
use futures::future::join_all;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::tool::{SubAgentOptions, Tool, ToolCapabilities, ToolContext, ToolResult};

#[derive(Debug, Deserialize)]
struct Workstream {
    #[serde(default)]
    id: Option<String>,
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
    #[serde(default)]
    depends_on: Vec<String>,
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
        "Launch multiple sub-agents for related workstreams, honoring simple dependencies and aggregating results. This is a lightweight coordinator-mode helper."
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
                            "id": { "type": "string" },
                            "description": { "type": "string" },
                            "prompt": { "type": "string" },
                            "subagent_type": { "type": "string" },
                            "model": { "type": "string" },
                            "run_in_background": { "type": "boolean" },
                            "allowed_tools": {
                                "type": "array",
                                "items": { "type": "string" }
                            },
                            "depends_on": {
                                "type": "array",
                                "items": { "type": "string" },
                                "description": "Optional workstream IDs that must finish before this workstream runs."
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

        let normalized = normalize_workstreams(workstreams)?;
        let runner = ctx
            .sub_agent_runner
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Sub-agent runner not available"))?;

        let mut completed_outputs: HashMap<String, String> = HashMap::new();
        let mut finished: HashSet<String> = HashSet::new();
        let mut pending = normalized.clone();
        let mut phase = 0usize;
        let mut rendered = Vec::new();

        while !pending.is_empty() {
            phase += 1;
            let mut ready = Vec::new();
            let mut still_pending = Vec::new();

            for workstream in pending.into_iter() {
                if workstream
                    .depends_on
                    .iter()
                    .all(|dependency| finished.contains(dependency))
                {
                    ready.push(workstream);
                } else {
                    still_pending.push(workstream);
                }
            }

            if ready.is_empty() {
                return Ok(ToolResult::error(
                    "Coordinator could not resolve workstream dependencies. Check for cycles or missing dependency IDs."
                        .to_string(),
                ));
            }

            let futures = ready.iter().map(|workstream| {
                let prerequisite_summary = if workstream.depends_on.is_empty() {
                    "No prerequisite workstreams.".to_string()
                } else {
                    workstream
                        .depends_on
                        .iter()
                        .filter_map(|dependency| {
                            completed_outputs.get(dependency).map(|output| {
                                let preview: String = output.chars().take(240).collect();
                                format!("{} => {}", dependency, preview)
                            })
                        })
                        .collect::<Vec<_>>()
                        .join("\n")
                };
                let prompt = format!(
                    "Coordinator goal:\n{}\n\nWorkstream ID: {}\nWorkstream:\n{}\n\nPrerequisite outputs:\n{}\n\nTask:\n{}",
                    goal,
                    workstream.id,
                    workstream.description,
                    prerequisite_summary,
                    workstream.prompt
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
            for (workstream, result) in ready.into_iter().zip(results.into_iter()) {
                match result {
                    Ok(output) => {
                        completed_outputs.insert(workstream.id.clone(), output.clone());
                        finished.insert(workstream.id.clone());
                        rendered.push(json!({
                            "phase": phase,
                            "id": workstream.id,
                            "description": workstream.description,
                            "status": "ok",
                            "output": output,
                        }));
                    }
                    Err(err) => {
                        finished.insert(workstream.id.clone());
                        rendered.push(json!({
                            "phase": phase,
                            "id": workstream.id,
                            "description": workstream.description,
                            "status": "error",
                            "output": format!("{}", err),
                        }));
                    }
                }
            }

            pending = still_pending;
        }

        Ok(ToolResult::success_with_metadata(
            serde_json::to_string_pretty(&rendered)?,
            json!({
                "goal": goal,
                "workstream_count": normalized.len(),
                "phase_count": phase,
                "results": rendered,
            }),
        ))
    }
}

fn normalize_workstreams(workstreams: Vec<Workstream>) -> Result<Vec<NormalizedWorkstream>> {
    let mut seen = HashSet::new();
    let mut normalized = Vec::new();
    for (index, workstream) in workstreams.into_iter().enumerate() {
        let id = workstream
            .id
            .clone()
            .unwrap_or_else(|| format!("ws{}", index + 1));
        if !seen.insert(id.clone()) {
            return Err(anyhow::anyhow!(
                "Duplicate coordinator workstream id '{}'.",
                id
            ));
        }
        normalized.push(NormalizedWorkstream {
            id,
            description: workstream.description,
            prompt: workstream.prompt,
            subagent_type: workstream.subagent_type,
            model: workstream.model,
            run_in_background: workstream.run_in_background,
            allowed_tools: workstream.allowed_tools,
            depends_on: workstream.depends_on,
        });
    }

    let all_ids = normalized
        .iter()
        .map(|workstream| workstream.id.clone())
        .collect::<HashSet<_>>();
    for workstream in &normalized {
        for dependency in &workstream.depends_on {
            if !all_ids.contains(dependency) {
                return Err(anyhow::anyhow!(
                    "Workstream '{}' depends on unknown id '{}'.",
                    workstream.id,
                    dependency
                ));
            }
        }
    }

    Ok(normalized)
}

#[derive(Debug, Clone)]
struct NormalizedWorkstream {
    id: String,
    description: String,
    prompt: String,
    subagent_type: Option<String>,
    model: Option<String>,
    run_in_background: Option<bool>,
    allowed_tools: Vec<String>,
    depends_on: Vec<String>,
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
                            "id": "review",
                            "description": "review",
                            "prompt": "review the patch"
                        },
                        {
                            "id": "verify",
                            "description": "verify",
                            "prompt": "run validation",
                            "depends_on": ["review"]
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
        assert!(result.content.contains("\"phase\": 1"));
        assert!(result.content.contains("\"phase\": 2"));
    }

    #[test]
    fn coordinator_rejects_unknown_dependency() {
        let result = super::normalize_workstreams(vec![super::Workstream {
            id: Some("verify".to_string()),
            description: "verify".to_string(),
            prompt: "run validation".to_string(),
            subagent_type: None,
            model: None,
            run_in_background: None,
            allowed_tools: Vec::new(),
            depends_on: vec!["missing".to_string()],
        }]);
        assert!(result.is_err());
    }
}
