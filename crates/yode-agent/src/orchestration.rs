use std::collections::BTreeMap;

use anyhow::{anyhow, Result};
use futures::future::join_all;

use crate::{
    evaluate_agent_plan, AgentOrchestrationReport, AgentPlanProgress, AgentRunRequest,
    AgentRunResult, AgentRunStatus, AgentRunner, AgentTeamManager, AgentTeamMessage,
    AgentTeamState,
};

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

    // Stage the entire ready batch before awaiting any worker. AgentTeamManager is deliberately
    // mutated only on the coordinator task; workers receive immutable requests and may run truly
    // concurrently without sharing mutable team state.
    let mut staged = Vec::with_capacity(ready_members.len());
    for member_id in ready_members {
        let messages = manager.consume_message_context(team_id, &member_id, 8);
        let state = manager
            .snapshot(team_id)
            .and_then(|snapshot| snapshot.state)
            .ok_or_else(|| anyhow!("Team '{}' state missing.", team_id))?;
        let request = build_agent_run_request(&state, &member_id, messages)?;
        manager.update_member(team_id, &member_id, "running", None, None, None)?;
        report.launched_member_ids.push(member_id.clone());
        staged.push((member_id, request));
    }

    let completed = join_all(staged.into_iter().map(|(member_id, request)| async move {
        let result = match runner.run(request).await {
            Ok(mut result) => {
                if result.member_id != member_id {
                    result.member_id = member_id.clone();
                }
                result
            }
            Err(error) => AgentRunResult {
                member_id: member_id.clone(),
                status: AgentRunStatus::Failed,
                summary: error.to_string(),
                artifact_path: None,
            },
        };
        (member_id, result)
    }))
    .await;

    // Reconcile results after the concurrent batch completes. join_all preserves staging order,
    // so reports remain deterministic while execution is parallel.
    for (member_id, mut result) in completed {
        if result.member_id != member_id {
            result.member_id = member_id;
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use async_trait::async_trait;
    use tokio::sync::Barrier;

    use super::*;
    use crate::AgentTeamMemberState;

    struct BarrierRunner {
        barrier: Arc<Barrier>,
    }

    #[async_trait]
    impl AgentRunner for BarrierRunner {
        async fn run(&self, request: AgentRunRequest) -> Result<AgentRunResult> {
            self.barrier.wait().await;
            Ok(AgentRunResult {
                member_id: request.member_id,
                status: AgentRunStatus::Completed,
                summary: "parallel complete".to_string(),
                artifact_path: None,
            })
        }
    }

    fn worker(id: &str) -> AgentTeamMemberState {
        AgentTeamMemberState {
            member_id: id.to_string(),
            description: format!("work on {id}"),
            subagent_type: Some("worker".to_string()),
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

    #[tokio::test]
    async fn ready_batch_executes_concurrently() {
        let mut manager = AgentTeamManager::new();
        manager.ensure_team(
            "parallel feature",
            Some("team-parallel"),
            "parallel",
            vec![worker("api"), worker("ui")],
        );
        let runner = BarrierRunner {
            barrier: Arc::new(Barrier::new(2)),
        };

        let report = tokio::time::timeout(
            Duration::from_secs(1),
            run_ready_agent_steps(&mut manager, "team-parallel", &runner, 2),
        )
        .await
        .expect("parallel workers should reach the barrier together")
        .unwrap();

        assert_eq!(report.launched_member_ids, vec!["api", "ui"]);
        assert_eq!(report.results.len(), 2);
        assert!(report
            .results
            .iter()
            .all(|result| result.status == AgentRunStatus::Completed));
    }
}
