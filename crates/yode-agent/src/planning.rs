use std::collections::{BTreeMap, BTreeSet};

use anyhow::{anyhow, Result};

use crate::{
    sanitize_id, AgentPlan, AgentPlanBatch, AgentPlanMode, AgentPlanProgress, AgentPlanStep,
    AgentTeamMemberState, AgentTeamState,
};

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

pub fn evaluate_agent_plan(state: &AgentTeamState) -> Option<AgentPlanProgress> {
    let plan = state.plan.as_ref()?;
    let member_statuses = state
        .members
        .iter()
        .map(|member| (member.member_id.as_str(), member.status.as_str()))
        .collect::<BTreeMap<_, _>>();
    let completed_steps = plan
        .steps
        .iter()
        .filter(|step| step_status(step, &member_statuses) == "completed")
        .map(|step| step.step_id.as_str())
        .collect::<BTreeSet<_>>();

    let mut progress = AgentPlanProgress::default();
    for step in &plan.steps {
        match step_status(step, &member_statuses) {
            "completed" => progress.completed_step_ids.push(step.step_id.clone()),
            "failed" | "cancelled" => progress.failed_step_ids.push(step.step_id.clone()),
            "running" => progress.running_step_ids.push(step.step_id.clone()),
            _ if step
                .depends_on
                .iter()
                .all(|dependency| completed_steps.contains(dependency.as_str())) =>
            {
                progress.ready_step_ids.push(step.step_id.clone());
            }
            _ => progress.blocked_step_ids.push(step.step_id.clone()),
        }
    }
    Some(progress)
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

pub fn sync_agent_team_plan_statuses(state: &mut AgentTeamState) {
    let Some(plan) = state.plan.as_mut() else {
        return;
    };
    let member_statuses = state
        .members
        .iter()
        .map(|member| (member.member_id.as_str(), member.status.as_str()))
        .collect::<BTreeMap<_, _>>();
    for step in &mut plan.steps {
        step.status = step_status(step, &member_statuses).to_string();
    }
}

fn step_status<'a>(step: &'a AgentPlanStep, member_statuses: &BTreeMap<&str, &'a str>) -> &'a str {
    step.member_id
        .as_deref()
        .and_then(|member_id| member_statuses.get(member_id).copied())
        .unwrap_or(step.status.as_str())
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
