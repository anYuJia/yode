use std::collections::BTreeMap;

use anyhow::{anyhow, Result};

use crate::{
    default_team_id, now_string, plan_agent_team, sync_agent_team_plan_statuses, truncate_preview,
    AgentTeamMemberState, AgentTeamMessage, AgentTeamSnapshot, AgentTeamState,
};

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
        let previous_members = snapshot
            .state
            .as_ref()
            .map(|state| {
                state
                    .members
                    .iter()
                    .map(|member| (member.member_id.clone(), member.clone()))
                    .collect::<BTreeMap<_, _>>()
            })
            .unwrap_or_default();
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
                .filter(|member| matches!(member.status.as_str(), "planned" | "queued" | "running"))
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
            plan: plan_agent_team(goal, mode, &members).ok(),
            members: members
                .drain(..)
                .map(|mut member| {
                    if let Some(previous) = previous_members.get(&member.member_id) {
                        member.runtime_task_id = member
                            .runtime_task_id
                            .or_else(|| previous.runtime_task_id.clone());
                        member.last_result_preview = member
                            .last_result_preview
                            .or_else(|| previous.last_result_preview.clone());
                        member.result_artifact_path = member
                            .result_artifact_path
                            .or_else(|| previous.result_artifact_path.clone());
                        if member.last_updated_at.is_none() {
                            member.last_updated_at = previous.last_updated_at.clone();
                        }
                        if member.pending_message_count == 0 {
                            member.pending_message_count = previous.pending_message_count;
                        }
                        if member.last_message_at.is_none() {
                            member.last_message_at = previous.last_message_at.clone();
                        }
                    }
                    member
                })
                .collect(),
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
            .filter(|member| matches!(member.status.as_str(), "planned" | "queued" | "running"))
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
        sync_agent_team_plan_statuses(state);
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
        let snapshot = self.teams.entry(team_id.to_string()).or_default();
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
            for member in &mut state.members {
                if target == "all" || target == member.member_id {
                    member.pending_message_count = member.pending_message_count.saturating_add(1);
                    member.last_message_at = Some(entry.at.clone());
                }
            }
        }
        self.latest_team_id = Some(team_id.to_string());
        Ok(entry)
    }

    pub fn snapshot(&self, team_id: &str) -> Option<AgentTeamSnapshot> {
        self.teams.get(team_id).cloned()
    }

    pub fn delete_team(&mut self, team_id: &str) -> Option<AgentTeamSnapshot> {
        let removed = self.teams.remove(team_id);
        if self.latest_team_id.as_deref() == Some(team_id) {
            self.latest_team_id = self.teams.keys().next_back().cloned();
        }
        removed
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

    pub fn message_context(
        &self,
        team_id: &str,
        member_id: &str,
        max_items: usize,
    ) -> Vec<AgentTeamMessage> {
        self.teams
            .get(team_id)
            .map(|snapshot| {
                snapshot
                    .messages
                    .iter()
                    .filter(|message| message.target == "all" || message.target == member_id)
                    .rev()
                    .take(max_items.max(1))
                    .cloned()
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn consume_message_context(
        &mut self,
        team_id: &str,
        member_id: &str,
        max_items: usize,
    ) -> Vec<AgentTeamMessage> {
        let messages = self.message_context(team_id, member_id, max_items);
        if messages.is_empty() {
            return messages;
        }
        if let Some(state) = self
            .teams
            .get_mut(team_id)
            .and_then(|snapshot| snapshot.state.as_mut())
        {
            if let Some(member) = state
                .members
                .iter_mut()
                .find(|member| member.member_id == member_id)
            {
                member.pending_message_count = 0;
                member.last_updated_at = Some(now_string());
            }
            state.updated_at = now_string();
        }
        self.latest_team_id = Some(team_id.to_string());
        messages
    }
}
