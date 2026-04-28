use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::Arc;
pub use yode_agent::{
    AgentTeamManager, AgentTeamMemberState, AgentTeamMessage, AgentTeamSnapshot, AgentTeamState,
};

use crate::tool::{Tool, ToolCapabilities, ToolContext, ToolResult};

#[derive(Debug, Clone, Default)]
pub struct AgentTeamArtifactSet {
    pub summary_path: Option<PathBuf>,
    pub state_path: Option<PathBuf>,
    pub messages_path: Option<PathBuf>,
    pub monitor_path: Option<PathBuf>,
    pub bundle_path: Option<PathBuf>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct TeamMemberInput {
    id: String,
    description: String,
    #[serde(default)]
    subagent_type: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    run_in_background: Option<bool>,
    #[serde(default)]
    allowed_tools: Vec<String>,
}

pub fn persist_agent_team_runtime(
    working_dir: &Path,
    goal: &str,
    team_id: Option<&str>,
    mode: &str,
    mut members: Vec<AgentTeamMemberState>,
) -> Result<AgentTeamArtifactSet> {
    let team_id = team_id
        .filter(|id| !id.trim().is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| default_team_id(goal));
    for member in &mut members {
        if member.last_updated_at.is_none() {
            member.last_updated_at = Some(now_string());
        }
    }
    let state = AgentTeamState {
        kind: "agent_team".to_string(),
        team_id: team_id.clone(),
        goal: goal.to_string(),
        mode: mode.to_string(),
        created_at: now_string(),
        updated_at: now_string(),
        member_count: members.len(),
        active_count: members
            .iter()
            .filter(|member| matches!(member.status.as_str(), "planned" | "running"))
            .count(),
        completed_count: members
            .iter()
            .filter(|member| member.status == "completed")
            .count(),
        failed_count: members
            .iter()
            .filter(|member| member.status == "failed")
            .count(),
        latest_message_count: load_team_messages(working_dir, &team_id)
            .map(|messages| messages.len())
            .unwrap_or(0),
        latest_message_artifact: Some(
            team_messages_path(working_dir, &team_id)
                .display()
                .to_string(),
        ),
        latest_bundle_artifact: Some(
            team_bundle_path(working_dir, &team_id)
                .display()
                .to_string(),
        ),
        members,
    };
    write_agent_team_state(working_dir, &state)
}

pub fn load_agent_team_state(working_dir: &Path, team_id: &str) -> Result<Option<AgentTeamState>> {
    let path = team_state_path(working_dir, team_id);
    if !path.exists() {
        return Ok(None);
    }
    Ok(Some(serde_json::from_str(&std::fs::read_to_string(path)?)?))
}

pub fn load_agent_team_messages(
    working_dir: &Path,
    team_id: &str,
) -> Result<Vec<AgentTeamMessage>> {
    Ok(load_team_messages(working_dir, team_id).unwrap_or_default())
}

pub fn load_agent_team_snapshot(
    working_dir: &Path,
    team_id: &str,
) -> Result<Option<AgentTeamSnapshot>> {
    let Some(state) = load_agent_team_state(working_dir, team_id)? else {
        return Ok(None);
    };
    Ok(Some(AgentTeamSnapshot {
        state: Some(state),
        messages: load_team_messages(working_dir, team_id).unwrap_or_default(),
    }))
}

pub fn agent_team_artifact_paths(working_dir: &Path, team_id: &str) -> AgentTeamArtifactSet {
    AgentTeamArtifactSet {
        summary_path: Some(team_summary_path(working_dir, team_id)),
        state_path: Some(team_state_path(working_dir, team_id)),
        messages_path: Some(team_messages_path(working_dir, team_id)),
        monitor_path: Some(team_monitor_path(working_dir, team_id)),
        bundle_path: Some(team_bundle_path(working_dir, team_id)),
    }
}

pub fn hydrate_agent_team_manager(
    working_dir: &Path,
    manager: &mut AgentTeamManager,
    team_id: &str,
) -> Result<Option<AgentTeamSnapshot>> {
    if manager.snapshot(team_id).is_none() {
        if let Some(state) = load_agent_team_state(working_dir, team_id)? {
            let messages = load_team_messages(working_dir, team_id).unwrap_or_default();
            manager.upsert_snapshot(state, messages);
        }
    }
    Ok(manager.snapshot(team_id))
}

pub fn persist_agent_team_snapshot(
    working_dir: &Path,
    snapshot: &AgentTeamSnapshot,
) -> Result<AgentTeamArtifactSet> {
    let mut state = snapshot
        .state
        .clone()
        .ok_or_else(|| anyhow::anyhow!("Agent team snapshot has no state."))?;
    let dir = teams_dir(working_dir);
    std::fs::create_dir_all(&dir)?;
    let state_path = team_state_path(working_dir, &state.team_id);
    let summary_path = team_summary_path(working_dir, &state.team_id);
    let messages_path = team_messages_path(working_dir, &state.team_id);
    let monitor_path = team_monitor_path(working_dir, &state.team_id);
    let bundle_path = team_bundle_path(working_dir, &state.team_id);

    state.latest_message_count = snapshot.messages.len();
    state.latest_message_artifact = Some(messages_path.display().to_string());
    state.latest_bundle_artifact = Some(bundle_path.display().to_string());

    let message_body = if snapshot.messages.is_empty() {
        "# Agent Team Messages\n\n- none\n".to_string()
    } else {
        format!(
            "# Agent Team Messages\n\n{}\n",
            snapshot
                .messages
                .iter()
                .map(|entry| format!(
                    "- {} | {} | {} | {}",
                    entry.at, entry.target, entry.kind, entry.message
                ))
                .collect::<Vec<_>>()
                .join("\n")
        )
    };
    std::fs::write(&messages_path, message_body)?;
    std::fs::write(&state_path, serde_json::to_string_pretty(&state)?)?;
    std::fs::write(&summary_path, render_team_summary(&state))?;
    std::fs::write(
        &monitor_path,
        render_agent_team_monitor_from_snapshot(snapshot, None, true)?,
    )?;
    std::fs::write(
        &bundle_path,
        render_team_bundle(&state, messages_path.as_path()),
    )?;

    Ok(AgentTeamArtifactSet {
        summary_path: Some(summary_path),
        state_path: Some(state_path),
        messages_path: Some(messages_path),
        monitor_path: Some(monitor_path),
        bundle_path: Some(bundle_path),
    })
}

pub fn latest_agent_team_file(working_dir: &Path, suffix: &str) -> Option<PathBuf> {
    let dir = teams_dir(working_dir);
    let mut entries = std::fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(suffix))
        })
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| right.file_name().cmp(&left.file_name()));
    entries.into_iter().next()
}

pub fn update_agent_team_member(
    working_dir: &Path,
    team_id: &str,
    member_id: &str,
    status: &str,
    runtime_task_id: Option<String>,
    result_preview: Option<String>,
    result_artifact_path: Option<String>,
) -> Result<AgentTeamArtifactSet> {
    let mut state = load_agent_team_state(working_dir, team_id)?
        .ok_or_else(|| anyhow::anyhow!("Team '{}' not found.", team_id))?;
    if let Some(member) = state
        .members
        .iter_mut()
        .find(|member| member.member_id == member_id)
    {
        member.status = status.to_string();
        if runtime_task_id.is_some() {
            member.runtime_task_id = runtime_task_id;
        }
        if result_preview.is_some() {
            member.last_result_preview = result_preview.map(|value| truncate_preview(&value, 240));
        }
        if result_artifact_path.is_some() {
            member.result_artifact_path = result_artifact_path;
        }
        member.last_updated_at = Some(now_string());
    } else {
        return Err(anyhow::anyhow!(
            "Member '{}' not found in team '{}'.",
            member_id,
            team_id
        ));
    }
    state.updated_at = now_string();
    state.active_count = state
        .members
        .iter()
        .filter(|member| matches!(member.status.as_str(), "planned" | "running"))
        .count();
    state.completed_count = state
        .members
        .iter()
        .filter(|member| member.status == "completed")
        .count();
    state.failed_count = state
        .members
        .iter()
        .filter(|member| member.status == "failed")
        .count();
    write_agent_team_state(working_dir, &state)
}

pub fn append_agent_team_message(
    working_dir: &Path,
    team_id: &str,
    target: &str,
    kind: &str,
    message: &str,
) -> Result<PathBuf> {
    let path = team_messages_path(working_dir, team_id);
    std::fs::create_dir_all(path.parent().unwrap())?;
    let entry = AgentTeamMessage {
        at: now_string(),
        target: target.to_string(),
        kind: kind.to_string(),
        message: message.to_string(),
    };
    let mut messages = load_team_messages(working_dir, team_id).unwrap_or_default();
    messages.push(entry.clone());
    let body = format!(
        "# Agent Team Messages\n\n{}\n",
        messages
            .iter()
            .map(|entry| format!(
                "- {} | {} | {} | {}",
                entry.at, entry.target, entry.kind, entry.message
            ))
            .collect::<Vec<_>>()
            .join("\n")
    );
    std::fs::write(&path, body)?;
    if let Some(mut state) = load_agent_team_state(working_dir, team_id)? {
        state.latest_message_count = messages.len();
        state.latest_message_artifact = Some(path.display().to_string());
        state.updated_at = now_string();
        for member in &mut state.members {
            if target == "all" || target == member.member_id {
                member.pending_message_count = member.pending_message_count.saturating_add(1);
                member.last_message_at = Some(entry.at.clone());
            }
        }
        let _ = write_agent_team_state(working_dir, &state);
    }
    Ok(path)
}

pub fn consume_agent_team_messages(
    working_dir: &Path,
    team_id: &str,
    member_id: &str,
    max_items: usize,
) -> Result<Vec<AgentTeamMessage>> {
    let messages = load_team_messages(working_dir, team_id)
        .unwrap_or_default()
        .into_iter()
        .filter(|message| message.target == "all" || message.target == member_id)
        .rev()
        .take(max_items.max(1))
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>();
    if !messages.is_empty() {
        if let Some(mut state) = load_agent_team_state(working_dir, team_id)? {
            if let Some(member) = state
                .members
                .iter_mut()
                .find(|member| member.member_id == member_id)
            {
                member.pending_message_count = 0;
                member.last_updated_at = Some(now_string());
            }
            state.updated_at = now_string();
            let _ = write_agent_team_state(working_dir, &state);
        }
    }
    Ok(messages)
}

pub fn render_agent_team_monitor(
    working_dir: &Path,
    team_id: Option<&str>,
    runtime_tasks: Option<&Arc<tokio::sync::Mutex<crate::runtime_tasks::RuntimeTaskStore>>>,
    include_messages: bool,
) -> Result<String> {
    let team_id = match team_id {
        Some(team_id) => team_id.to_string(),
        None => latest_agent_team_file(working_dir, "agent-team-state.json")
            .and_then(|path| {
                path.file_stem()
                    .and_then(|stem| stem.to_str())
                    .map(|stem| stem.trim_end_matches("-agent-team-state").to_string())
            })
            .ok_or_else(|| anyhow::anyhow!("No agent team state artifact found."))?,
    };
    let state = load_agent_team_state(working_dir, &team_id)?
        .ok_or_else(|| anyhow::anyhow!("Team '{}' not found.", team_id))?;
    let messages = if include_messages {
        load_team_messages(working_dir, &team_id).unwrap_or_default()
    } else {
        Vec::new()
    };
    render_agent_team_monitor_from_snapshot(
        &AgentTeamSnapshot {
            state: Some(state),
            messages,
        },
        runtime_tasks,
        include_messages,
    )
}

pub fn render_agent_team_monitor_from_snapshot(
    snapshot: &AgentTeamSnapshot,
    runtime_tasks: Option<&Arc<tokio::sync::Mutex<crate::runtime_tasks::RuntimeTaskStore>>>,
    include_messages: bool,
) -> Result<String> {
    let state = snapshot
        .state
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Agent team snapshot has no state."))?;
    let runtime_snapshot = if let Some(store) = runtime_tasks {
        if let Ok(guard) = store.try_lock() {
            guard.list()
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };
    let message_lines = if include_messages {
        snapshot
            .messages
            .clone()
            .into_iter()
            .rev()
            .take(6)
            .map(|entry| {
                format!(
                    "- {} [{}:{}] {}",
                    entry.at, entry.target, entry.kind, entry.message
                )
            })
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    let mut body = vec![
        "# Agent Team Monitor".to_string(),
        String::new(),
        format!("- Team: {}", state.team_id),
        format!("- Goal: {}", state.goal),
        format!("- Mode: {}", state.mode),
        format!(
            "- Members: {} total / {} active / {} completed / {} failed",
            state.member_count, state.active_count, state.completed_count, state.failed_count
        ),
        format!("- Updated at: {}", state.updated_at),
        String::new(),
        "Members:".to_string(),
    ];
    for member in &state.members {
        let runtime_status = member
            .runtime_task_id
            .as_deref()
            .and_then(|task_id| runtime_snapshot.iter().find(|task| task.id == task_id))
            .map(|task| format!("{:?}", task.status))
            .unwrap_or_else(|| "none".to_string());
        body.push(format!(
            "- {} [{}] task={} runtime={} inbox={} inheritance={}{}{}",
            member.member_id,
            member.status,
            member.runtime_task_id.as_deref().unwrap_or("none"),
            runtime_status,
            member.pending_message_count,
            member.permission_inheritance,
            member
                .last_result_preview
                .as_ref()
                .map(|value| format!(" / {}", value))
                .unwrap_or_default(),
            member
                .result_artifact_path
                .as_ref()
                .map(|path| format!(" / artifact={}", path))
                .unwrap_or_default()
        ));
    }
    if include_messages {
        body.push(String::new());
        body.push("Recent messages:".to_string());
        if message_lines.is_empty() {
            body.push("- none".to_string());
        } else {
            body.extend(message_lines);
        }
    }
    Ok(body.join("\n"))
}

pub struct TeamCreateTool;
pub struct SendMessageTool;
pub struct TeamReceiveTool;
pub struct TeamMonitorTool;

#[async_trait]
impl Tool for TeamCreateTool {
    fn name(&self) -> &str {
        "team_create"
    }

    fn user_facing_name(&self) -> &str {
        "Agent Team"
    }

    fn description(&self) -> &str {
        "Create or refresh an agent team runtime artifact from a set of members. Use this before coordinating sub-agents through a shared team state."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "goal": { "type": "string" },
                "team_id": { "type": "string" },
                "mode": { "type": "string", "default": "manual" },
                "members": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "id": { "type": "string" },
                            "description": { "type": "string" },
                            "subagent_type": { "type": "string" },
                            "model": { "type": "string" },
                            "run_in_background": { "type": "boolean" },
                            "allowed_tools": { "type": "array", "items": { "type": "string" } }
                        },
                        "required": ["id", "description"]
                    }
                }
            },
            "required": ["goal", "members"]
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
        let working_dir = ctx
            .working_dir
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Working directory not set"))?;
        let goal = params
            .get("goal")
            .and_then(|value| value.as_str())
            .ok_or_else(|| anyhow::anyhow!("'goal' parameter is required"))?;
        let team_id = params.get("team_id").and_then(|value| value.as_str());
        let mode = params
            .get("mode")
            .and_then(|value| value.as_str())
            .unwrap_or("manual");
        let members = serde_json::from_value::<Vec<TeamMemberInput>>(
            params
                .get("members")
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("'members' parameter is required"))?,
        )?;
        let members = members
            .into_iter()
            .map(team_member_from_input)
            .collect::<Vec<_>>();
        let artifacts = if let Some(manager) = ctx.team_runtime.as_ref() {
            let state = {
                let mut manager = manager.lock().await;
                manager.ensure_team(goal, team_id, mode, members)
            };
            persist_agent_team_snapshot(
                working_dir,
                &AgentTeamSnapshot {
                    state: Some(state),
                    messages: Vec::new(),
                },
            )?
        } else {
            persist_agent_team_runtime(working_dir, goal, team_id, mode, members)?
        };
        Ok(ToolResult::success_with_metadata(
            format!(
                "Agent team prepared.\nSummary: {}\nState: {}\nMonitor: {}",
                artifacts
                    .summary_path
                    .as_ref()
                    .map(|path| path.display().to_string())
                    .unwrap_or_else(|| "none".to_string()),
                artifacts
                    .state_path
                    .as_ref()
                    .map(|path| path.display().to_string())
                    .unwrap_or_else(|| "none".to_string()),
                artifacts
                    .monitor_path
                    .as_ref()
                    .map(|path| path.display().to_string())
                    .unwrap_or_else(|| "none".to_string()),
            ),
            json!({
                "team_summary_artifact": artifacts.summary_path.as_ref().map(|path| path.display().to_string()),
                "team_state_artifact": artifacts.state_path.as_ref().map(|path| path.display().to_string()),
                "team_messages_artifact": artifacts.messages_path.as_ref().map(|path| path.display().to_string()),
                "team_monitor_artifact": artifacts.monitor_path.as_ref().map(|path| path.display().to_string()),
                "team_bundle_artifact": artifacts.bundle_path.as_ref().map(|path| path.display().to_string()),
            }),
        ))
    }
}

#[async_trait]
impl Tool for SendMessageTool {
    fn name(&self) -> &str {
        "send_message"
    }

    fn user_facing_name(&self) -> &str {
        "Team Message"
    }

    fn description(&self) -> &str {
        "Append a message or handoff note into an agent team runtime thread."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "team_id": { "type": "string" },
                "target": { "type": "string", "default": "all" },
                "message": { "type": "string" },
                "kind": { "type": "string", "default": "message" }
            },
            "required": ["team_id", "message"]
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
        let working_dir = ctx
            .working_dir
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Working directory not set"))?;
        let team_id = params
            .get("team_id")
            .and_then(|value| value.as_str())
            .ok_or_else(|| anyhow::anyhow!("'team_id' parameter is required"))?;
        let target = params
            .get("target")
            .and_then(|value| value.as_str())
            .unwrap_or("all");
        let message = params
            .get("message")
            .and_then(|value| value.as_str())
            .ok_or_else(|| anyhow::anyhow!("'message' parameter is required"))?;
        let kind = params
            .get("kind")
            .and_then(|value| value.as_str())
            .unwrap_or("message");
        let path = if let Some(manager) = ctx.team_runtime.as_ref() {
            let snapshot = {
                let mut manager = manager.lock().await;
                let _ = hydrate_agent_team_manager(working_dir, &mut manager, team_id)?;
                manager.append_message(team_id, target, kind, message)?;
                manager
                    .snapshot(team_id)
                    .ok_or_else(|| anyhow::anyhow!("Team '{}' not found.", team_id))?
            };
            persist_agent_team_snapshot(working_dir, &snapshot)?
                .messages_path
                .ok_or_else(|| anyhow::anyhow!("Team message artifact missing"))?
        } else {
            append_agent_team_message(working_dir, team_id, target, kind, message)?
        };
        Ok(ToolResult::success_with_metadata(
            format!("Team message recorded: {}", path.display()),
            json!({
                "team_id": team_id,
                "target": target,
                "kind": kind,
                "message_artifact": path.display().to_string(),
            }),
        ))
    }
}

#[async_trait]
impl Tool for TeamReceiveTool {
    fn name(&self) -> &str {
        "team_receive"
    }

    fn user_facing_name(&self) -> &str {
        "Team Inbox"
    }

    fn description(&self) -> &str {
        "Read pending messages for a specific agent team member. Team members should call this while running to receive handoffs or user guidance sent after launch."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "team_id": { "type": "string" },
                "member_id": { "type": "string" },
                "max_items": { "type": "integer", "minimum": 1, "default": 8 },
                "consume": { "type": "boolean", "default": true }
            },
            "required": ["team_id", "member_id"]
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
        let working_dir = ctx
            .working_dir
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Working directory not set"))?;
        let team_id = params
            .get("team_id")
            .and_then(|value| value.as_str())
            .ok_or_else(|| anyhow::anyhow!("'team_id' parameter is required"))?;
        let member_id = params
            .get("member_id")
            .and_then(|value| value.as_str())
            .ok_or_else(|| anyhow::anyhow!("'member_id' parameter is required"))?;
        let max_items = params
            .get("max_items")
            .and_then(|value| value.as_u64())
            .map(|value| value.max(1) as usize)
            .unwrap_or(8);
        let consume = params
            .get("consume")
            .and_then(|value| value.as_bool())
            .unwrap_or(true);

        let messages = if let Some(manager) = ctx.team_runtime.as_ref() {
            let snapshot = {
                let mut manager = manager.lock().await;
                hydrate_agent_team_manager(working_dir, &mut manager, team_id)?
                    .ok_or_else(|| anyhow::anyhow!("Team '{}' not found.", team_id))?;
                let messages = if consume {
                    manager.consume_message_context(team_id, member_id, max_items)
                } else {
                    manager.message_context(team_id, member_id, max_items)
                };
                (messages, manager.snapshot(team_id))
            };
            if let Some(snapshot) = snapshot.1.as_ref() {
                let _ = persist_agent_team_snapshot(working_dir, snapshot);
            }
            snapshot.0
        } else if consume {
            consume_agent_team_messages(working_dir, team_id, member_id, max_items)?
        } else {
            load_team_messages(working_dir, team_id)
                .unwrap_or_default()
                .into_iter()
                .filter(|message| message.target == "all" || message.target == member_id)
                .rev()
                .take(max_items)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect()
        };

        let content = if messages.is_empty() {
            "No team messages.".to_string()
        } else {
            messages
                .iter()
                .map(|message| {
                    format!(
                        "{} [{}:{}] {}",
                        message.at, message.target, message.kind, message.message
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        };
        Ok(ToolResult::success_with_metadata(
            content,
            json!({
                "team_id": team_id,
                "member_id": member_id,
                "message_count": messages.len(),
                "consumed": consume,
            }),
        ))
    }
}

#[async_trait]
impl Tool for TeamMonitorTool {
    fn name(&self) -> &str {
        "team_monitor"
    }

    fn user_facing_name(&self) -> &str {
        "Team Monitor"
    }

    fn description(&self) -> &str {
        "Inspect the latest agent team state and background member progress."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "team_id": { "type": "string" },
                "include_messages": { "type": "boolean", "default": false }
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
        let working_dir = ctx
            .working_dir
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Working directory not set"))?;
        let team_id = params.get("team_id").and_then(|value| value.as_str());
        let include_messages = params
            .get("include_messages")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        let (rendered, artifacts) =
            if let (Some(team_id), Some(manager)) = (team_id, ctx.team_runtime.as_ref()) {
                let snapshot = {
                    let mut manager = manager.lock().await;
                    hydrate_agent_team_manager(working_dir, &mut manager, team_id)?
                        .ok_or_else(|| anyhow::anyhow!("Team '{}' not found.", team_id))?
                };
                (
                    render_agent_team_monitor_from_snapshot(
                        &snapshot,
                        ctx.runtime_tasks.as_ref(),
                        include_messages,
                    )?,
                    Some(persist_agent_team_snapshot(working_dir, &snapshot)?),
                )
            } else {
                (
                    render_agent_team_monitor(
                        working_dir,
                        team_id,
                        ctx.runtime_tasks.as_ref(),
                        include_messages,
                    )?,
                    if let Some(team_id) = team_id {
                        load_agent_team_state(working_dir, team_id)?
                            .map(|state| write_agent_team_state(working_dir, &state))
                            .transpose()?
                    } else {
                        None
                    },
                )
            };
        Ok(ToolResult::success_with_metadata(
            rendered,
            json!({
                "team_id": team_id,
                "team_monitor_artifact": artifacts
                    .as_ref()
                    .and_then(|set| set.monitor_path.as_ref())
                    .map(|path| path.display().to_string()),
                "team_bundle_artifact": artifacts
                    .as_ref()
                    .and_then(|set| set.bundle_path.as_ref())
                    .map(|path| path.display().to_string()),
            }),
        ))
    }
}

fn teams_dir(working_dir: &Path) -> PathBuf {
    working_dir.join(".yode").join("teams")
}

fn team_state_path(working_dir: &Path, team_id: &str) -> PathBuf {
    teams_dir(working_dir).join(format!("{}-agent-team-state.json", sanitize_id(team_id)))
}

fn team_summary_path(working_dir: &Path, team_id: &str) -> PathBuf {
    teams_dir(working_dir).join(format!("{}-agent-team.md", sanitize_id(team_id)))
}

fn team_messages_path(working_dir: &Path, team_id: &str) -> PathBuf {
    teams_dir(working_dir).join(format!("{}-agent-team-messages.md", sanitize_id(team_id)))
}

fn team_monitor_path(working_dir: &Path, team_id: &str) -> PathBuf {
    teams_dir(working_dir).join(format!("{}-agent-team-monitor.md", sanitize_id(team_id)))
}

fn team_bundle_path(working_dir: &Path, team_id: &str) -> PathBuf {
    teams_dir(working_dir).join(format!("{}-agent-team-bundle.md", sanitize_id(team_id)))
}

fn write_agent_team_state(
    working_dir: &Path,
    state: &AgentTeamState,
) -> Result<AgentTeamArtifactSet> {
    let dir = teams_dir(working_dir);
    std::fs::create_dir_all(&dir)?;
    let state_path = team_state_path(working_dir, &state.team_id);
    let summary_path = team_summary_path(working_dir, &state.team_id);
    let messages_path = team_messages_path(working_dir, &state.team_id);
    let monitor_path = team_monitor_path(working_dir, &state.team_id);
    let bundle_path = team_bundle_path(working_dir, &state.team_id);
    let state_json = serde_json::to_string_pretty(state)?;
    std::fs::write(&state_path, state_json)?;
    std::fs::write(&summary_path, render_team_summary(state))?;
    std::fs::write(
        &monitor_path,
        render_agent_team_monitor(working_dir, Some(&state.team_id), None, true)?,
    )?;
    std::fs::write(
        &bundle_path,
        render_team_bundle(state, messages_path.as_path()),
    )?;
    if !messages_path.exists() {
        std::fs::write(&messages_path, "# Agent Team Messages\n\n- none\n")?;
    }
    Ok(AgentTeamArtifactSet {
        summary_path: Some(summary_path),
        state_path: Some(state_path),
        messages_path: Some(messages_path),
        monitor_path: Some(monitor_path),
        bundle_path: Some(bundle_path),
    })
}

fn render_team_summary(state: &AgentTeamState) -> String {
    let mut lines = vec![
        "# Agent Team".to_string(),
        String::new(),
        format!("- Team: {}", state.team_id),
        format!("- Goal: {}", state.goal),
        format!("- Mode: {}", state.mode),
        format!(
            "- Members: {} total / {} active / {} completed / {} failed",
            state.member_count, state.active_count, state.completed_count, state.failed_count
        ),
        format!("- Updated at: {}", state.updated_at),
        format!("- Messages: {}", state.latest_message_count),
        String::new(),
        "Members:".to_string(),
    ];
    for member in &state.members {
        lines.push(format!(
            "- {} [{}] tools={} inbox={} inheritance={}{}{}",
            member.member_id,
            member.status,
            if member.allowed_tools.is_empty() {
                "inherit".to_string()
            } else {
                member.allowed_tools.join(",")
            },
            member.pending_message_count,
            member.permission_inheritance,
            member
                .runtime_task_id
                .as_ref()
                .map(|task_id| format!(" / task={}", task_id))
                .unwrap_or_default(),
            member
                .result_artifact_path
                .as_ref()
                .map(|path| format!(" / artifact={}", path))
                .unwrap_or_default(),
        ));
    }
    lines.join("\n")
}

fn render_team_bundle(state: &AgentTeamState, messages_path: &Path) -> String {
    let messages = std::fs::read_to_string(messages_path)
        .unwrap_or_else(|_| "# Agent Team Messages\n\n- none\n".to_string());
    format!(
        "# Agent Team Bundle\n\n- Team: {}\n- Goal: {}\n- State updated: {}\n- Message artifact: {}\n\n## Team Summary\n\n{}\n\n## Messages\n\n{}\n",
        state.team_id,
        state.goal,
        state.updated_at,
        messages_path.display(),
        render_team_summary(state),
        messages
    )
}

fn team_member_from_input(input: TeamMemberInput) -> AgentTeamMemberState {
    AgentTeamMemberState {
        member_id: input.id,
        description: input.description,
        subagent_type: input.subagent_type,
        model: input.model,
        run_in_background: input.run_in_background.unwrap_or(true),
        allowed_tools: input.allowed_tools.clone(),
        permission_inheritance: if input.allowed_tools.is_empty() {
            "parent_tool_pool".to_string()
        } else {
            "explicit_allowlist".to_string()
        },
        status: "planned".to_string(),
        runtime_task_id: None,
        last_result_preview: None,
        result_artifact_path: None,
        last_updated_at: Some(now_string()),
        pending_message_count: 0,
        last_message_at: None,
    }
}

fn load_team_messages(working_dir: &Path, team_id: &str) -> Option<Vec<AgentTeamMessage>> {
    let path = team_messages_path(working_dir, team_id);
    let body = std::fs::read_to_string(path).ok()?;
    let mut messages = Vec::new();
    for line in body.lines() {
        let line = line.trim();
        if !line.starts_with("- ") {
            continue;
        }
        let parts = line
            .trim_start_matches("- ")
            .split(" | ")
            .collect::<Vec<_>>();
        if parts.len() < 4 {
            continue;
        }
        messages.push(AgentTeamMessage {
            at: parts[0].to_string(),
            target: parts[1].to_string(),
            kind: parts[2].to_string(),
            message: parts[3..].join(" | "),
        });
    }
    Some(messages)
}

fn default_team_id(goal: &str) -> String {
    let slug = sanitize_id(goal);
    if slug.is_empty() {
        format!("team-{}", chrono::Local::now().format("%Y%m%d-%H%M%S"))
    } else {
        format!("team-{}", slug)
    }
}

fn sanitize_id(raw: &str) -> String {
    raw.chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

fn truncate_preview(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        text.to_string()
    } else {
        format!("{}...", text.chars().take(max_chars).collect::<String>())
    }
}

fn now_string() -> String {
    chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

#[cfg(test)]
mod tests {
    use yode_agent::AgentTeamManager;

    use super::{
        append_agent_team_message, latest_agent_team_file, persist_agent_team_runtime,
        render_agent_team_monitor, update_agent_team_member, AgentTeamMemberState, SendMessageTool,
        TeamCreateTool, TeamMonitorTool, TeamReceiveTool,
    };
    use crate::runtime_tasks::RuntimeTaskStore;
    use crate::tool::{Tool, ToolContext};
    use serde_json::json;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    #[test]
    fn persist_team_runtime_writes_summary_and_state() {
        let dir = tempfile::tempdir().unwrap();
        let artifacts = persist_agent_team_runtime(
            dir.path(),
            "ship feature",
            Some("team-demo"),
            "manual",
            vec![AgentTeamMemberState {
                member_id: "review".to_string(),
                description: "review".to_string(),
                subagent_type: None,
                model: None,
                run_in_background: true,
                allowed_tools: vec!["read_file".to_string()],
                permission_inheritance: "explicit_allowlist".to_string(),
                status: "planned".to_string(),
                runtime_task_id: None,
                last_result_preview: None,
                result_artifact_path: None,
                last_updated_at: Some("2026-01-01 00:00:00".to_string()),
                pending_message_count: 0,
                last_message_at: None,
            }],
        )
        .unwrap();
        assert!(artifacts.summary_path.unwrap().exists());
        assert!(artifacts.state_path.unwrap().exists());
        assert!(latest_agent_team_file(dir.path(), "agent-team-state.json").is_some());
    }

    #[test]
    fn update_member_and_messages_refresh_monitor() {
        let dir = tempfile::tempdir().unwrap();
        persist_agent_team_runtime(
            dir.path(),
            "ship feature",
            Some("team-demo"),
            "manual",
            vec![AgentTeamMemberState {
                member_id: "review".to_string(),
                description: "review".to_string(),
                subagent_type: None,
                model: None,
                run_in_background: true,
                allowed_tools: vec![],
                permission_inheritance: "parent_tool_pool".to_string(),
                status: "planned".to_string(),
                runtime_task_id: None,
                last_result_preview: None,
                result_artifact_path: None,
                last_updated_at: Some("2026-01-01 00:00:00".to_string()),
                pending_message_count: 0,
                last_message_at: None,
            }],
        )
        .unwrap();
        update_agent_team_member(
            dir.path(),
            "team-demo",
            "review",
            "running",
            Some("task-1".to_string()),
            Some("started".to_string()),
            None,
        )
        .unwrap();
        append_agent_team_message(dir.path(), "team-demo", "review", "handoff", "check tests")
            .unwrap();
        let rendered =
            render_agent_team_monitor(dir.path(), Some("team-demo"), None, true).unwrap();
        assert!(rendered.contains("task-1"));
        assert!(rendered.contains("check tests"));
    }

    #[tokio::test]
    async fn tools_create_message_and_monitor_team() {
        let dir = tempfile::tempdir().unwrap();
        let mut ctx = ToolContext::empty();
        ctx.working_dir = Some(dir.path().to_path_buf());
        ctx.runtime_tasks = Some(Arc::new(Mutex::new(RuntimeTaskStore::new())));

        let create = TeamCreateTool;
        let create_result = create
            .execute(
                json!({
                    "goal": "ship feature",
                    "team_id": "team-demo",
                    "members": [
                        { "id": "review", "description": "review" }
                    ]
                }),
                &ctx,
            )
            .await
            .unwrap();
        assert!(!create_result.is_error);

        let send = SendMessageTool;
        let _ = send
            .execute(
                json!({
                    "team_id": "team-demo",
                    "target": "review",
                    "kind": "handoff",
                    "message": "focus on risk"
                }),
                &ctx,
            )
            .await
            .unwrap();

        let monitor = TeamMonitorTool;
        let monitor_result = monitor
            .execute(
                json!({
                    "team_id": "team-demo",
                    "include_messages": true
                }),
                &ctx,
            )
            .await
            .unwrap();
        assert!(monitor_result.content.contains("focus on risk"));
        assert!(monitor_result.content.contains("Agent Team Monitor"));
    }

    #[tokio::test]
    async fn send_message_returns_artifact_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let mut ctx = ToolContext::empty();
        ctx.working_dir = Some(dir.path().to_path_buf());

        TeamCreateTool
            .execute(
                json!({
                    "goal": "ship feature",
                    "team_id": "team-demo",
                    "members": [{ "id": "review", "description": "review" }]
                }),
                &ctx,
            )
            .await
            .unwrap();

        let result = SendMessageTool
            .execute(
                json!({
                    "team_id": "team-demo",
                    "target": "review",
                    "kind": "handoff",
                    "message": "focus on tests"
                }),
                &ctx,
            )
            .await
            .unwrap();

        assert!(!result.is_error);
        assert!(result.metadata.as_ref().unwrap()["message_artifact"]
            .as_str()
            .unwrap()
            .contains("agent-team-messages"));
    }

    #[tokio::test]
    async fn team_receive_consumes_member_inbox() {
        let dir = tempfile::tempdir().unwrap();
        let mut ctx = ToolContext::empty();
        ctx.working_dir = Some(dir.path().to_path_buf());

        TeamCreateTool
            .execute(
                json!({
                    "goal": "ship feature",
                    "team_id": "team-demo",
                    "members": [{ "id": "review", "description": "review" }]
                }),
                &ctx,
            )
            .await
            .unwrap();
        SendMessageTool
            .execute(
                json!({
                    "team_id": "team-demo",
                    "target": "review",
                    "message": "focus on tests"
                }),
                &ctx,
            )
            .await
            .unwrap();

        let result = TeamReceiveTool
            .execute(
                json!({
                    "team_id": "team-demo",
                    "member_id": "review"
                }),
                &ctx,
            )
            .await
            .unwrap();

        assert!(result.content.contains("focus on tests"));
        let state = super::load_agent_team_state(dir.path(), "team-demo")
            .unwrap()
            .unwrap();
        assert_eq!(state.members[0].pending_message_count, 0);
    }

    #[tokio::test]
    async fn team_monitor_errors_for_missing_team() {
        let dir = tempfile::tempdir().unwrap();
        let mut ctx = ToolContext::empty();
        ctx.working_dir = Some(dir.path().to_path_buf());

        let result = TeamMonitorTool
            .execute(
                json!({
                    "team_id": "missing",
                    "include_messages": true
                }),
                &ctx,
            )
            .await;

        assert!(result.is_err());
        assert!(format!("{}", result.unwrap_err()).contains("not found"));
    }

    #[tokio::test]
    async fn team_tools_update_live_manager_when_available() {
        let dir = tempfile::tempdir().unwrap();
        let mut ctx = ToolContext::empty();
        ctx.working_dir = Some(dir.path().to_path_buf());
        ctx.team_runtime = Some(Arc::new(Mutex::new(AgentTeamManager::new())));

        TeamCreateTool
            .execute(
                json!({
                    "goal": "ship feature",
                    "team_id": "team-demo",
                    "members": [{ "id": "review", "description": "review" }]
                }),
                &ctx,
            )
            .await
            .unwrap();
        SendMessageTool
            .execute(
                json!({
                    "team_id": "team-demo",
                    "target": "review",
                    "message": "focus on risk"
                }),
                &ctx,
            )
            .await
            .unwrap();

        let snapshot = ctx
            .team_runtime
            .as_ref()
            .unwrap()
            .lock()
            .await
            .snapshot("team-demo")
            .unwrap();
        assert_eq!(snapshot.messages.len(), 1);
        assert_eq!(snapshot.state.as_ref().unwrap().team_id, "team-demo");
    }
}
