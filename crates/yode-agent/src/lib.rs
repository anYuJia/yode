mod manager;
mod orchestration;
mod planning;

use anyhow::Result;
use async_trait::async_trait;
use chrono::Local;
use serde::{Deserialize, Serialize};

pub use manager::AgentTeamManager;
pub use orchestration::{build_agent_run_request, run_ready_agent_steps};
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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentTeamSnapshot {
    pub state: Option<AgentTeamState>,
    pub messages: Vec<AgentTeamMessage>,
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
    use anyhow::anyhow;

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

    #[test]
    fn manager_counts_queued_members_as_active() {
        let mut manager = AgentTeamManager::new();
        let state = manager.ensure_team(
            "ship feature",
            Some("team-demo"),
            "manual",
            vec![AgentTeamMemberState {
                status: "queued".to_string(),
                ..member("api", Some("worker"))
            }],
        );

        assert_eq!(state.active_count, 1);
    }

    #[test]
    fn cancelled_plan_steps_are_terminal_not_ready() {
        let mut manager = AgentTeamManager::new();
        let state = manager.ensure_team(
            "ship feature",
            Some("team-demo"),
            "parallel",
            vec![AgentTeamMemberState {
                status: "cancelled".to_string(),
                ..member("api", Some("worker"))
            }],
        );
        let progress = evaluate_agent_plan(&state).unwrap();

        assert_eq!(progress.failed_step_ids, vec!["member:api"]);
        assert!(progress.ready_step_ids.is_empty());
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
