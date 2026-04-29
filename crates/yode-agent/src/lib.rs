use std::collections::{BTreeMap, BTreeSet};

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

#[derive(Debug, Default)]
pub struct AgentPlanner;

impl AgentPlanner {
    pub fn plan_team(
        goal: &str,
        mode: &str,
        members: &[AgentTeamMemberState],
    ) -> Result<AgentPlan> {
        plan_agent_team(goal, mode, members)
    }
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

pub fn plan_agent_team(
    goal: &str,
    mode: &str,
    members: &[AgentTeamMemberState],
) -> Result<AgentPlan> {
    if members.is_empty() {
        return Err(anyhow!("Agent team planning requires at least one member."));
    }
    let mut seen = BTreeSet::new();
    for member in members {
        let member_id = member.member_id.trim();
        if member_id.is_empty() {
            return Err(anyhow!("Agent team member id cannot be empty."));
        }
        if !seen.insert(member_id.to_string()) {
            return Err(anyhow!("Duplicate agent team member id '{}'.", member_id));
        }
    }

    let plan_mode = AgentPlanMode::from_mode(mode);
    let steps = match plan_mode {
        AgentPlanMode::Sequential => sequential_steps(members),
        AgentPlanMode::Manual | AgentPlanMode::Parallel | AgentPlanMode::ReviewGate => {
            dependency_grouped_steps(members)
        }
    };
    let batches = build_batches(&steps);
    Ok(AgentPlan {
        goal: goal.to_string(),
        mode: plan_mode,
        step_count: steps.len(),
        parallel_batch_count: batches.len(),
        steps,
        batches,
    })
}

fn sequential_steps(members: &[AgentTeamMemberState]) -> Vec<AgentPlanStep> {
    let mut steps = Vec::with_capacity(members.len());
    let mut previous_step_id = None;
    for member in members {
        let step_id = member_step_id(&member.member_id);
        let depends_on = previous_step_id.into_iter().collect::<Vec<_>>();
        steps.push(member_plan_step(member, "sequential", depends_on));
        previous_step_id = Some(step_id);
    }
    steps
}

fn dependency_grouped_steps(members: &[AgentTeamMemberState]) -> Vec<AgentPlanStep> {
    let coordinators = members
        .iter()
        .filter(|member| member_phase(member) == "coordinate")
        .collect::<Vec<_>>();
    let reviewers = members
        .iter()
        .filter(|member| member_phase(member) == "review")
        .collect::<Vec<_>>();
    let workers = members
        .iter()
        .filter(|member| member_phase(member) == "execute")
        .collect::<Vec<_>>();

    let coordinator_ids = coordinators
        .iter()
        .map(|member| member_step_id(&member.member_id))
        .collect::<Vec<_>>();
    let worker_ids = workers
        .iter()
        .map(|member| member_step_id(&member.member_id))
        .collect::<Vec<_>>();

    let mut steps = Vec::with_capacity(members.len());
    for member in coordinators {
        steps.push(member_plan_step(member, "coordinate", Vec::new()));
    }
    for member in workers {
        steps.push(member_plan_step(member, "execute", coordinator_ids.clone()));
    }
    for member in reviewers {
        let depends_on = if worker_ids.is_empty() {
            coordinator_ids.clone()
        } else {
            worker_ids.clone()
        };
        steps.push(member_plan_step(member, "review", depends_on));
    }
    steps
}

fn member_plan_step(
    member: &AgentTeamMemberState,
    phase: &str,
    depends_on: Vec<String>,
) -> AgentPlanStep {
    AgentPlanStep {
        step_id: member_step_id(&member.member_id),
        member_id: Some(member.member_id.clone()),
        title: member.member_id.clone(),
        description: member.description.clone(),
        phase: phase.to_string(),
        depends_on,
        run_in_background: member.run_in_background,
        status: member.status.clone(),
    }
}

fn build_batches(steps: &[AgentPlanStep]) -> Vec<AgentPlanBatch> {
    let mut remaining = steps
        .iter()
        .map(|step| (step.step_id.clone(), step))
        .collect::<BTreeMap<_, _>>();
    let mut completed = BTreeSet::new();
    let mut batches = Vec::new();

    while !remaining.is_empty() {
        let ready = remaining
            .iter()
            .filter(|(_, step)| step.depends_on.iter().all(|id| completed.contains(id)))
            .map(|(step_id, _)| step_id.clone())
            .collect::<Vec<_>>();
        if ready.is_empty() {
            let blocked = remaining.keys().cloned().collect::<Vec<_>>();
            batches.push(AgentPlanBatch {
                batch_id: format!("batch-{}", batches.len() + 1),
                step_ids: blocked,
            });
            break;
        }
        for step_id in &ready {
            remaining.remove(step_id);
            completed.insert(step_id.clone());
        }
        batches.push(AgentPlanBatch {
            batch_id: format!("batch-{}", batches.len() + 1),
            step_ids: ready,
        });
    }

    batches
}

fn member_phase(member: &AgentTeamMemberState) -> &'static str {
    let joined = format!(
        "{} {}",
        member.member_id,
        member.subagent_type.as_deref().unwrap_or_default()
    )
    .to_ascii_lowercase();
    if joined.contains("coordinator") || joined.contains("coordinate") || joined.contains("plan") {
        "coordinate"
    } else if joined.contains("review")
        || joined.contains("verify")
        || joined.contains("verification")
        || joined.contains("qa")
        || joined.contains("test")
    {
        "review"
    } else {
        "execute"
    }
}

fn member_step_id(member_id: &str) -> String {
    format!("member:{}", sanitize_id(member_id))
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
}
