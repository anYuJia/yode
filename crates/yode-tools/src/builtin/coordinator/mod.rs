use std::collections::{HashMap, HashSet};

use anyhow::Result;
use async_trait::async_trait;
use futures::future::join_all;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::builtin::review_common::persist_review_artifact;
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
                },
                "dry_run": {
                    "type": "boolean",
                    "default": false,
                    "description": "If true, return the dependency phases without launching sub-agents."
                },
                "max_parallel": {
                    "type": "integer",
                    "minimum": 1,
                    "description": "Maximum number of workstreams to run concurrently inside each dependency phase. Defaults to all ready workstreams."
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
        let dry_run = params
            .get("dry_run")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        let max_parallel = params
            .get("max_parallel")
            .and_then(|value| value.as_u64())
            .map(|value| value.max(1) as usize)
            .unwrap_or(usize::MAX);
        let phases = build_execution_phases(&normalized)?;

        if dry_run {
            let plan = render_phase_plan(&phases, max_parallel);
            return Ok(ToolResult::success_with_metadata(
                serde_json::to_string_pretty(&plan)?,
                json!({
                    "goal": goal,
                    "dry_run": true,
                    "phase_count": phases.len(),
                    "workstream_count": normalized.len(),
                    "max_parallel": max_parallel_label(max_parallel),
                    "plan": plan,
                }),
            ));
        }

        let runner = ctx
            .sub_agent_runner
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Sub-agent runner not available"))?;

        let mut completed_outputs: HashMap<String, String> = HashMap::new();
        let mut rendered = Vec::new();

        for (phase_index, phase_workstreams) in phases.iter().enumerate() {
            for (batch_index, batch) in phase_workstreams.chunks(max_parallel).enumerate() {
                let futures = batch.iter().map(|workstream| {
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
                for (workstream, result) in batch.iter().zip(results.into_iter()) {
                    match result {
                        Ok(output) => {
                            completed_outputs.insert(workstream.id.clone(), output.clone());
                            rendered.push(json!({
                                "phase": phase_index + 1,
                                "batch": batch_index + 1,
                                "id": workstream.id,
                                "description": workstream.description,
                                "status": "ok",
                                "output": output,
                            }));
                        }
                        Err(err) => {
                            rendered.push(json!({
                                "phase": phase_index + 1,
                                "batch": batch_index + 1,
                                "id": workstream.id,
                                "description": workstream.description,
                                "status": "error",
                                "output": format!("{}", err),
                            }));
                        }
                    }
                }
            }
        }

        let rendered_text = serde_json::to_string_pretty(&rendered)?;
        let artifact_path = ctx
            .working_dir
            .as_deref()
            .and_then(|dir| persist_review_artifact(dir, "coordinator", &goal, &rendered_text).ok())
            .map(|path| path.display().to_string());

        Ok(ToolResult::success_with_metadata(
            rendered_text,
            json!({
                "goal": goal,
                "workstream_count": normalized.len(),
                "phase_count": phases.len(),
                "max_parallel": max_parallel_label(max_parallel),
                "coordination_artifact_path": artifact_path,
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

fn build_execution_phases(
    workstreams: &[NormalizedWorkstream],
) -> Result<Vec<Vec<NormalizedWorkstream>>> {
    let mut finished: HashSet<String> = HashSet::new();
    let mut pending = workstreams.to_vec();
    let mut phases = Vec::new();

    while !pending.is_empty() {
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
            let blocked = still_pending
                .iter()
                .map(|workstream| {
                    let missing = workstream
                        .depends_on
                        .iter()
                        .filter(|dependency| !finished.contains(*dependency))
                        .cloned()
                        .collect::<Vec<_>>();
                    format!("{} -> waiting for {}", workstream.id, missing.join(", "))
                })
                .collect::<Vec<_>>()
                .join("; ");
            return Err(anyhow::anyhow!(
                "Coordinator could not resolve workstream dependencies. Blocked set: {}",
                blocked
            ));
        }

        for workstream in &ready {
            finished.insert(workstream.id.clone());
        }
        phases.push(ready);
        pending = still_pending;
    }

    Ok(phases)
}

fn render_phase_plan(phases: &[Vec<NormalizedWorkstream>], max_parallel: usize) -> Vec<Value> {
    phases
        .iter()
        .enumerate()
        .map(|(phase_index, workstreams)| {
            json!({
                "phase": phase_index + 1,
                "batches": workstreams
                    .chunks(max_parallel)
                    .enumerate()
                    .map(|(batch_index, batch)| {
                        json!({
                            "batch": batch_index + 1,
                            "workstreams": batch
                                .iter()
                                .map(|workstream| workstream.id.clone())
                                .collect::<Vec<_>>(),
                        })
                    })
                    .collect::<Vec<_>>(),
                "workstreams": workstreams
                    .iter()
                    .map(|workstream| {
                        json!({
                            "id": workstream.id,
                            "description": workstream.description,
                            "depends_on": workstream.depends_on,
                            "run_in_background": workstream.run_in_background.unwrap_or(true),
                            "allowed_tools": workstream.allowed_tools,
                        })
                    })
                    .collect::<Vec<_>>(),
            })
        })
        .collect()
}

fn max_parallel_label(max_parallel: usize) -> Value {
    if max_parallel == usize::MAX {
        Value::String("all".to_string())
    } else {
        json!(max_parallel)
    }
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

    #[tokio::test]
    async fn coordinate_agents_dry_run_returns_phase_plan() {
        let ctx = ToolContext::empty();

        let tool = CoordinateAgentsTool;
        let result = tool
            .execute(
                serde_json::json!({
                    "goal": "ship the feature",
                    "dry_run": true,
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
        assert!(result.content.contains("\"phase\": 1"));
        assert!(result.content.contains("\"phase\": 2"));
        assert_eq!(result.metadata.unwrap()["dry_run"], true);
    }

    #[tokio::test]
    async fn coordinate_agents_respects_max_parallel_batches() {
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
                    "max_parallel": 2,
                    "workstreams": [
                        { "id": "a", "description": "a", "prompt": "a" },
                        { "id": "b", "description": "b", "prompt": "b" },
                        { "id": "c", "description": "c", "prompt": "c" }
                    ]
                }),
                &ctx,
            )
            .await
            .unwrap();

        assert!(!result.is_error);
        assert!(result.content.contains("\"batch\": 1"));
        assert!(result.content.contains("\"batch\": 2"));
        assert_eq!(result.metadata.unwrap()["max_parallel"], 2);
        assert_eq!(seen.lock().unwrap().len(), 3);
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

    #[test]
    fn coordinator_reports_blocked_cycle_details() {
        let workstreams = vec![
            super::NormalizedWorkstream {
                id: "a".to_string(),
                description: "a".to_string(),
                prompt: "a".to_string(),
                subagent_type: None,
                model: None,
                run_in_background: None,
                allowed_tools: Vec::new(),
                depends_on: vec!["b".to_string()],
            },
            super::NormalizedWorkstream {
                id: "b".to_string(),
                description: "b".to_string(),
                prompt: "b".to_string(),
                subagent_type: None,
                model: None,
                run_in_background: None,
                allowed_tools: Vec::new(),
                depends_on: vec!["a".to_string()],
            },
        ];

        let err = super::build_execution_phases(&workstreams).unwrap_err();
        assert!(err.to_string().contains("a -> waiting for b"));
        assert!(err.to_string().contains("b -> waiting for a"));
    }
}
