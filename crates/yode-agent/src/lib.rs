use std::collections::BTreeMap;

use anyhow::{anyhow, Result};
use chrono::Local;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTeamMemberState {
    pub member_id: String,
    pub description: String,
    #[serde(default)]
    pub subagent_type: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    pub run_in_background: bool,
    #[serde(default)]
    pub allowed_tools: Vec<String>,
    pub permission_inheritance: String,
    pub status: String,
    #[serde(default)]
    pub runtime_task_id: Option<String>,
    #[serde(default)]
    pub last_result_preview: Option<String>,
    #[serde(default)]
    pub result_artifact_path: Option<String>,
    #[serde(default)]
    pub last_updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTeamMessage {
    pub at: String,
    pub target: String,
    pub kind: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTeamState {
    pub kind: String,
    pub team_id: String,
    pub goal: String,
    pub mode: String,
    pub created_at: String,
    pub updated_at: String,
    pub member_count: usize,
    pub active_count: usize,
    pub completed_count: usize,
    pub failed_count: usize,
    pub latest_message_count: usize,
    #[serde(default)]
    pub latest_message_artifact: Option<String>,
    #[serde(default)]
    pub latest_bundle_artifact: Option<String>,
    pub members: Vec<AgentTeamMemberState>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentTeamSnapshot {
    pub state: Option<AgentTeamState>,
    pub messages: Vec<AgentTeamMessage>,
}

#[derive(Debug, Default)]
pub struct AgentTeamManager {
    teams: BTreeMap<String, AgentTeamSnapshot>,
    latest_team_id: Option<String>,
}

impl AgentTeamManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn latest_team_id(&self) -> Option<&str> {
        self.latest_team_id.as_deref()
    }

    pub fn ensure_team(
        &mut self,
        goal: &str,
        team_id: Option<&str>,
        mode: &str,
        mut members: Vec<AgentTeamMemberState>,
    ) -> AgentTeamState {
        let team_id = team_id
            .filter(|id| !id.trim().is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| default_team_id(goal));
        for member in &mut members {
            if member.last_updated_at.is_none() {
                member.last_updated_at = Some(now_string());
            }
        }
        let snapshot = self.teams.entry(team_id.clone()).or_default();
        let state = AgentTeamState {
            kind: "agent_team".to_string(),
            team_id: team_id.clone(),
            goal: goal.to_string(),
            mode: mode.to_string(),
            created_at: snapshot
                .state
                .as_ref()
                .map(|state| state.created_at.clone())
                .unwrap_or_else(now_string),
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
            latest_message_count: snapshot.messages.len(),
            latest_message_artifact: snapshot
                .state
                .as_ref()
                .and_then(|state| state.latest_message_artifact.clone()),
            latest_bundle_artifact: snapshot
                .state
                .as_ref()
                .and_then(|state| state.latest_bundle_artifact.clone()),
            members,
        };
        snapshot.state = Some(state.clone());
        self.latest_team_id = Some(team_id);
        state
    }

    pub fn update_member(
        &mut self,
        team_id: &str,
        member_id: &str,
        status: &str,
        runtime_task_id: Option<String>,
        result_preview: Option<String>,
        result_artifact_path: Option<String>,
    ) -> Result<AgentTeamState> {
        let snapshot = self
            .teams
            .get_mut(team_id)
            .ok_or_else(|| anyhow!("Team '{}' not found.", team_id))?;
        let state = snapshot
            .state
            .as_mut()
            .ok_or_else(|| anyhow!("Team '{}' state missing.", team_id))?;
        let member = state
            .members
            .iter_mut()
            .find(|member| member.member_id == member_id)
            .ok_or_else(|| anyhow!("Member '{}' not found in team '{}'.", member_id, team_id))?;
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
        self.latest_team_id = Some(team_id.to_string());
        Ok(state.clone())
    }

    pub fn append_message(
        &mut self,
        team_id: &str,
        target: &str,
        kind: &str,
        message: &str,
    ) -> Result<AgentTeamMessage> {
        let snapshot = self
            .teams
            .entry(team_id.to_string())
            .or_insert_with(AgentTeamSnapshot::default);
        let entry = AgentTeamMessage {
            at: now_string(),
            target: target.to_string(),
            kind: kind.to_string(),
            message: message.to_string(),
        };
        snapshot.messages.push(entry.clone());
        if let Some(state) = snapshot.state.as_mut() {
            state.latest_message_count = snapshot.messages.len();
            state.updated_at = now_string();
        }
        self.latest_team_id = Some(team_id.to_string());
        Ok(entry)
    }

    pub fn snapshot(&self, team_id: &str) -> Option<AgentTeamSnapshot> {
        self.teams.get(team_id).cloned()
    }

    pub fn upsert_snapshot(&mut self, state: AgentTeamState, messages: Vec<AgentTeamMessage>) {
        self.latest_team_id = Some(state.team_id.clone());
        self.teams.insert(
            state.team_id.clone(),
            AgentTeamSnapshot {
                state: Some(state),
                messages,
            },
        );
    }

    pub fn list_team_ids(&self) -> Vec<String> {
        self.teams.keys().cloned().collect()
    }
}

fn default_team_id(goal: &str) -> String {
    let slug = sanitize_id(goal);
    if slug.is_empty() {
        format!("team-{}", Local::now().format("%Y%m%d-%H%M%S"))
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

fn now_string() -> String {
    Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

fn truncate_preview(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        text.to_string()
    } else {
        format!("{}...", text.chars().take(max_chars).collect::<String>())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manager_creates_updates_and_messages() {
        let mut manager = AgentTeamManager::new();
        let state = manager.ensure_team(
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
                last_updated_at: None,
            }],
        );
        assert_eq!(state.team_id, "team-demo");
        let updated = manager
            .update_member("team-demo", "review", "running", Some("task-1".to_string()), None, None)
            .unwrap();
        assert_eq!(updated.active_count, 1);
        manager
            .append_message("team-demo", "review", "handoff", "check tests")
            .unwrap();
        assert_eq!(manager.snapshot("team-demo").unwrap().messages.len(), 1);
        assert_eq!(manager.list_team_ids(), vec!["team-demo".to_string()]);
    }
}
