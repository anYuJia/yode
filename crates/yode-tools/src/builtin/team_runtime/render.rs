use std::path::Path;
use std::sync::Arc;

use anyhow::Result;

use super::{evaluate_agent_plan, AgentTeamSnapshot, AgentTeamState};

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
    ];
    if let Some(plan) = &state.plan {
        body.push("Plan:".to_string());
        body.push(format!(
            "- {:?}: {} steps across {} batches",
            plan.mode, plan.step_count, plan.parallel_batch_count
        ));
        for batch in &plan.batches {
            body.push(format!(
                "- {}: {}",
                batch.batch_id,
                batch.step_ids.join(", ")
            ));
        }
        if let Some(progress) = evaluate_agent_plan(state) {
            body.push(format!(
                "- Ready: {}",
                format_step_list(&progress.ready_step_ids)
            ));
            body.push(format!(
                "- Blocked: {}",
                format_step_list(&progress.blocked_step_ids)
            ));
        }
        body.push(String::new());
    }
    body.push("Members:".to_string());
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

pub(super) fn render_team_summary(state: &AgentTeamState) -> String {
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
    ];
    if let Some(plan) = &state.plan {
        lines.push("Plan:".to_string());
        lines.push(format!(
            "- Mode: {:?} / {} steps / {} batches",
            plan.mode, plan.step_count, plan.parallel_batch_count
        ));
        for batch in &plan.batches {
            lines.push(format!(
                "- {}: {}",
                batch.batch_id,
                batch.step_ids.join(", ")
            ));
        }
        if let Some(progress) = evaluate_agent_plan(state) {
            lines.push(format!(
                "- Ready: {}",
                format_step_list(&progress.ready_step_ids)
            ));
            lines.push(format!(
                "- Blocked: {}",
                format_step_list(&progress.blocked_step_ids)
            ));
        }
        lines.push(String::new());
    }
    lines.push("Members:".to_string());
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

pub(super) fn render_team_bundle(state: &AgentTeamState, messages_path: &Path) -> String {
    let messages = match std::fs::read_to_string(messages_path) {
        Ok(messages) => messages,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            "# Agent Team Messages\n\n- none\n".to_string()
        }
        Err(err) => {
            tracing::warn!(
                path = %messages_path.display(),
                error = %err,
                "failed to read agent team messages while rendering bundle"
            );
            "# Agent Team Messages\n\n- none\n".to_string()
        }
    };
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

fn format_step_list(step_ids: &[String]) -> String {
    if step_ids.is_empty() {
        "none".to_string()
    } else {
        step_ids.join(", ")
    }
}
