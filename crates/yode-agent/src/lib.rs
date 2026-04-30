mod planning;

use std::collections::BTreeMap;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use chrono::Local;
use serde::{Deserialize, Serialize};

pub use planning::{
    evaluate_agent_plan, plan_agent_team, sync_agent_team_plan_statuses, AgentPlanner,
};

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
    #[serde(default)]
    pub pending_message_count: u32,
    #[serde(default)]
    pub last_message_at: Option<String>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan: Option<AgentPlan>,
    pub members: Vec<AgentTeamMemberState>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentPlanMode {
    Manual,
    Parallel,
    Sequential,
    ReviewGate,
}

impl AgentPlanMode {
    pub fn from_mode(mode: &str) -> Self {
        match mode.trim().to_ascii_lowercase().as_str() {
            "parallel" | "fanout" | "fan-out" => Self::Parallel,
            "sequential" | "serial" => Self::Sequential,
            "review" | "review_gate" | "review-gate" => Self::ReviewGate,
            _ => Self::Manual,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentPlanBatch {
    pub batch_id: String,
    pub step_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentPlanStep {
    pub step_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub member_id: Option<String>,
    pub title: String,
    pub description: String,
    pub phase: String,
    pub depends_on: Vec<String>,
    pub run_in_background: bool,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentPlan {
    pub goal: String,
    pub mode: AgentPlanMode,
    pub step_count: usize,
    pub parallel_batch_count: usize,
    pub steps: Vec<AgentPlanStep>,
    pub batches: Vec<AgentPlanBatch>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentPlanProgress {
    pub ready_step_ids: Vec<String>,
    pub blocked_step_ids: Vec<String>,
    pub running_step_ids: Vec<String>,
    pub completed_step_ids: Vec<String>,
    pub failed_step_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentRunStatus {
    Planned,
    Running,
    Completed,
    Failed,
    Cancelled,
}

impl AgentRunStatus {
    pub fn as_member_status(&self) -> &'static str {
        match self {
            Self::Planned => "planned",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRunRequest {
    pub team_id: String,
    pub member_id: String,
    pub goal: String,
    pub prompt: String,
    #[serde(default)]
    pub subagent_type: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    pub run_in_background: bool,
    #[serde(default)]
    pub allowed_tools: Vec<String>,
    #[serde(default)]
    pub messages: Vec<AgentTeamMessage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRunResult {
    pub member_id: String,
    pub status: AgentRunStatus,
    pub summary: String,
    #[serde(default)]
    pub artifact_path: Option<String>,
}

#[async_trait]
pub trait AgentRunner: Send + Sync {
    async fn run(&self, request: AgentRunRequest) -> Result<AgentRunResult>;
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentOrchestrationReport {
    pub team_id: String,
    pub ready_step_ids: Vec<String>,
    pub launched_member_ids: Vec<String>,
    pub blocked_step_ids: Vec<String>,
    pub results: Vec<AgentRunResult>,
}

pub fn build_agent_run_request(
    state: &AgentTeamState,
    member_id: &str,
    messages: Vec<AgentTeamMessage>,
) -> Result<AgentRunRequest> {
    let member = state
        .members
        .iter()
        .find(|member| member.member_id == member_id)
        .ok_or_else(|| {
            anyhow!(
                "Member '{}' not found in team '{}'.",
                member_id,
                state.team_id
            )
        })?;
    Ok(AgentRunRequest {
        team_id: state.team_id.clone(),
        member_id: member.member_id.clone(),
        goal: state.goal.clone(),
        prompt: member.description.clone(),
        subagent_type: member.subagent_type.clone(),
        model: member.model.clone(),
        run_in_background: member.run_in_background,
        allowed_tools: member.allowed_tools.clone(),
        messages,
    })
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

pub async fn run_ready_agent_steps<R: AgentRunner>(
    manager: &mut AgentTeamManager,
    team_id: &str,
    runner: &R,
    max_steps: usize,
) -> Result<AgentOrchestrationReport> {
    let snapshot = manager
        .snapshot(team_id)
        .ok_or_else(|| anyhow!("Team '{}' not found.", team_id))?;
    let state = snapshot
        .state
        .as_ref()
        .ok_or_else(|| anyhow!("Team '{}' state missing.", team_id))?;
    let progress = evaluate_agent_plan(state).unwrap_or_default();
    let ready_members = ready_members_for_progress(state, &progress)
        .into_iter()
        .take(max_steps)
        .collect::<Vec<_>>();

    let mut report = AgentOrchestrationReport {
        team_id: team_id.to_string(),
        ready_step_ids: progress.ready_step_ids,
        launched_member_ids: Vec::new(),
        blocked_step_ids: progress.blocked_step_ids,
        results: Vec::new(),
    };

    for member_id in ready_members {
        manager.update_member(team_id, &member_id, "running", None, None, None)?;
        let messages = manager.consume_message_context(team_id, &member_id, 8);
        let state = manager
            .snapshot(team_id)
            .and_then(|snapshot| snapshot.state)
            .ok_or_else(|| anyhow!("Team '{}' state missing.", team_id))?;
        let request = build_agent_run_request(&state, &member_id, messages)?;
        report.launched_member_ids.push(member_id.clone());

        let mut result = match runner.run(request).await {
            Ok(result) => result,
            Err(error) => AgentRunResult {
                member_id: member_id.clone(),
                status: AgentRunStatus::Failed,
                summary: error.to_string(),
                artifact_path: None,
            },
        };
        if result.member_id != member_id {
            result.member_id = member_id.clone();
        }

        manager.update_member(
            team_id,
            &result.member_id,
            result.status.as_member_status(),
            None,
            Some(result.summary.clone()),
            result.artifact_path.clone(),
        )?;
        report.results.push(result);
    }

    Ok(report)
}

fn ready_members_for_progress(state: &AgentTeamState, progress: &AgentPlanProgress) -> Vec<String> {
    let member_statuses = state
        .members
        .iter()
        .map(|member| (member.member_id.as_str(), member.status.as_str()))
        .collect::<BTreeMap<_, _>>();
    let steps = state
        .plan
        .as_ref()
        .map(|plan| {
            plan.steps
                .iter()
                .map(|step| (step.step_id.as_str(), step))
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();

    progress
        .ready_step_ids
        .iter()
        .filter_map(|step_id| steps.get(step_id.as_str()))
        .filter_map(|step| step.member_id.as_deref())
        .filter(|member_id| {
            matches!(
                member_statuses.get(member_id).copied(),
                Some("planned") | Some("queued")
            )
        })
        .map(str::to_string)
        .collect()
}

fn default_team_id(goal: &str) -> String {
    let slug = sanitize_id(goal);
    if slug.is_empty() {
        format!("team-{}", Local::now().format("%Y%m%d-%H%M%S"))
    } else {
        format!("team-{}", slug)
    }
}

pub(crate) fn sanitize_id(raw: &str) -> String {
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

    fn member(id: &str, subagent_type: Option<&str>) -> AgentTeamMemberState {
        AgentTeamMemberState {
            member_id: id.to_string(),
            description: format!("{} work", id),
            subagent_type: subagent_type.map(str::to_string),
            model: None,
            run_in_background: true,
            allowed_tools: Vec::new(),
            permission_inheritance: "parent_tool_pool".to_string(),
            status: "planned".to_string(),
            runtime_task_id: None,
            last_result_preview: None,
            result_artifact_path: None,
            last_updated_at: None,
            pending_message_count: 0,
            last_message_at: None,
        }
    }

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
                pending_message_count: 0,
                last_message_at: None,
            }],
        );
        assert_eq!(state.team_id, "team-demo");
        assert_eq!(state.plan.as_ref().unwrap().parallel_batch_count, 1);
        let updated = manager
            .update_member(
                "team-demo",
                "review",
                "running",
                Some("task-1".to_string()),
                None,
                None,
            )
            .unwrap();
        assert_eq!(updated.active_count, 1);
        manager
            .append_message("team-demo", "review", "handoff", "check tests")
            .unwrap();
        assert_eq!(manager.snapshot("team-demo").unwrap().messages.len(), 1);
        assert_eq!(manager.list_team_ids(), vec!["team-demo".to_string()]);
        assert!(manager.delete_team("team-demo").is_some());
        assert!(manager.snapshot("team-demo").is_none());
        assert_eq!(manager.latest_team_id(), None);
    }

    #[test]
    fn consuming_messages_clears_member_pending_count() {
        let mut manager = AgentTeamManager::new();
        manager.ensure_team(
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
                status: "running".to_string(),
                runtime_task_id: None,
                last_result_preview: None,
                result_artifact_path: None,
                last_updated_at: None,
                pending_message_count: 0,
                last_message_at: None,
            }],
        );
        manager
            .append_message("team-demo", "review", "message", "check tests")
            .unwrap();
        assert_eq!(
            manager
                .snapshot("team-demo")
                .unwrap()
                .state
                .unwrap()
                .members[0]
                .pending_message_count,
            1
        );

        let messages = manager.consume_message_context("team-demo", "review", 10);
        assert_eq!(messages.len(), 1);
        assert_eq!(
            manager
                .snapshot("team-demo")
                .unwrap()
                .state
                .unwrap()
                .members[0]
                .pending_message_count,
            0
        );
    }

    #[test]
    fn planner_batches_parallel_members_behind_coordinator() {
        let plan = plan_agent_team(
            "ship feature",
            "parallel",
            &[
                member("coordinator", Some("coordinator")),
                member("api", Some("worker")),
                member("ui", Some("worker")),
                member("review", Some("review")),
            ],
        )
        .unwrap();

        assert_eq!(plan.mode, AgentPlanMode::Parallel);
        assert_eq!(plan.step_count, 4);
        assert_eq!(plan.batches.len(), 3);
        assert_eq!(plan.batches[0].step_ids, vec!["member:coordinator"]);
        assert_eq!(plan.batches[1].step_ids, vec!["member:api", "member:ui"]);
        assert_eq!(plan.batches[2].step_ids, vec!["member:review"]);
        let review = plan
            .steps
            .iter()
            .find(|step| step.step_id == "member:review")
            .unwrap();
        assert_eq!(review.depends_on, vec!["member:api", "member:ui"]);
    }

    #[test]
    fn planner_serializes_sequential_members() {
        let plan = AgentPlanner::plan_team(
            "migrate storage",
            "sequential",
            &[
                member("discover", None),
                member("patch", None),
                member("verify", None),
            ],
        )
        .unwrap();

        assert_eq!(plan.mode, AgentPlanMode::Sequential);
        assert_eq!(plan.batches.len(), 3);
        assert_eq!(plan.steps[0].depends_on, Vec::<String>::new());
        assert_eq!(plan.steps[1].depends_on, vec!["member:discover"]);
        assert_eq!(plan.steps[2].depends_on, vec!["member:patch"]);
    }

    #[test]
    fn planner_rejects_empty_or_duplicate_members() {
        assert!(plan_agent_team("empty", "parallel", &[]).is_err());
        assert!(plan_agent_team(
            "duplicate",
            "parallel",
            &[member("worker", None), member("worker", Some("review"))],
        )
        .is_err());
    }

    #[test]
    fn plan_progress_marks_ready_and_blocked_steps() {
        let mut manager = AgentTeamManager::new();
        let state = manager.ensure_team(
            "ship feature",
            Some("team-demo"),
            "parallel",
            vec![
                member("coordinator", Some("coordinator")),
                member("api", Some("worker")),
                member("ui", Some("worker")),
                member("review", Some("review")),
            ],
        );
        let progress = evaluate_agent_plan(&state).unwrap();
        assert_eq!(progress.ready_step_ids, vec!["member:coordinator"]);
        assert_eq!(
            progress.blocked_step_ids,
            vec!["member:api", "member:ui", "member:review"]
        );

        let state = manager
            .update_member("team-demo", "coordinator", "completed", None, None, None)
            .unwrap();
        let progress = evaluate_agent_plan(&state).unwrap();
        assert_eq!(progress.completed_step_ids, vec!["member:coordinator"]);
        assert_eq!(progress.ready_step_ids, vec!["member:api", "member:ui"]);
        assert_eq!(progress.blocked_step_ids, vec!["member:review"]);
    }

    struct EchoRunner;

    #[async_trait]
    impl AgentRunner for EchoRunner {
        async fn run(&self, request: AgentRunRequest) -> Result<AgentRunResult> {
            Ok(AgentRunResult {
                member_id: request.member_id,
                status: AgentRunStatus::Completed,
                summary: format!("{} messages", request.messages.len()),
                artifact_path: None,
            })
        }
    }

    struct WrongMemberRunner;

    #[async_trait]
    impl AgentRunner for WrongMemberRunner {
        async fn run(&self, _request: AgentRunRequest) -> Result<AgentRunResult> {
            Ok(AgentRunResult {
                member_id: "wrong-member".to_string(),
                status: AgentRunStatus::Completed,
                summary: "done".to_string(),
                artifact_path: None,
            })
        }
    }

    struct FailingRunner;

    #[async_trait]
    impl AgentRunner for FailingRunner {
        async fn run(&self, _request: AgentRunRequest) -> Result<AgentRunResult> {
            Err(anyhow!("runner failed"))
        }
    }

    #[tokio::test]
    async fn agent_runner_boundary_uses_team_member_context() {
        let state = AgentTeamState {
            kind: "agent_team".to_string(),
            team_id: "team-demo".to_string(),
            goal: "ship feature".to_string(),
            mode: "parallel".to_string(),
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
            member_count: 1,
            active_count: 1,
            completed_count: 0,
            failed_count: 0,
            latest_message_count: 1,
            latest_message_artifact: None,
            latest_bundle_artifact: None,
            plan: None,
            members: vec![member("api", Some("worker"))],
        };
        let request = build_agent_run_request(
            &state,
            "api",
            vec![AgentTeamMessage {
                at: "now".to_string(),
                target: "api".to_string(),
                kind: "handoff".to_string(),
                message: "check API".to_string(),
            }],
        )
        .unwrap();

        assert_eq!(request.goal, "ship feature");
        assert_eq!(request.prompt, "api work");
        assert_eq!(request.subagent_type.as_deref(), Some("worker"));
        assert!(request.run_in_background);

        let result = EchoRunner.run(request).await.unwrap();
        assert_eq!(result.member_id, "api");
        assert_eq!(result.status.as_member_status(), "completed");
        assert_eq!(result.summary, "1 messages");
    }

    #[tokio::test]
    async fn orchestrator_runs_ready_members_and_updates_plan() {
        let mut manager = AgentTeamManager::new();
        manager.ensure_team(
            "ship feature",
            Some("team-demo"),
            "parallel",
            vec![
                member("coordinator", Some("coordinator")),
                member("api", Some("worker")),
                member("ui", Some("worker")),
            ],
        );
        manager
            .append_message("team-demo", "coordinator", "handoff", "plan first")
            .unwrap();

        let report = run_ready_agent_steps(&mut manager, "team-demo", &EchoRunner, 4)
            .await
            .unwrap();
        assert_eq!(report.launched_member_ids, vec!["coordinator"]);
        assert_eq!(report.results[0].summary, "1 messages");

        let state = manager.snapshot("team-demo").unwrap().state.unwrap();
        assert_eq!(
            state
                .members
                .iter()
                .find(|member| member.member_id == "coordinator")
                .unwrap()
                .status,
            "completed"
        );
        let progress = evaluate_agent_plan(&state).unwrap();
        assert_eq!(progress.ready_step_ids, vec!["member:api", "member:ui"]);

        let report = run_ready_agent_steps(&mut manager, "team-demo", &EchoRunner, 4)
            .await
            .unwrap();
        assert_eq!(report.launched_member_ids, vec!["api", "ui"]);
        let state = manager.snapshot("team-demo").unwrap().state.unwrap();
        assert_eq!(state.completed_count, 3);
    }

    #[tokio::test]
    async fn orchestrator_normalizes_runner_member_id() {
        let mut manager = AgentTeamManager::new();
        manager.ensure_team(
            "ship feature",
            Some("team-demo"),
            "parallel",
            vec![member("api", Some("worker"))],
        );

        let report = run_ready_agent_steps(&mut manager, "team-demo", &WrongMemberRunner, 1)
            .await
            .unwrap();
        assert_eq!(report.results[0].member_id, "api");
        let state = manager.snapshot("team-demo").unwrap().state.unwrap();
        assert_eq!(state.members[0].member_id, "api");
        assert_eq!(state.members[0].status, "completed");
    }

    #[tokio::test]
    async fn orchestrator_marks_runner_errors_as_failed_results() {
        let mut manager = AgentTeamManager::new();
        manager.ensure_team(
            "ship feature",
            Some("team-demo"),
            "parallel",
            vec![member("api", Some("worker"))],
        );

        let report = run_ready_agent_steps(&mut manager, "team-demo", &FailingRunner, 1)
            .await
            .unwrap();
        assert_eq!(report.results[0].member_id, "api");
        assert_eq!(report.results[0].status, AgentRunStatus::Failed);
        assert_eq!(report.results[0].summary, "runner failed");
        let state = manager.snapshot("team-demo").unwrap().state.unwrap();
        assert_eq!(state.members[0].status, "failed");
        assert_eq!(state.failed_count, 1);
    }
}
