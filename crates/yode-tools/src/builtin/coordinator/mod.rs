mod planning;

use anyhow::Result;
use async_trait::async_trait;
use futures::future::join_all;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::builtin::orchestration_common::persist_coordinator_runtime_artifacts;
use crate::builtin::team_runtime::{
    persist_agent_team_runtime, update_agent_team_member, AgentTeamMemberState,
};
use crate::tool::{SubAgentOptions, Tool, ToolCapabilities, ToolContext, ToolResult};

use self::planning::{
    build_execution_phases, max_parallel_label, normalize_workstreams, render_phase_plan,
    render_phase_timeline,
};

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
        let team_id = format!(
            "team-{}",
            goal.chars()
                .map(|ch| if ch.is_ascii_alphanumeric() { ch.to_ascii_lowercase() } else { '-' })
                .collect::<String>()
                .trim_matches('-')
        );
        let team_members = normalized
            .iter()
            .map(|workstream| AgentTeamMemberState {
                member_id: workstream.id.clone(),
                description: workstream.description.clone(),
                subagent_type: workstream.subagent_type.clone(),
                model: workstream.model.clone(),
                run_in_background: workstream.run_in_background.unwrap_or(true),
                allowed_tools: workstream.allowed_tools.clone(),
                permission_inheritance: if workstream.allowed_tools.is_empty() {
                    "parent_tool_pool".to_string()
                } else {
                    "explicit_allowlist".to_string()
                },
                status: if dry_run {
                    "planned".to_string()
                } else {
                    "running".to_string()
                },
                runtime_task_id: None,
                last_result_preview: None,
                result_artifact_path: None,
                last_updated_at: Some(chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string()),
            })
            .collect::<Vec<_>>();
        let team_artifacts = ctx
            .working_dir
            .as_deref()
            .and_then(|dir| persist_agent_team_runtime(dir, &goal, Some(&team_id), if dry_run { "dry_run" } else { "coordinate" }, team_members).ok());

        if dry_run {
            let plan = render_phase_plan(&phases, max_parallel);
            let timeline = render_phase_timeline(&phases, max_parallel);
            let artifacts = ctx
                .working_dir
                .as_deref()
                .and_then(|dir| {
                    persist_coordinator_runtime_artifacts(
                        dir,
                        &goal,
                        true,
                        &max_parallel_label(max_parallel).to_string(),
                        phases.len(),
                        normalized.len(),
                        &timeline,
                        &plan,
                        &[],
                    )
                    .ok()
                });
            return Ok(ToolResult::success_with_metadata(
                format!(
                    "Coordinator phase timeline\n{}\n\nJSON plan\n{}\n",
                    timeline,
                    serde_json::to_string_pretty(&plan)?
                ),
                json!({
                    "goal": goal,
                    "dry_run": true,
                    "phase_count": phases.len(),
                    "workstream_count": normalized.len(),
                    "max_parallel": max_parallel_label(max_parallel),
                    "timeline": timeline,
                    "plan": plan,
                    "coordinator_summary_artifact": artifacts
                        .as_ref()
                        .and_then(|set| set.summary_path.as_ref())
                        .map(|path| path.display().to_string()),
                    "coordinator_state_artifact": artifacts
                        .as_ref()
                        .and_then(|set| set.state_path.as_ref())
                        .map(|path| path.display().to_string()),
                    "orchestration_timeline_artifact": artifacts
                        .as_ref()
                        .and_then(|set| set.timeline_path.as_ref())
                        .map(|path| path.display().to_string()),
                    "team_id": team_id,
                    "team_summary_artifact": team_artifacts
                        .as_ref()
                        .and_then(|set| set.summary_path.as_ref())
                        .map(|path| path.display().to_string()),
                    "team_state_artifact": team_artifacts
                        .as_ref()
                        .and_then(|set| set.state_path.as_ref())
                        .map(|path| path.display().to_string()),
                    "team_monitor_artifact": team_artifacts
                        .as_ref()
                        .and_then(|set| set.monitor_path.as_ref())
                        .map(|path| path.display().to_string()),
                }),
            ));
        }

        let runner = ctx
            .sub_agent_runner
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Sub-agent runner not available"))?;

        let mut completed_outputs: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
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
                            team_id: Some(team_id.clone()),
                            member_id: Some(workstream.id.clone()),
                        },
                    )
                });

                let results = join_all(futures).await;
                for (workstream, result) in batch.iter().zip(results.into_iter()) {
                    match result {
                        Ok(output) => {
                            completed_outputs.insert(workstream.id.clone(), output.clone());
                            if let Some(dir) = ctx.working_dir.as_deref() {
                                let _ = update_agent_team_member(
                                    dir,
                                    &team_id,
                                    &workstream.id,
                                    if workstream.run_in_background.unwrap_or(true) {
                                        "running"
                                    } else {
                                        "completed"
                                    },
                                    parse_team_task_id(&output),
                                    Some(output.clone()),
                                    None,
                                );
                            }
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
                            if let Some(dir) = ctx.working_dir.as_deref() {
                                let _ = update_agent_team_member(
                                    dir,
                                    &team_id,
                                    &workstream.id,
                                    "failed",
                                    None,
                                    Some(format!("{}", err)),
                                    None,
                                );
                            }
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
        let timeline = render_phase_timeline(&phases, max_parallel);
        let plan = render_phase_plan(&phases, max_parallel);
        let artifacts = ctx
            .working_dir
            .as_deref()
            .and_then(|dir| {
                persist_coordinator_runtime_artifacts(
                    dir,
                    &goal,
                    false,
                    &max_parallel_label(max_parallel).to_string(),
                    phases.len(),
                    normalized.len(),
                    &timeline,
                    &plan,
                    &rendered,
                )
                .ok()
            });

        Ok(ToolResult::success_with_metadata(
            rendered_text,
            json!({
                "goal": goal,
                "workstream_count": normalized.len(),
                "phase_count": phases.len(),
                "max_parallel": max_parallel_label(max_parallel),
                "timeline": timeline,
                "plan": plan,
                "coordination_artifact_path": artifacts
                    .as_ref()
                    .and_then(|set| set.summary_path.as_ref())
                    .map(|path| path.display().to_string()),
                "coordinator_state_artifact": artifacts
                    .as_ref()
                    .and_then(|set| set.state_path.as_ref())
                    .map(|path| path.display().to_string()),
                "orchestration_timeline_artifact": artifacts
                    .as_ref()
                    .and_then(|set| set.timeline_path.as_ref())
                    .map(|path| path.display().to_string()),
                "team_id": team_id,
                "team_summary_artifact": team_artifacts
                    .as_ref()
                    .and_then(|set| set.summary_path.as_ref())
                    .map(|path| path.display().to_string()),
                "team_state_artifact": team_artifacts
                    .as_ref()
                    .and_then(|set| set.state_path.as_ref())
                    .map(|path| path.display().to_string()),
                "team_monitor_artifact": team_artifacts
                    .as_ref()
                    .and_then(|set| set.monitor_path.as_ref())
                    .map(|path| path.display().to_string()),
                "results": rendered,
            }),
        ))
    }
}

fn parse_team_task_id(output: &str) -> Option<String> {
    output
        .strip_prefix("Background sub-agent launched as ")
        .and_then(|rest| rest.split_once('.'))
        .map(|(task_id, _)| task_id.trim().to_string())
}

#[cfg(test)]
mod tests;
