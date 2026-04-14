use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::commands::artifact_nav::{
    latest_artifact_by_suffix, latest_checkpoint_artifact, latest_runtime_orchestration_artifact,
};
use yode_tools::{RuntimeTask, RuntimeTaskStatus};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RemoteQueueItem {
    pub id: String,
    pub command: String,
    pub status: String,
    pub attempts: u32,
    pub runtime_task_id: Option<String>,
    pub transcript_path: Option<String>,
    pub last_run_at: Option<String>,
    pub last_result_preview: Option<String>,
    pub execution_artifact: Option<String>,
    pub acknowledged_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RemoteControlPayload {
    pub kind: String,
    pub goal: String,
    pub session_id: String,
    pub provider: String,
    pub model: String,
    pub working_dir: String,
    pub remote_dir: String,
    pub created_at: String,
    pub status: String,
    pub command_queue: Vec<RemoteQueueItem>,
    pub latest_remote_capability: Option<String>,
    pub latest_remote_execution: Option<String>,
    pub latest_checkpoint: Option<String>,
    pub latest_orchestration: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RemoteTransportPayload {
    pub kind: String,
    pub session_id: String,
    pub remote_dir: String,
    pub created_at: String,
    pub handshake_status: String,
    pub handshake_summary: String,
    pub retry_backoff_secs: Vec<u64>,
    pub latest_remote_control: Option<String>,
    pub latest_remote_execution: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct RemoteControlArtifacts {
    pub summary_path: PathBuf,
    pub state_path: PathBuf,
    pub queue_path: PathBuf,
}

pub(crate) fn write_remote_control_artifacts(
    project_root: &Path,
    session_id: &str,
    provider: &str,
    model: &str,
    goal: &str,
) -> anyhow::Result<RemoteControlArtifacts> {
    let dir = project_root.join(".yode").join("remote");
    std::fs::create_dir_all(&dir)?;
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
    let short_session = session_id.chars().take(8).collect::<String>();
    let slug = remote_slug(goal);
    let summary_path = dir.join(format!("{}-{}-remote-control.md", stamp, short_session));
    let state_path = dir.join(format!("{}-{}-remote-control-session.json", stamp, short_session));
    let queue_path = dir.join(format!("{}-{}-remote-command-queue.md", stamp, short_session));

    let payload = build_remote_control_payload(project_root, session_id, provider, model, goal);
    std::fs::write(&state_path, serde_json::to_string_pretty(&payload)?)?;
    std::fs::write(&summary_path, render_remote_control_summary(&payload, &state_path, &queue_path))?;
    std::fs::write(&queue_path, render_remote_control_queue(&payload))?;

    let _ = slug;
    Ok(RemoteControlArtifacts {
        summary_path,
        state_path,
        queue_path,
    })
}

pub(crate) fn latest_remote_control_artifact(project_root: &Path) -> Option<PathBuf> {
    latest_artifact_by_suffix(&project_root.join(".yode").join("remote"), "remote-control.md")
}

pub(crate) fn latest_remote_control_state_artifact(project_root: &Path) -> Option<PathBuf> {
    latest_artifact_by_suffix(
        &project_root.join(".yode").join("remote"),
        "remote-control-session.json",
    )
}

pub(crate) fn latest_remote_command_queue_artifact(project_root: &Path) -> Option<PathBuf> {
    latest_artifact_by_suffix(
        &project_root.join(".yode").join("remote"),
        "remote-command-queue.md",
    )
}

pub(crate) fn latest_remote_task_handoff_artifact(project_root: &Path) -> Option<PathBuf> {
    latest_artifact_by_suffix(
        &project_root.join(".yode").join("remote"),
        "remote-task-handoff.md",
    )
}

pub(crate) fn latest_remote_queue_execution_artifact(project_root: &Path) -> Option<PathBuf> {
    latest_artifact_by_suffix(
        &project_root.join(".yode").join("remote"),
        "remote-queue-execution.md",
    )
}

pub(crate) fn latest_remote_transport_artifact(project_root: &Path) -> Option<PathBuf> {
    latest_artifact_by_suffix(
        &project_root.join(".yode").join("remote"),
        "remote-transport.md",
    )
}

pub(crate) fn latest_remote_transport_state_artifact(project_root: &Path) -> Option<PathBuf> {
    latest_artifact_by_suffix(
        &project_root.join(".yode").join("remote"),
        "remote-transport-state.json",
    )
}

pub(crate) fn render_remote_control_doctor(project_root: &Path) -> String {
    let payload = latest_remote_control_state_artifact(project_root)
        .and_then(|path| load_remote_control_payload(&path).ok());
    let Some(payload) = payload else {
        return "Remote control doctor\n  Status: no remote control session artifact yet".to_string();
    };

    format!(
        "Remote control doctor\n  Goal: {}\n  Status: {}\n  Queue: {} total / {} completed\n  Capability: {}\n  Execution: {}\n  Checkpoint: {}\n  Orchestration: {}\n  Transport: {}",
        payload.goal,
        payload.status,
        payload.command_queue.len(),
        payload
            .command_queue
            .iter()
            .filter(|item| item.status == "completed")
            .count(),
        payload.latest_remote_capability.as_deref().unwrap_or("none"),
        payload.latest_remote_execution.as_deref().unwrap_or("none"),
        payload.latest_checkpoint.as_deref().unwrap_or("none"),
        payload.latest_orchestration.as_deref().unwrap_or("none"),
        latest_remote_transport_state_artifact(project_root)
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "none".to_string()),
    )
}

pub(crate) fn export_remote_control_bundle(
    project_root: &Path,
) -> anyhow::Result<Option<PathBuf>> {
    let Some(summary) = latest_remote_control_artifact(project_root) else {
        return Ok(None);
    };
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let bundle_dir = cwd.join(format!(
        "remote-control-bundle-{}",
        chrono::Local::now().format("%Y%m%d-%H%M%S")
    ));
    std::fs::create_dir_all(&bundle_dir)?;
    let state = latest_remote_control_state_artifact(project_root);
    let queue = latest_remote_command_queue_artifact(project_root);
    let handoff = latest_remote_task_handoff_artifact(project_root);
    let _ = std::fs::copy(&summary, bundle_dir.join("remote-control.md"));
    if let Some(state) = state {
        let _ = std::fs::copy(&state, bundle_dir.join("remote-control-session.json"));
    }
    if let Some(queue) = queue {
        let _ = std::fs::copy(&queue, bundle_dir.join("remote-command-queue.md"));
    }
    if let Some(handoff) = handoff {
        let _ = std::fs::copy(&handoff, bundle_dir.join("remote-task-handoff.md"));
    }
    if let Some(execution) = latest_remote_queue_execution_artifact(project_root) {
        let _ = std::fs::copy(&execution, bundle_dir.join("remote-queue-execution.md"));
    }
    if let Some(transport) = latest_remote_transport_artifact(project_root) {
        let _ = std::fs::copy(&transport, bundle_dir.join("remote-transport.md"));
    }
    if let Some(transport_state) = latest_remote_transport_state_artifact(project_root) {
        let _ = std::fs::copy(&transport_state, bundle_dir.join("remote-transport-state.json"));
    }
    Ok(Some(bundle_dir))
}

pub(crate) fn write_remote_transport_artifacts(
    project_root: &Path,
    session_id: &str,
) -> anyhow::Result<(PathBuf, PathBuf)> {
    let dir = project_root.join(".yode").join("remote");
    std::fs::create_dir_all(&dir)?;
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
    let short_session = session_id.chars().take(8).collect::<String>();
    let summary_path = dir.join(format!("{}-{}-remote-transport.md", stamp, short_session));
    let state_path = dir.join(format!("{}-{}-remote-transport-state.json", stamp, short_session));
    let payload = build_remote_transport_payload(project_root, session_id);
    std::fs::write(&state_path, serde_json::to_string_pretty(&payload)?)?;
    let body = format!(
        "# Remote Transport\n\n- Session: {}\n- Remote dir: {}\n- Handshake: {}\n- Summary: {}\n- Retry backoff: {}\n- Latest remote control: {}\n- Latest remote execution: {}\n- State artifact: {}\n",
        payload.session_id,
        payload.remote_dir,
        payload.handshake_status,
        payload.handshake_summary,
        payload
            .retry_backoff_secs
            .iter()
            .map(u64::to_string)
            .collect::<Vec<_>>()
            .join(", "),
        payload.latest_remote_control.as_deref().unwrap_or("none"),
        payload.latest_remote_execution.as_deref().unwrap_or("none"),
        state_path.display(),
    );
    std::fs::write(&summary_path, body)?;
    Ok((summary_path, state_path))
}

pub(crate) fn render_remote_task_inventory(tasks: &[RuntimeTask]) -> String {
    if tasks.is_empty() {
        return "Remote task continuation inventory\n  Tasks: none".to_string();
    }
    let mut tasks = tasks.to_vec();
    tasks.sort_by(|a, b| b.last_progress_at.cmp(&a.last_progress_at));
    let mut lines = vec![
        "Remote task continuation inventory".to_string(),
        format!("  Tasks: {}", tasks.len()),
    ];
    for task in tasks.iter().take(12) {
        lines.push(format!(
            "  - {} [{}:{}] {} / transcript={} / output={}",
            task.id,
            task.kind,
            task_status_label(&task.status),
            task.description,
            task.transcript_path.as_deref().unwrap_or("none"),
            task.output_path
        ));
    }
    lines.join("\n")
}

pub(crate) fn render_remote_retry_summary(tasks: &[RuntimeTask]) -> String {
    let mut failed = tasks
        .iter()
        .filter(|task| matches!(task.status, RuntimeTaskStatus::Failed | RuntimeTaskStatus::Cancelled))
        .collect::<Vec<_>>();
    failed.sort_by(|a, b| b.completed_at.cmp(&a.completed_at));
    if failed.is_empty() {
        return "Remote task retry summary\n  Failed tasks: none".to_string();
    }
    let mut lines = vec![
        "Remote task retry summary".to_string(),
        format!("  Failed tasks: {}", failed.len()),
    ];
    for task in failed.iter().take(8) {
        lines.push(format!(
            "  - {} [{}] attempt {}{} / {}",
            task.id,
            task_status_label(&task.status),
            task.attempt,
            task.retry_of
                .as_ref()
                .map(|retry_of| format!(" (retry of {})", retry_of))
                .unwrap_or_default(),
            task.error.as_deref().unwrap_or("no error detail")
        ));
    }
    lines.join("\n")
}

pub(crate) fn write_remote_task_handoff_artifact(
    project_root: &Path,
    session_id: &str,
    task: &RuntimeTask,
) -> anyhow::Result<PathBuf> {
    let dir = project_root.join(".yode").join("remote");
    std::fs::create_dir_all(&dir)?;
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
    let short_session = session_id.chars().take(8).collect::<String>();
    let path = dir.join(format!("{}-{}-remote-task-handoff.md", stamp, short_session));
    let body = format!(
        "# Remote Task Handoff\n\n- Task: {}\n- Kind: {}\n- Status: {}\n- Description: {}\n- Attempt: {}\n- Retry of: {}\n- Output: {}\n- Transcript: {}\n- Latest remote control: {}\n- Latest checkpoint: {}\n- Latest orchestration: {}\n\n## Summary\n\n- Carry this task through `/remote-control follow {}` or `/tasks follow {}`.\n- Re-check remote capability and execution state before retrying.\n",
        task.id,
        task.kind,
        task_status_label(&task.status),
        task.description,
        task.attempt,
        task.retry_of.as_deref().unwrap_or("none"),
        task.output_path,
        task.transcript_path.as_deref().unwrap_or("none"),
        latest_remote_control_artifact(project_root)
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "none".to_string()),
        latest_checkpoint_artifact(project_root)
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "none".to_string()),
        latest_runtime_orchestration_artifact(project_root)
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "none".to_string()),
        task.id,
        task.id,
    );
    std::fs::write(&path, body)?;
    Ok(path)
}

pub(crate) fn load_remote_control_payload(path: &Path) -> anyhow::Result<RemoteControlPayload> {
    Ok(serde_json::from_str(&std::fs::read_to_string(path)?)?)
}

pub(crate) fn latest_remote_control_payload(
    project_root: &Path,
) -> anyhow::Result<Option<(RemoteControlPayload, RemoteControlArtifacts)>> {
    let Some(summary_path) = latest_remote_control_artifact(project_root) else {
        return Ok(None);
    };
    let Some(state_path) = latest_remote_control_state_artifact(project_root) else {
        return Ok(None);
    };
    let Some(queue_path) = latest_remote_command_queue_artifact(project_root) else {
        return Ok(None);
    };
    Ok(Some((
        load_remote_control_payload(&state_path)?,
        RemoteControlArtifacts {
            summary_path,
            state_path,
            queue_path,
        },
    )))
}

pub(crate) fn queue_item_target(
    project_root: &Path,
    target: &str,
) -> anyhow::Result<Option<(RemoteControlPayload, RemoteControlArtifacts, usize)>> {
    let Some((payload, artifacts)) = latest_remote_control_payload(project_root)? else {
        return Ok(None);
    };
    if payload.command_queue.is_empty() {
        return Ok(None);
    }
    let trimmed = target.trim();
    let index = if trimmed.is_empty() || trimmed == "latest" {
        Some(0usize)
    } else if let Ok(index) = trimmed.parse::<usize>() {
        index.checked_sub(1)
    } else {
        payload
            .command_queue
            .iter()
            .position(|item| item.id == trimmed || item.command == trimmed)
    };
    Ok(index
        .filter(|index| *index < payload.command_queue.len())
        .map(|index| (payload, artifacts, index)))
}

pub(crate) fn mark_remote_queue_item(
    project_root: &Path,
    target: &str,
    next_status: &str,
    preview: Option<String>,
) -> anyhow::Result<Option<(RemoteControlPayload, RemoteControlArtifacts)>> {
    let Some((mut payload, artifacts, index)) = queue_item_target(project_root, target)? else {
        return Ok(None);
    };
    let item = &mut payload.command_queue[index];
    item.status = next_status.to_string();
    item.attempts = item.attempts.saturating_add(u32::from(next_status != "acked"));
    item.last_run_at = Some(chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string());
    if next_status == "acked" {
        item.acknowledged_at = item.last_run_at.clone();
    }
    if let Some(preview) = preview {
        item.last_result_preview = Some(truncate_preview(&preview, 180));
    }
    payload.status = summarize_queue_status(&payload.command_queue);
    rewrite_remote_control_artifacts(&payload, &artifacts)?;
    Ok(Some((payload, artifacts)))
}

pub(crate) fn bind_remote_queue_item_runtime(
    project_root: &Path,
    target: &str,
    runtime_task_id: Option<String>,
    transcript_path: Option<String>,
    execution_artifact: Option<String>,
) -> anyhow::Result<Option<(RemoteControlPayload, RemoteControlArtifacts)>> {
    let Some((mut payload, artifacts, index)) = queue_item_target(project_root, target)? else {
        return Ok(None);
    };
    let item = &mut payload.command_queue[index];
    if runtime_task_id.is_some() {
        item.runtime_task_id = runtime_task_id;
    }
    if transcript_path.is_some() {
        item.transcript_path = transcript_path;
    }
    if execution_artifact.is_some() {
        item.execution_artifact = execution_artifact;
    }
    rewrite_remote_control_artifacts(&payload, &artifacts)?;
    Ok(Some((payload, artifacts)))
}

pub(crate) fn write_remote_queue_execution_artifact(
    project_root: &Path,
    item: &RemoteQueueItem,
    output_preview: &str,
) -> anyhow::Result<PathBuf> {
    let dir = project_root.join(".yode").join("remote");
    std::fs::create_dir_all(&dir)?;
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
    let path = dir.join(format!("{}-{}-remote-queue-execution.md", stamp, item.id));
    let body = format!(
        "# Remote Queue Execution\n\n- Item: {}\n- Command: {}\n- Status: {}\n- Attempts: {}\n- Last run: {}\n\n## Result Preview\n\n```text\n{}\n```\n",
        item.id,
        item.command,
        item.status,
        item.attempts,
        item.last_run_at.as_deref().unwrap_or("none"),
        output_preview,
    );
    std::fs::write(&path, body)?;
    Ok(path)
}

fn build_remote_control_payload(
    project_root: &Path,
    session_id: &str,
    provider: &str,
    model: &str,
    goal: &str,
) -> RemoteControlPayload {
    let remote_dir = project_root.join(".yode").join("remote");
    let latest_remote_capability =
        latest_artifact_by_suffix(&remote_dir, "remote-workflow-capability.json")
            .map(|path| path.display().to_string());
    let latest_remote_execution =
        latest_artifact_by_suffix(&remote_dir, "remote-execution-state.json")
            .map(|path| path.display().to_string());
    let latest_checkpoint = latest_checkpoint_artifact(project_root)
        .map(|path| path.display().to_string());
    let latest_orchestration = latest_runtime_orchestration_artifact(project_root)
        .map(|path| path.display().to_string());
    let command_queue = [
        "/doctor remote",
        "/doctor remote-review",
        "/inspect artifact latest-remote-capability",
        "/inspect artifact latest-remote-execution",
        "/inspect artifact latest-checkpoint",
        "/inspect artifact latest-orchestration",
    ]
    .into_iter()
    .enumerate()
    .map(|(index, command)| RemoteQueueItem {
        id: format!("q-{}", index + 1),
        command: command.to_string(),
        status: "queued".to_string(),
        attempts: 0,
        runtime_task_id: None,
        transcript_path: None,
        last_run_at: None,
        last_result_preview: None,
        execution_artifact: None,
        acknowledged_at: None,
    })
    .collect::<Vec<_>>();

    RemoteControlPayload {
        kind: "remote_control_session".to_string(),
        goal: if goal.trim().is_empty() {
            "continue the current task from a remote control surface".to_string()
        } else {
            goal.trim().to_string()
        },
        session_id: session_id.to_string(),
        provider: provider.to_string(),
        model: model.to_string(),
        working_dir: project_root.display().to_string(),
        remote_dir: remote_dir.display().to_string(),
        created_at: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        status: "planned".to_string(),
        command_queue,
        latest_remote_capability,
        latest_remote_execution,
        latest_checkpoint,
        latest_orchestration,
    }
}

fn build_remote_transport_payload(project_root: &Path, session_id: &str) -> RemoteTransportPayload {
    let remote_dir = project_root.join(".yode").join("remote");
    let handshake_status = if remote_dir.exists() { "ready" } else { "missing" }.to_string();
    let handshake_summary = if remote_dir.exists() {
        "remote artifact directory available; transport handshake can begin".to_string()
    } else {
        "remote artifact directory missing; run remote-control plan or doctor remote".to_string()
    };
    RemoteTransportPayload {
        kind: "remote_transport_state".to_string(),
        session_id: session_id.to_string(),
        remote_dir: remote_dir.display().to_string(),
        created_at: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        handshake_status,
        handshake_summary,
        retry_backoff_secs: vec![1, 2, 5, 10, 30],
        latest_remote_control: latest_remote_control_artifact(project_root)
            .map(|path| path.display().to_string()),
        latest_remote_execution: latest_artifact_by_suffix(&remote_dir, "remote-execution-state.json")
            .map(|path| path.display().to_string()),
    }
}

fn render_remote_control_summary(
    payload: &RemoteControlPayload,
    state_path: &Path,
    queue_path: &Path,
) -> String {
    [
        "# Remote Control Session".to_string(),
        String::new(),
        format!("- Goal: {}", payload.goal),
        format!("- Session: {}", payload.session_id),
        format!("- Provider: {}", payload.provider),
        format!("- Model: {}", payload.model),
        format!("- Working dir: {}", payload.working_dir),
        format!("- Remote dir: {}", payload.remote_dir),
        format!("- Status: {}", payload.status),
        format!("- Queue size: {}", payload.command_queue.len()),
        format!(
            "- Queue completed: {}",
            payload
                .command_queue
                .iter()
                .filter(|item| item.status == "completed")
                .count()
        ),
        format!("- State artifact: {}", state_path.display()),
        format!("- Queue artifact: {}", queue_path.display()),
        String::new(),
        format!("- Latest remote capability: {}", payload.latest_remote_capability.as_deref().unwrap_or("none")),
        format!("- Latest remote execution: {}", payload.latest_remote_execution.as_deref().unwrap_or("none")),
        format!("- Latest checkpoint: {}", payload.latest_checkpoint.as_deref().unwrap_or("none")),
        format!("- Latest orchestration: {}", payload.latest_orchestration.as_deref().unwrap_or("none")),
        String::new(),
        "Use `/remote-control queue`, `/remote-control doctor`, or `/remote-control bundle`.".to_string(),
    ]
    .join("\n")
}

fn render_remote_control_queue(payload: &RemoteControlPayload) -> String {
    let mut lines = vec![
        "# Remote Command Queue".to_string(),
        String::new(),
        format!("- Goal: {}", payload.goal),
        format!("- Status: {}", payload.status),
        String::new(),
        "Commands:".to_string(),
    ];
    for (index, item) in payload.command_queue.iter().enumerate() {
        lines.push(format!(
            "- {}. {} [{}] attempts={}{}{}{}",
            index + 1,
            item.command,
            item.status,
            item.attempts,
            item.runtime_task_id
                .as_ref()
                .map(|task_id| format!(" / task={}", task_id))
                .unwrap_or_default(),
            item.last_result_preview
                .as_ref()
                .map(|preview| format!(" / {}", preview))
                .unwrap_or_default(),
            item.execution_artifact
                .as_ref()
                .map(|path| format!(" / execution={}", path))
                .unwrap_or_default()
        ));
    }
    lines.join("\n")
}

fn rewrite_remote_control_artifacts(
    payload: &RemoteControlPayload,
    artifacts: &RemoteControlArtifacts,
) -> anyhow::Result<()> {
    std::fs::write(&artifacts.state_path, serde_json::to_string_pretty(payload)?)?;
    std::fs::write(
        &artifacts.summary_path,
        render_remote_control_summary(payload, &artifacts.state_path, &artifacts.queue_path),
    )?;
    std::fs::write(&artifacts.queue_path, render_remote_control_queue(payload))?;
    Ok(())
}

fn summarize_queue_status(items: &[RemoteQueueItem]) -> String {
    if items.iter().any(|item| item.status == "running") {
        "running".to_string()
    } else if items.iter().all(|item| item.status == "acked") {
        "acked".to_string()
    } else if items.iter().any(|item| item.status == "failed") {
        "attention".to_string()
    } else if items.iter().all(|item| item.status == "completed") {
        "completed".to_string()
    } else {
        "planned".to_string()
    }
}

fn remote_slug(raw: &str) -> String {
    let slug = raw
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch.to_ascii_lowercase() } else { '-' })
        .collect::<String>();
    slug.trim_matches('-').to_string()
}

fn truncate_preview(text: &str, max_chars: usize) -> String {
    let squashed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if squashed.chars().count() <= max_chars {
        squashed
    } else {
        format!("{}...", squashed.chars().take(max_chars).collect::<String>())
    }
}

fn task_status_label(status: &RuntimeTaskStatus) -> &'static str {
    match status {
        RuntimeTaskStatus::Pending => "pending",
        RuntimeTaskStatus::Running => "running",
        RuntimeTaskStatus::Completed => "completed",
        RuntimeTaskStatus::Failed => "failed",
        RuntimeTaskStatus::Cancelled => "cancelled",
    }
}

#[cfg(test)]
mod tests {
    use super::{
        export_remote_control_bundle, latest_remote_command_queue_artifact,
        latest_remote_control_artifact, latest_remote_control_state_artifact,
        latest_remote_task_handoff_artifact, render_remote_control_doctor,
        render_remote_retry_summary, render_remote_task_inventory,
        write_remote_control_artifacts, write_remote_task_handoff_artifact,
    };

    #[test]
    fn writes_remote_control_artifacts_and_bundle() {
        let dir = std::env::temp_dir().join(format!("yode-remote-control-{}", uuid::Uuid::new_v4()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".yode").join("remote")).unwrap();
        let artifacts = write_remote_control_artifacts(
            &dir,
            "session-1234",
            "anthropic",
            "claude",
            "remote continue",
        )
        .unwrap();
        assert!(artifacts.summary_path.exists());
        assert!(latest_remote_control_artifact(&dir).is_some());
        assert!(latest_remote_control_state_artifact(&dir).is_some());
        assert!(latest_remote_command_queue_artifact(&dir).is_some());
        assert!(render_remote_control_doctor(&dir).contains("Remote control doctor"));
        let bundle = export_remote_control_bundle(&dir).unwrap();
        assert!(bundle.is_some());
        let task = yode_tools::RuntimeTask {
            id: "task-1".to_string(),
            kind: "agent".to_string(),
            source_tool: "agent".to_string(),
            description: "continue remote review".to_string(),
            status: yode_tools::RuntimeTaskStatus::Failed,
            attempt: 2,
            retry_of: Some("task-0".to_string()),
            output_path: "/tmp/task.log".to_string(),
            transcript_path: Some("/tmp/task.md".to_string()),
            created_at: "2026-01-01 00:00:00".to_string(),
            started_at: None,
            completed_at: Some("2026-01-01 00:00:02".to_string()),
            last_progress: None,
            last_progress_at: None,
            progress_history: Vec::new(),
            error: Some("boom".to_string()),
        };
        assert!(render_remote_task_inventory(std::slice::from_ref(&task)).contains("task-1"));
        assert!(render_remote_retry_summary(std::slice::from_ref(&task)).contains("Failed tasks"));
        let handoff = write_remote_task_handoff_artifact(&dir, "session-1234", &task).unwrap();
        assert!(latest_remote_task_handoff_artifact(&dir).is_some());
        assert!(handoff.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
