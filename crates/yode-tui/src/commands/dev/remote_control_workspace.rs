use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::commands::artifact_nav::{
    latest_artifact_by_suffix, latest_checkpoint_artifact, latest_runtime_orchestration_artifact,
};
use yode_tools::runtime_tasks::latest_transcript_artifact_path;
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
    #[serde(default = "default_remote_transport_connection_status")]
    pub connection_status: String,
    #[serde(default)]
    pub connection_id: Option<String>,
    #[serde(default)]
    pub connected_at: Option<String>,
    #[serde(default)]
    pub disconnected_at: Option<String>,
    #[serde(default)]
    pub reconnect_attempts: u32,
    #[serde(default)]
    pub last_error: Option<String>,
    #[serde(default)]
    pub last_command: Option<String>,
    #[serde(default)]
    pub queue_gate: Option<String>,
    #[serde(default)]
    pub last_transition_at: Option<String>,
    #[serde(default)]
    pub latest_transport_task_id: Option<String>,
    #[serde(default)]
    pub latest_event: Option<String>,
    #[serde(default)]
    pub latest_event_at: Option<String>,
    #[serde(default)]
    pub latest_event_artifact: Option<String>,
    #[serde(default)]
    pub live_session_status: Option<String>,
    #[serde(default)]
    pub continuity_id: Option<String>,
    #[serde(default)]
    pub active_endpoint_id: Option<String>,
    #[serde(default)]
    pub resume_cursor: Option<u64>,
    pub latest_remote_control: Option<String>,
    pub latest_remote_execution: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RemoteSessionEndpoint {
    pub endpoint_id: String,
    pub device_kind: String,
    pub device_label: String,
    pub status: String,
    pub connection_id: Option<String>,
    pub last_seen_at: String,
    pub last_result_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RemoteLiveSessionPayload {
    pub kind: String,
    pub session_id: String,
    pub continuity_id: String,
    pub created_at: String,
    pub updated_at: String,
    pub session_status: String,
    pub transport_status: String,
    pub active_endpoint_id: Option<String>,
    pub resume_count: u32,
    pub last_resumed_at: Option<String>,
    pub latest_queue_item_id: Option<String>,
    pub latest_result_id: Option<String>,
    pub latest_result_status: Option<String>,
    pub latest_result_summary: Option<String>,
    pub result_cursor: u64,
    pub resume_cursor: u64,
    pub latest_remote_control: Option<String>,
    pub latest_transport_state: Option<String>,
    pub latest_transport_events: Option<String>,
    pub latest_transcript_path: Option<String>,
    pub transcript_sync_status: String,
    pub last_transcript_sync_at: Option<String>,
    pub transcript_sync_artifact: Option<String>,
    pub endpoints: Vec<RemoteSessionEndpoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RemoteQueueResultIngest {
    pub item: String,
    pub status: String,
    pub summary: String,
    #[serde(default)]
    pub endpoint_id: Option<String>,
    #[serde(default)]
    pub device_kind: Option<String>,
    #[serde(default)]
    pub device_label: Option<String>,
    #[serde(default)]
    pub transcript_path: Option<String>,
    #[serde(default)]
    pub result_id: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct RemoteQueueResultOutcome {
    pub item_id: String,
    pub status: String,
    pub execution_path: PathBuf,
    pub session_state_path: PathBuf,
    pub transcript_sync_path: Option<PathBuf>,
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
    let state_path = dir.join(format!(
        "{}-{}-remote-control-session.json",
        stamp, short_session
    ));
    let queue_path = dir.join(format!(
        "{}-{}-remote-command-queue.md",
        stamp, short_session
    ));

    let payload = build_remote_control_payload(project_root, session_id, provider, model, goal);
    std::fs::write(&state_path, serde_json::to_string_pretty(&payload)?)?;
    std::fs::write(
        &summary_path,
        render_remote_control_summary(&payload, &state_path, &queue_path),
    )?;
    std::fs::write(&queue_path, render_remote_control_queue(&payload))?;

    let _ = slug;
    Ok(RemoteControlArtifacts {
        summary_path,
        state_path,
        queue_path,
    })
}

pub(crate) fn latest_remote_control_artifact(project_root: &Path) -> Option<PathBuf> {
    latest_artifact_by_suffix(
        &project_root.join(".yode").join("remote"),
        "remote-control.md",
    )
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

pub(crate) fn latest_remote_transport_events_artifact(project_root: &Path) -> Option<PathBuf> {
    latest_artifact_by_suffix(
        &project_root.join(".yode").join("remote"),
        "remote-transport-events.md",
    )
}

pub(crate) fn latest_remote_live_session_artifact(project_root: &Path) -> Option<PathBuf> {
    latest_artifact_by_suffix(
        &project_root.join(".yode").join("remote"),
        "remote-live-session.md",
    )
}

pub(crate) fn latest_remote_live_session_state_artifact(project_root: &Path) -> Option<PathBuf> {
    latest_artifact_by_suffix(
        &project_root.join(".yode").join("remote"),
        "remote-live-session-state.json",
    )
}

pub(crate) fn latest_remote_session_transcript_sync_artifact(project_root: &Path) -> Option<PathBuf> {
    latest_artifact_by_suffix(
        &project_root.join(".yode").join("remote"),
        "remote-session-transcript-sync.md",
    )
}

pub(crate) fn render_remote_control_doctor(project_root: &Path) -> String {
    let payload = latest_remote_control_state_artifact(project_root)
        .and_then(|path| load_remote_control_payload(&path).ok());
    let transport = latest_remote_transport_state_artifact(project_root)
        .and_then(|path| load_remote_transport_payload(&path).ok());
    let live_session = latest_remote_live_session_state_artifact(project_root)
        .and_then(|path| load_remote_live_session_payload(&path).ok());
    let Some(payload) = payload else {
        return "Remote control doctor\n  Status: no remote control session artifact yet"
            .to_string();
    };

    format!(
        "Remote control doctor\n  Goal: {}\n  Status: {}\n  Queue: {} total / {} completed\n  Capability: {}\n  Execution: {}\n  Checkpoint: {}\n  Orchestration: {}\n  Transport: {}\n  Live session: {}",
        payload.goal,
        remote_queue_status_label(&payload.status),
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
        transport
            .as_ref()
            .map(|transport| {
                format!(
                    "{} / reconnects={} / gate={} / task={} / event={}",
                    transport.connection_status,
                    transport.reconnect_attempts,
                    transport.queue_gate.as_deref().unwrap_or("none"),
                    transport.latest_transport_task_id.as_deref().unwrap_or("none"),
                    transport.latest_event.as_deref().unwrap_or("none"),
                )
            })
            .unwrap_or_else(|| "none".to_string()),
        live_session
            .as_ref()
            .map(|session| {
                format!(
                    "{} / continuity={} / endpoint={} / cursor={} / transcript={}",
                    session.session_status,
                    session.continuity_id,
                    session.active_endpoint_id.as_deref().unwrap_or("none"),
                    session.resume_cursor,
                    session.latest_transcript_path.as_deref().unwrap_or("none"),
                )
            })
            .unwrap_or_else(|| "none".to_string()),
    )
}

pub(crate) fn export_remote_control_bundle(project_root: &Path) -> anyhow::Result<Option<PathBuf>> {
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
        let _ = std::fs::copy(
            &transport_state,
            bundle_dir.join("remote-transport-state.json"),
        );
    }
    if let Some(transport_events) = latest_remote_transport_events_artifact(project_root) {
        let _ = std::fs::copy(
            &transport_events,
            bundle_dir.join("remote-transport-events.md"),
        );
    }
    if let Some(live_session) = latest_remote_live_session_artifact(project_root) {
        let _ = std::fs::copy(&live_session, bundle_dir.join("remote-live-session.md"));
    }
    if let Some(live_session_state) = latest_remote_live_session_state_artifact(project_root) {
        let _ = std::fs::copy(
            &live_session_state,
            bundle_dir.join("remote-live-session-state.json"),
        );
    }
    if let Some(transcript_sync) = latest_remote_session_transcript_sync_artifact(project_root) {
        let _ = std::fs::copy(
            &transcript_sync,
            bundle_dir.join("remote-session-transcript-sync.md"),
        );
    }
    Ok(Some(bundle_dir))
}

pub(crate) fn write_remote_transport_artifacts(
    project_root: &Path,
    session_id: &str,
) -> anyhow::Result<(PathBuf, PathBuf)> {
    let payload = build_remote_transport_payload(project_root, session_id);
    write_remote_transport_payload(project_root, session_id, payload)
}

pub(crate) fn current_remote_transport_payload(
    project_root: &Path,
    session_id: &str,
) -> RemoteTransportPayload {
    build_remote_transport_payload(project_root, session_id)
}

pub(crate) fn mark_remote_transport_reconnecting(
    project_root: &Path,
    session_id: &str,
    command: &str,
    task_id: Option<&str>,
) -> anyhow::Result<(PathBuf, PathBuf)> {
    let now = now_string();
    update_remote_transport_payload(project_root, session_id, |payload| {
        payload.connection_status = "reconnecting".to_string();
        payload.last_command = Some(command.to_string());
        payload.queue_gate = Some(format!("pending: {}", command));
        payload.last_transition_at = Some(now.clone());
        payload.latest_transport_task_id = task_id.map(str::to_string);
    })
}

pub(crate) fn mark_remote_transport_connected(
    project_root: &Path,
    session_id: &str,
    command: &str,
    task_id: Option<&str>,
    reconnect: bool,
) -> anyhow::Result<(PathBuf, PathBuf)> {
    let now = now_string();
    update_remote_transport_payload(project_root, session_id, |payload| {
        payload.connection_status = "connected".to_string();
        payload.connection_id = Some(format!("transport-{}", uuid::Uuid::new_v4()));
        payload.connected_at = Some(now.clone());
        payload.disconnected_at = None;
        if reconnect {
            payload.reconnect_attempts = payload.reconnect_attempts.saturating_add(1);
        }
        payload.last_error = None;
        payload.last_command = Some(command.to_string());
        payload.queue_gate = Some(format!("ready: {}", command));
        payload.last_transition_at = Some(now.clone());
        payload.latest_transport_task_id = task_id.map(str::to_string);
    })
}

pub(crate) fn mark_remote_transport_disconnected(
    project_root: &Path,
    session_id: &str,
    command: &str,
) -> anyhow::Result<(PathBuf, PathBuf)> {
    let now = now_string();
    update_remote_transport_payload(project_root, session_id, |payload| {
        payload.connection_status = "disconnected".to_string();
        payload.connection_id = None;
        payload.disconnected_at = Some(now.clone());
        payload.last_error = None;
        payload.last_command = Some(command.to_string());
        payload.queue_gate = Some("blocked: transport disconnected".to_string());
        payload.last_transition_at = Some(now.clone());
    })
}

pub(crate) fn mark_remote_transport_failed(
    project_root: &Path,
    session_id: &str,
    command: &str,
    error: &str,
    reconnect: bool,
    task_id: Option<&str>,
) -> anyhow::Result<(PathBuf, PathBuf)> {
    let now = now_string();
    update_remote_transport_payload(project_root, session_id, |payload| {
        payload.connection_status = "error".to_string();
        payload.connection_id = None;
        payload.disconnected_at = Some(now.clone());
        if reconnect {
            payload.reconnect_attempts = payload.reconnect_attempts.saturating_add(1);
        }
        payload.last_error = Some(error.to_string());
        payload.last_command = Some(command.to_string());
        payload.queue_gate = Some(format!("blocked: {}", error));
        payload.last_transition_at = Some(now.clone());
        payload.latest_transport_task_id = task_id.map(str::to_string);
    })
}

pub(crate) fn note_remote_transport_dispatch(
    project_root: &Path,
    session_id: &str,
    command: &str,
    allowed: bool,
    reason: &str,
) -> anyhow::Result<(PathBuf, PathBuf)> {
    let now = now_string();
    update_remote_transport_payload(project_root, session_id, |payload| {
        payload.last_command = Some(command.to_string());
        payload.queue_gate = Some(if allowed {
            format!("ready: {}", reason)
        } else {
            format!("blocked: {}", reason)
        });
        payload.last_transition_at = Some(now.clone());
    })
}

pub(crate) fn record_remote_transport_event(
    project_root: &Path,
    session_id: &str,
    kind: &str,
    item_id: Option<&str>,
    task_id: Option<&str>,
    detail: &str,
) -> anyhow::Result<PathBuf> {
    let dir = project_root.join(".yode").join("remote");
    std::fs::create_dir_all(&dir)?;
    let short_session = session_id.chars().take(8).collect::<String>();
    let path = dir.join(format!("{}-remote-transport-events.md", short_session));
    let line = format!(
        "- {} | {}{}{} | {}\n",
        now_string(),
        kind,
        item_id
            .map(|item_id| format!(" | item={}", item_id))
            .unwrap_or_default(),
        task_id
            .map(|task_id| format!(" | task={}", task_id))
            .unwrap_or_default(),
        truncate_preview(detail, 240)
    );
    if path.exists() {
        let mut body = std::fs::read_to_string(&path)?;
        body.push_str(&line);
        std::fs::write(&path, body)?;
    } else {
        let body = format!("# Remote Transport Events\n\n{}", line);
        std::fs::write(&path, body)?;
    }
    let event_label = format!(
        "{}{}{}",
        kind,
        item_id
            .map(|item_id| format!(" item={}", item_id))
            .unwrap_or_default(),
        task_id
            .map(|task_id| format!(" task={}", task_id))
            .unwrap_or_default()
    );
    let now = now_string();
    let path_display = path.display().to_string();
    let _ = update_remote_transport_payload(project_root, session_id, |payload| {
        payload.latest_event = Some(event_label.clone());
        payload.latest_event_at = Some(now.clone());
        payload.latest_event_artifact = Some(path_display.clone());
    });
    Ok(path)
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
    lines.push("  Follow: /remote-control follow latest | /tasks monitor | /tasks follow latest".to_string());
    lines.join("\n")
}

pub(crate) fn render_remote_retry_summary(tasks: &[RuntimeTask]) -> String {
    let mut failed = tasks
        .iter()
        .filter(|task| {
            matches!(
                task.status,
                RuntimeTaskStatus::Failed | RuntimeTaskStatus::Cancelled
            )
        })
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
    lines.push("  Next: /remote-control retry latest | /remote-control follow latest | /remote-control doctor".to_string());
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
    let path = dir.join(format!(
        "{}-{}-remote-task-handoff.md",
        stamp, short_session
    ));
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

pub(crate) fn load_remote_transport_payload(path: &Path) -> anyhow::Result<RemoteTransportPayload> {
    Ok(serde_json::from_str(&std::fs::read_to_string(path)?)?)
}

pub(crate) fn load_remote_live_session_payload(path: &Path) -> anyhow::Result<RemoteLiveSessionPayload> {
    Ok(serde_json::from_str(&std::fs::read_to_string(path)?)?)
}

pub(crate) fn load_remote_queue_result_ingest(path: &Path) -> anyhow::Result<RemoteQueueResultIngest> {
    Ok(serde_json::from_str(&std::fs::read_to_string(path)?)?)
}

pub(crate) fn write_remote_live_session_artifacts(
    project_root: &Path,
    session_id: &str,
) -> anyhow::Result<(PathBuf, PathBuf)> {
    let payload = build_remote_live_session_payload(project_root, session_id);
    write_remote_live_session_payload(project_root, session_id, payload)
}

#[cfg(test)]
pub(crate) fn current_remote_live_session_payload(
    project_root: &Path,
    session_id: &str,
) -> RemoteLiveSessionPayload {
    build_remote_live_session_payload(project_root, session_id)
}

pub(crate) fn sync_remote_live_session_transport(
    project_root: &Path,
    session_id: &str,
) -> anyhow::Result<(PathBuf, PathBuf)> {
    update_remote_live_session_payload(project_root, session_id, |payload| {
        let transport = build_remote_transport_payload(project_root, session_id);
        let endpoint = local_remote_endpoint(
            &transport,
            None,
            None,
            None,
            payload.latest_result_id.as_deref(),
        );
        upsert_remote_session_endpoint(payload, endpoint);
        payload.transport_status = transport.connection_status.clone();
        payload.session_status = session_status_from_transport(&transport, payload);
        payload.active_endpoint_id = Some(default_remote_endpoint_id());
        payload.updated_at = now_string();
        if transport.reconnect_attempts > payload.resume_count {
            payload.resume_count = transport.reconnect_attempts;
            payload.last_resumed_at = Some(now_string());
        }
        payload.resume_cursor = payload.result_cursor;
    })
}

pub(crate) fn mark_remote_live_session_dispatch(
    project_root: &Path,
    session_id: &str,
    item_id: &str,
    task_id: Option<&str>,
) -> anyhow::Result<(PathBuf, PathBuf)> {
    update_remote_live_session_payload(project_root, session_id, |payload| {
        let transport = build_remote_transport_payload(project_root, session_id);
        let endpoint = local_remote_endpoint(
            &transport,
            None,
            None,
            None,
            payload.latest_result_id.as_deref(),
        );
        upsert_remote_session_endpoint(payload, endpoint);
        payload.transport_status = transport.connection_status.clone();
        payload.session_status = if transport.connection_status == "connected" {
            "awaiting_result".to_string()
        } else {
            session_status_from_transport(&transport, payload)
        };
        payload.active_endpoint_id = Some(default_remote_endpoint_id());
        payload.latest_queue_item_id = Some(item_id.to_string());
        payload.updated_at = now_string();
        if task_id.is_some() && payload.last_resumed_at.is_none() {
            payload.last_resumed_at = Some(now_string());
        }
    })
}

pub(crate) fn ingest_remote_queue_result(
    project_root: &Path,
    session_id: &str,
    ingest: &RemoteQueueResultIngest,
) -> anyhow::Result<RemoteQueueResultOutcome> {
    let target = if ingest.item.trim().is_empty() {
        "latest"
    } else {
        ingest.item.trim()
    };
    let (payload, _, index) = queue_item_target(project_root, target)?
        .ok_or_else(|| anyhow::anyhow!("Unknown queue target '{}'.", target))?;
    let item = payload.command_queue[index].clone();
    let status = normalize_ingest_status(&ingest.status)?;
    let summary = if ingest.summary.trim().is_empty() {
        format!("remote result {} for {}", status, item.command)
    } else {
        ingest.summary.trim().to_string()
    };
    let transcript_path = ingest
        .transcript_path
        .clone()
        .or_else(|| item.transcript_path.clone())
        .or_else(|| latest_transcript_artifact_path(project_root));
    let source = ingest
        .source
        .as_deref()
        .unwrap_or("remote_result_ingest");
    let result_id = ingest
        .result_id
        .clone()
        .unwrap_or_else(|| format!("result-{}", uuid::Uuid::new_v4()));

    let _ = note_remote_transport_dispatch(
        project_root,
        session_id,
        &item.command,
        true,
        &format!("ingested {} result {}", status, result_id),
    );
    let event_path = record_remote_transport_event(
        project_root,
        session_id,
        &format!("ingest_{}", status),
        Some(item.id.as_str()),
        item.runtime_task_id.as_deref(),
        &format!("{} via {}: {}", result_id, source, summary),
    )?;
    let (payload, _) = mark_remote_queue_item(project_root, target, status, Some(summary.clone()))?
        .ok_or_else(|| anyhow::anyhow!("Unknown queue target '{}'.", target))?;
    let item = payload.command_queue[index].clone();
    let execution = write_remote_queue_execution_artifact(
        project_root,
        &item,
        "ingested",
        &summary,
        Some(event_path.as_path()),
    )?;
    let _ = bind_remote_queue_item_runtime(
        project_root,
        target,
        item.runtime_task_id.clone(),
        transcript_path.clone(),
        Some(execution.display().to_string()),
    )?;
    let transcript_sync_path = write_remote_session_transcript_sync_artifact(
        project_root,
        session_id,
        &item.id,
        transcript_path.as_deref(),
        &result_id,
        ingest.endpoint_id.as_deref(),
    )?;
    let (_, session_state_path) = update_remote_live_session_payload(project_root, session_id, |live| {
        let transport = build_remote_transport_payload(project_root, session_id);
        live.transport_status = transport.connection_status.clone();
        live.session_status = match status {
            "completed" => "live".to_string(),
            "failed" => "attention".to_string(),
            other => other.to_string(),
        };
        live.active_endpoint_id = Some(
            ingest
                .endpoint_id
                .clone()
                .unwrap_or_else(default_remote_endpoint_id),
        );
        live.latest_queue_item_id = Some(item.id.clone());
        live.latest_result_id = Some(result_id.clone());
        live.latest_result_status = Some(status.to_string());
        live.latest_result_summary = Some(summary.clone());
        live.result_cursor = live.result_cursor.saturating_add(1);
        live.resume_cursor = live.result_cursor;
        live.updated_at = now_string();
        if let Some(path) = transcript_path.clone() {
            live.latest_transcript_path = Some(path);
            live.transcript_sync_status = "synced".to_string();
            live.last_transcript_sync_at = Some(now_string());
        }
        if let Some(path) = transcript_sync_path.as_ref() {
            live.transcript_sync_artifact = Some(path.display().to_string());
        }
        let endpoint = local_remote_endpoint(
            &transport,
            ingest.endpoint_id.as_deref(),
            ingest.device_kind.as_deref(),
            ingest.device_label.as_deref(),
            Some(result_id.as_str()),
        );
        upsert_remote_session_endpoint(live, endpoint);
    })?;
    Ok(RemoteQueueResultOutcome {
        item_id: item.id,
        status: status.to_string(),
        execution_path: execution,
        session_state_path,
        transcript_sync_path,
    })
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
    let now = now_string();
    if matches!(next_status, "running" | "dispatched")
        && item.status != "running"
        && item.status != "dispatched"
    {
        item.attempts = item.attempts.saturating_add(1);
    }
    item.status = next_status.to_string();
    item.last_run_at = Some(now);
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
    phase: &str,
    output_preview: &str,
    transport_event_artifact: Option<&Path>,
) -> anyhow::Result<PathBuf> {
    let dir = project_root.join(".yode").join("remote");
    std::fs::create_dir_all(&dir)?;
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
    let path = dir.join(format!("{}-{}-remote-queue-execution.md", stamp, item.id));
    let body = format!(
        "# Remote Queue Execution\n\n- Item: {}\n- Command: {}\n- Phase: {}\n- Status: {}\n- Attempts: {}\n- Last run: {}\n- Transport events: {}\n\n## Result Preview\n\n```text\n{}\n```\n",
        item.id,
        item.command,
        phase,
        item.status,
        item.attempts,
        item.last_run_at.as_deref().unwrap_or("none"),
        transport_event_artifact
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "none".to_string()),
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
    let latest_checkpoint =
        latest_checkpoint_artifact(project_root).map(|path| path.display().to_string());
    let latest_orchestration =
        latest_runtime_orchestration_artifact(project_root).map(|path| path.display().to_string());
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
    let mut payload = latest_remote_transport_state_artifact(project_root)
        .and_then(|path| load_remote_transport_payload(&path).ok())
        .unwrap_or_else(|| RemoteTransportPayload {
            kind: "remote_transport_state".to_string(),
            session_id: session_id.to_string(),
            remote_dir: remote_dir.display().to_string(),
            created_at: now_string(),
            handshake_status: String::new(),
            handshake_summary: String::new(),
            retry_backoff_secs: vec![1, 2, 5, 10, 30],
            connection_status: default_remote_transport_connection_status(),
            connection_id: None,
            connected_at: None,
            disconnected_at: None,
            reconnect_attempts: 0,
            last_error: None,
            last_command: None,
            queue_gate: None,
            last_transition_at: None,
            latest_transport_task_id: None,
            latest_event: None,
            latest_event_at: None,
            latest_event_artifact: None,
            live_session_status: None,
            continuity_id: None,
            active_endpoint_id: None,
            resume_cursor: None,
            latest_remote_control: None,
            latest_remote_execution: None,
        });
    hydrate_remote_transport_payload(&mut payload, project_root, session_id);
    payload
}

fn build_remote_live_session_payload(
    project_root: &Path,
    session_id: &str,
) -> RemoteLiveSessionPayload {
    let transport = latest_remote_transport_state_artifact(project_root)
        .and_then(|path| load_remote_transport_payload(&path).ok());
    let mut payload = latest_remote_live_session_state_artifact(project_root)
        .and_then(|path| load_remote_live_session_payload(&path).ok())
        .unwrap_or_else(|| RemoteLiveSessionPayload {
            kind: "remote_live_session".to_string(),
            session_id: session_id.to_string(),
            continuity_id: format!("continuity-{}", uuid::Uuid::new_v4()),
            created_at: now_string(),
            updated_at: now_string(),
            session_status: "idle".to_string(),
            transport_status: transport
                .as_ref()
                .map(|payload| payload.connection_status.clone())
                .unwrap_or_else(default_remote_transport_connection_status),
            active_endpoint_id: None,
            resume_count: 0,
            last_resumed_at: None,
            latest_queue_item_id: None,
            latest_result_id: None,
            latest_result_status: None,
            latest_result_summary: None,
            result_cursor: 0,
            resume_cursor: 0,
            latest_remote_control: None,
            latest_transport_state: None,
            latest_transport_events: None,
            latest_transcript_path: None,
            transcript_sync_status: "missing".to_string(),
            last_transcript_sync_at: None,
            transcript_sync_artifact: None,
            endpoints: Vec::new(),
        });
    hydrate_remote_live_session_payload(&mut payload, project_root, session_id, transport.as_ref());
    payload
}

fn default_remote_transport_connection_status() -> String {
    "disconnected".to_string()
}

fn empty_remote_transport_payload(project_root: &Path, session_id: &str) -> RemoteTransportPayload {
    RemoteTransportPayload {
        kind: "remote_transport_state".to_string(),
        session_id: session_id.to_string(),
        remote_dir: project_root.join(".yode").join("remote").display().to_string(),
        created_at: now_string(),
        handshake_status: "missing".to_string(),
        handshake_summary: "remote transport unavailable".to_string(),
        retry_backoff_secs: vec![1, 2, 5, 10, 30],
        connection_status: default_remote_transport_connection_status(),
        connection_id: None,
        connected_at: None,
        disconnected_at: None,
        reconnect_attempts: 0,
        last_error: None,
        last_command: None,
        queue_gate: None,
        last_transition_at: None,
        latest_transport_task_id: None,
        latest_event: None,
        latest_event_at: None,
        latest_event_artifact: None,
        live_session_status: None,
        continuity_id: None,
        active_endpoint_id: None,
        resume_cursor: None,
        latest_remote_control: None,
        latest_remote_execution: None,
    }
}

fn hydrate_remote_transport_payload(
    payload: &mut RemoteTransportPayload,
    project_root: &Path,
    session_id: &str,
) {
    let remote_dir = project_root.join(".yode").join("remote");
    payload.kind = "remote_transport_state".to_string();
    payload.session_id = session_id.to_string();
    payload.remote_dir = remote_dir.display().to_string();
    if payload.created_at.trim().is_empty() {
        payload.created_at = now_string();
    }
    if payload.retry_backoff_secs.is_empty() {
        payload.retry_backoff_secs = vec![1, 2, 5, 10, 30];
    }
    if payload.connection_status.trim().is_empty() {
        payload.connection_status = default_remote_transport_connection_status();
    }
    let remote_dir_ready = remote_dir.exists();
    payload.handshake_status = match (remote_dir_ready, payload.connection_status.as_str()) {
        (false, _) => "missing".to_string(),
        (true, "connected") => "connected".to_string(),
        (true, "reconnecting") => "reconnecting".to_string(),
        (true, "error") => "error".to_string(),
        (true, _) => "ready".to_string(),
    };
    payload.handshake_summary = remote_transport_handshake_summary(remote_dir_ready, payload);
    payload.latest_remote_control =
        latest_remote_control_artifact(project_root).map(|path| path.display().to_string());
    payload.latest_remote_execution =
        latest_artifact_by_suffix(&remote_dir, "remote-execution-state.json")
            .map(|path| path.display().to_string());
    payload.latest_event_artifact = latest_remote_transport_events_artifact(project_root)
        .map(|path| path.display().to_string())
        .or_else(|| payload.latest_event_artifact.clone());
    if let Some(session) = latest_remote_live_session_state_artifact(project_root)
        .and_then(|path| load_remote_live_session_payload(&path).ok())
    {
        payload.live_session_status = Some(session.session_status);
        payload.continuity_id = Some(session.continuity_id);
        payload.active_endpoint_id = session.active_endpoint_id;
        payload.resume_cursor = Some(session.resume_cursor);
    }
}

fn hydrate_remote_live_session_payload(
    payload: &mut RemoteLiveSessionPayload,
    project_root: &Path,
    session_id: &str,
    transport: Option<&RemoteTransportPayload>,
) {
    payload.kind = "remote_live_session".to_string();
    payload.session_id = session_id.to_string();
    if payload.continuity_id.trim().is_empty() {
        payload.continuity_id = format!("continuity-{}", uuid::Uuid::new_v4());
    }
    if payload.created_at.trim().is_empty() {
        payload.created_at = now_string();
    }
    if payload.updated_at.trim().is_empty() {
        payload.updated_at = now_string();
    }
    payload.transport_status = transport
        .map(|transport| transport.connection_status.clone())
        .unwrap_or_else(default_remote_transport_connection_status);
    payload.latest_remote_control =
        latest_remote_control_artifact(project_root).map(|path| path.display().to_string());
    payload.latest_transport_state =
        latest_remote_transport_state_artifact(project_root).map(|path| path.display().to_string());
    payload.latest_transport_events = latest_remote_transport_events_artifact(project_root)
        .map(|path| path.display().to_string());
    payload.latest_transcript_path = latest_transcript_artifact_path(project_root)
        .or_else(|| payload.latest_transcript_path.clone());
    payload.transcript_sync_artifact = latest_remote_session_transcript_sync_artifact(project_root)
        .map(|path| path.display().to_string())
        .or_else(|| payload.transcript_sync_artifact.clone());
    if payload.transcript_sync_status.trim().is_empty() {
        payload.transcript_sync_status = if payload.latest_transcript_path.is_some() {
            "pending".to_string()
        } else {
            "missing".to_string()
        };
    }
    if payload.active_endpoint_id.is_none() && !payload.endpoints.is_empty() {
        payload.active_endpoint_id = payload
            .endpoints
            .first()
            .map(|endpoint| endpoint.endpoint_id.clone());
    }
    let empty_transport;
    let transport = match transport {
        Some(transport) => transport,
        None => {
            empty_transport = empty_remote_transport_payload(project_root, session_id);
            &empty_transport
        }
    };
    payload.session_status = session_status_from_transport(transport, payload);
}

fn remote_transport_handshake_summary(
    remote_dir_ready: bool,
    payload: &RemoteTransportPayload,
) -> String {
    if !remote_dir_ready {
        return "remote artifact directory missing; run remote-control plan or doctor remote"
            .to_string();
    }
    match payload.connection_status.as_str() {
        "connected" => format!(
            "transport connected{}; queue gate={}",
            payload
                .connection_id
                .as_ref()
                .map(|id| format!(" ({})", id))
                .unwrap_or_default(),
            payload.queue_gate.as_deref().unwrap_or("ready"),
        ),
        "reconnecting" => format!(
            "transport reconnecting; attempts={} / task={}",
            payload.reconnect_attempts.saturating_add(1),
            payload
                .latest_transport_task_id
                .as_deref()
                .unwrap_or("none"),
        ),
        "error" => format!(
            "transport error: {}",
            payload.last_error.as_deref().unwrap_or("unknown failure")
        ),
        _ => format!(
            "remote artifact directory available; transport ready but disconnected{}",
            payload
                .queue_gate
                .as_ref()
                .map(|gate| format!(" / {}", gate))
                .unwrap_or_default(),
        ),
    }
}

fn session_status_from_transport(
    transport: &RemoteTransportPayload,
    payload: &RemoteLiveSessionPayload,
) -> String {
    if payload.latest_result_status.as_deref() == Some("failed") {
        "attention".to_string()
    } else {
        match transport.connection_status.as_str() {
            "connected" => "live".to_string(),
            "reconnecting" => "resuming".to_string(),
            "error" => "attention".to_string(),
            _ => "idle".to_string(),
        }
    }
}

fn update_remote_transport_payload<F>(
    project_root: &Path,
    session_id: &str,
    update: F,
) -> anyhow::Result<(PathBuf, PathBuf)>
where
    F: FnOnce(&mut RemoteTransportPayload),
{
    let mut payload = build_remote_transport_payload(project_root, session_id);
    update(&mut payload);
    write_remote_transport_payload(project_root, session_id, payload)
}

fn update_remote_live_session_payload<F>(
    project_root: &Path,
    session_id: &str,
    update: F,
) -> anyhow::Result<(PathBuf, PathBuf)>
where
    F: FnOnce(&mut RemoteLiveSessionPayload),
{
    let mut payload = build_remote_live_session_payload(project_root, session_id);
    update(&mut payload);
    write_remote_live_session_payload(project_root, session_id, payload)
}

fn write_remote_live_session_payload(
    project_root: &Path,
    session_id: &str,
    mut payload: RemoteLiveSessionPayload,
) -> anyhow::Result<(PathBuf, PathBuf)> {
    let dir = project_root.join(".yode").join("remote");
    std::fs::create_dir_all(&dir)?;
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
    let short_session = session_id.chars().take(8).collect::<String>();
    let summary_path = dir.join(format!("{}-{}-remote-live-session.md", stamp, short_session));
    let state_path = dir.join(format!(
        "{}-{}-remote-live-session-state.json",
        stamp, short_session
    ));
    let transport = latest_remote_transport_state_artifact(project_root)
        .and_then(|path| load_remote_transport_payload(&path).ok());
    hydrate_remote_live_session_payload(&mut payload, project_root, session_id, transport.as_ref());
    std::fs::write(&state_path, serde_json::to_string_pretty(&payload)?)?;
    std::fs::write(
        &summary_path,
        render_remote_live_session_summary(&payload, &state_path),
    )?;
    Ok((summary_path, state_path))
}

fn write_remote_transport_payload(
    project_root: &Path,
    session_id: &str,
    mut payload: RemoteTransportPayload,
) -> anyhow::Result<(PathBuf, PathBuf)> {
    let dir = project_root.join(".yode").join("remote");
    std::fs::create_dir_all(&dir)?;
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
    let short_session = session_id.chars().take(8).collect::<String>();
    let summary_path = dir.join(format!("{}-{}-remote-transport.md", stamp, short_session));
    let state_path = dir.join(format!(
        "{}-{}-remote-transport-state.json",
        stamp, short_session
    ));
    hydrate_remote_transport_payload(&mut payload, project_root, session_id);
    std::fs::write(&state_path, serde_json::to_string_pretty(&payload)?)?;
    std::fs::write(
        &summary_path,
        render_remote_transport_summary(&payload, &state_path),
    )?;
    Ok((summary_path, state_path))
}

fn render_remote_transport_summary(payload: &RemoteTransportPayload, state_path: &Path) -> String {
    format!(
        "# Remote Transport\n\n- Session: {}\n- Remote dir: {}\n- Connection: {}\n- Connection id: {}\n- Connected at: {}\n- Disconnected at: {}\n- Handshake: {}\n- Summary: {}\n- Reconnect attempts: {}\n- Retry backoff: {}\n- Last command: {}\n- Queue gate: {}\n- Last error: {}\n- Latest transport task: {}\n- Latest event: {}\n- Latest event at: {}\n- Latest event artifact: {}\n- Latest remote control: {}\n- Latest remote execution: {}\n- State artifact: {}\n",
        payload.session_id,
        payload.remote_dir,
        payload.connection_status,
        payload.connection_id.as_deref().unwrap_or("none"),
        payload.connected_at.as_deref().unwrap_or("none"),
        payload.disconnected_at.as_deref().unwrap_or("none"),
        payload.handshake_status,
        payload.handshake_summary,
        payload.reconnect_attempts,
        payload
            .retry_backoff_secs
            .iter()
            .map(u64::to_string)
            .collect::<Vec<_>>()
            .join(", "),
        payload.last_command.as_deref().unwrap_or("none"),
        payload.queue_gate.as_deref().unwrap_or("none"),
        payload.last_error.as_deref().unwrap_or("none"),
        payload.latest_transport_task_id.as_deref().unwrap_or("none"),
        payload.latest_event.as_deref().unwrap_or("none"),
        payload.latest_event_at.as_deref().unwrap_or("none"),
        payload.latest_event_artifact.as_deref().unwrap_or("none"),
        payload.latest_remote_control.as_deref().unwrap_or("none"),
        payload.latest_remote_execution.as_deref().unwrap_or("none"),
        state_path.display(),
    )
}

fn render_remote_live_session_summary(
    payload: &RemoteLiveSessionPayload,
    state_path: &Path,
) -> String {
    let endpoints = if payload.endpoints.is_empty() {
        "- none".to_string()
    } else {
        payload
            .endpoints
            .iter()
            .map(|endpoint| {
                format!(
                    "- {} [{}:{}] conn={} last_seen={} result={}",
                    endpoint.endpoint_id,
                    endpoint.device_kind,
                    endpoint.status,
                    endpoint.connection_id.as_deref().unwrap_or("none"),
                    endpoint.last_seen_at,
                    endpoint.last_result_id.as_deref().unwrap_or("none"),
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    format!(
        "# Remote Live Session\n\n- Session: {}\n- Continuity id: {}\n- Status: {}\n- Transport: {}\n- Active endpoint: {}\n- Resume count: {}\n- Last resumed at: {}\n- Latest queue item: {}\n- Latest result id: {}\n- Latest result status: {}\n- Latest result summary: {}\n- Result cursor: {}\n- Resume cursor: {}\n- Latest transcript: {}\n- Transcript sync: {}\n- Transcript sync artifact: {}\n- Latest remote control: {}\n- Latest transport state: {}\n- Latest transport events: {}\n- State artifact: {}\n\n## Endpoints\n\n{}\n",
        payload.session_id,
        payload.continuity_id,
        payload.session_status,
        payload.transport_status,
        payload.active_endpoint_id.as_deref().unwrap_or("none"),
        payload.resume_count,
        payload.last_resumed_at.as_deref().unwrap_or("none"),
        payload.latest_queue_item_id.as_deref().unwrap_or("none"),
        payload.latest_result_id.as_deref().unwrap_or("none"),
        payload.latest_result_status.as_deref().unwrap_or("none"),
        payload.latest_result_summary.as_deref().unwrap_or("none"),
        payload.result_cursor,
        payload.resume_cursor,
        payload.latest_transcript_path.as_deref().unwrap_or("none"),
        payload.transcript_sync_status,
        payload.transcript_sync_artifact.as_deref().unwrap_or("none"),
        payload.latest_remote_control.as_deref().unwrap_or("none"),
        payload.latest_transport_state.as_deref().unwrap_or("none"),
        payload.latest_transport_events.as_deref().unwrap_or("none"),
        state_path.display(),
        endpoints,
    )
}

fn write_remote_session_transcript_sync_artifact(
    project_root: &Path,
    session_id: &str,
    item_id: &str,
    transcript_path: Option<&str>,
    result_id: &str,
    endpoint_id: Option<&str>,
) -> anyhow::Result<Option<PathBuf>> {
    let Some(transcript_path) = transcript_path else {
        return Ok(None);
    };
    let dir = project_root.join(".yode").join("remote");
    std::fs::create_dir_all(&dir)?;
    let short_session = session_id.chars().take(8).collect::<String>();
    let path = dir.join(format!(
        "{}-remote-session-transcript-sync.md",
        short_session
    ));
    let body = format!(
        "# Remote Session Transcript Sync\n\n- Session: {}\n- Item: {}\n- Result id: {}\n- Endpoint: {}\n- Transcript: {}\n- Synced at: {}\n",
        session_id,
        item_id,
        result_id,
        endpoint_id.unwrap_or("none"),
        transcript_path,
        now_string(),
    );
    std::fs::write(&path, body)?;
    Ok(Some(path))
}

fn default_remote_endpoint_id() -> String {
    std::env::var("HOSTNAME")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .map(|value| format!("local-{}", sanitize_remote_label(&value)))
        .unwrap_or_else(|| "local-terminal".to_string())
}

fn local_remote_endpoint(
    transport: &RemoteTransportPayload,
    endpoint_id: Option<&str>,
    device_kind: Option<&str>,
    device_label: Option<&str>,
    last_result_id: Option<&str>,
) -> RemoteSessionEndpoint {
    let fallback_label = std::env::var("HOSTNAME")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "terminal".to_string());
    RemoteSessionEndpoint {
        endpoint_id: endpoint_id
            .map(str::to_string)
            .unwrap_or_else(default_remote_endpoint_id),
        device_kind: device_kind.unwrap_or("local_cli").to_string(),
        device_label: device_label.unwrap_or(&fallback_label).to_string(),
        status: transport.connection_status.clone(),
        connection_id: transport.connection_id.clone(),
        last_seen_at: now_string(),
        last_result_id: last_result_id.map(str::to_string),
    }
}

fn upsert_remote_session_endpoint(
    payload: &mut RemoteLiveSessionPayload,
    endpoint: RemoteSessionEndpoint,
) {
    if let Some(existing) = payload
        .endpoints
        .iter_mut()
        .find(|current| current.endpoint_id == endpoint.endpoint_id)
    {
        *existing = endpoint;
    } else {
        payload.endpoints.push(endpoint);
    }
    payload
        .endpoints
        .sort_by(|left, right| left.endpoint_id.cmp(&right.endpoint_id));
}

fn normalize_ingest_status(raw: &str) -> anyhow::Result<&'static str> {
    match raw.trim() {
        "completed" | "complete" | "success" => Ok("completed"),
        "failed" | "fail" | "error" => Ok("failed"),
        other => Err(anyhow::anyhow!(
            "Unsupported ingest status '{}'. Expected completed|failed.",
            other
        )),
    }
}

fn sanitize_remote_label(raw: &str) -> String {
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
    chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
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
        format!("- Status: {}", remote_queue_status_label(&payload.status)),
        format!("- Queue size: {}", payload.command_queue.len()),
        format!(
            "- Queue completed: {}",
            payload
                .command_queue
                .iter()
                .filter(|item| item.status == "completed")
                .count()
        ),
        format!(
            "- Queue acknowledged: {}",
            payload
                .command_queue
                .iter()
                .filter(|item| item.status == "acked")
                .count()
        ),
        format!("- State artifact: {}", state_path.display()),
        format!("- Queue artifact: {}", queue_path.display()),
        String::new(),
        format!(
            "- Latest remote capability: {}",
            payload
                .latest_remote_capability
                .as_deref()
                .unwrap_or("none")
        ),
        format!(
            "- Latest remote execution: {}",
            payload.latest_remote_execution.as_deref().unwrap_or("none")
        ),
        format!(
            "- Latest checkpoint: {}",
            payload.latest_checkpoint.as_deref().unwrap_or("none")
        ),
        format!(
            "- Latest orchestration: {}",
            payload.latest_orchestration.as_deref().unwrap_or("none")
        ),
        String::new(),
        "Use `/remote-control queue`, `/remote-control doctor`, or `/remote-control bundle`."
            .to_string(),
    ]
    .join("\n")
}

fn render_remote_control_queue(payload: &RemoteControlPayload) -> String {
    let mut lines = vec![
        "# Remote Command Queue".to_string(),
        String::new(),
        format!("- Goal: {}", payload.goal),
        format!("- Status: {}", remote_queue_status_label(&payload.status)),
        String::new(),
        "Commands:".to_string(),
    ];
    for (index, item) in payload.command_queue.iter().enumerate() {
        lines.push(format!(
            "- {}. {} [{}] attempts={}{}{}{}",
            index + 1,
            item.command,
            remote_queue_status_label(&item.status),
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
    std::fs::write(
        &artifacts.state_path,
        serde_json::to_string_pretty(payload)?,
    )?;
    std::fs::write(
        &artifacts.summary_path,
        render_remote_control_summary(payload, &artifacts.state_path, &artifacts.queue_path),
    )?;
    std::fs::write(&artifacts.queue_path, render_remote_control_queue(payload))?;
    Ok(())
}

fn summarize_queue_status(items: &[RemoteQueueItem]) -> String {
    if items
        .iter()
        .any(|item| matches!(item.status.as_str(), "running" | "dispatched"))
    {
        "running".to_string()
    } else if items.iter().all(|item| item.status == "acked") {
        "acked".to_string()
    } else if items.iter().any(|item| item.status == "failed") {
        "attention".to_string()
    } else if items.iter().all(|item| item.status == "completed") {
        "completed".to_string()
    } else {
        "queued".to_string()
    }
}

pub(crate) fn remote_queue_status_label(status: &str) -> &str {
    match status {
        "planned" | "queued" => "queued",
        "dispatched" => "dispatched",
        "running" => "running",
        "completed" => "completed",
        "failed" => "failed",
        "acked" => "acknowledged",
        "attention" => "needs-attention",
        other => other,
    }
}

fn remote_slug(raw: &str) -> String {
    let slug = raw
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();
    slug.trim_matches('-').to_string()
}

fn truncate_preview(text: &str, max_chars: usize) -> String {
    let squashed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if squashed.chars().count() <= max_chars {
        squashed
    } else {
        format!(
            "{}...",
            squashed.chars().take(max_chars).collect::<String>()
        )
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
        bind_remote_queue_item_runtime, current_remote_live_session_payload,
        current_remote_transport_payload, export_remote_control_bundle,
        ingest_remote_queue_result,
        latest_remote_command_queue_artifact, latest_remote_control_artifact,
        latest_remote_control_state_artifact, latest_remote_live_session_state_artifact,
        latest_remote_session_transcript_sync_artifact, latest_remote_task_handoff_artifact,
        latest_remote_transport_events_artifact, latest_remote_transport_state_artifact,
        load_remote_live_session_payload, load_remote_transport_payload, load_remote_control_payload,
        mark_remote_queue_item, queue_item_target, record_remote_transport_event,
        remote_queue_status_label,
        mark_remote_transport_connected, mark_remote_transport_disconnected,
        mark_remote_transport_failed, mark_remote_transport_reconnecting,
        note_remote_transport_dispatch, render_remote_control_doctor, render_remote_retry_summary,
        render_remote_task_inventory, sync_remote_live_session_transport,
        write_remote_control_artifacts, write_remote_live_session_artifacts,
        write_remote_task_handoff_artifact, write_remote_transport_artifacts,
    };

    #[test]
    fn writes_remote_control_artifacts_and_bundle() {
        let dir =
            std::env::temp_dir().join(format!("yode-remote-control-{}", uuid::Uuid::new_v4()));
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

    #[test]
    fn remote_transport_lifecycle_persists_connection_state() {
        let dir =
            std::env::temp_dir().join(format!("yode-remote-transport-{}", uuid::Uuid::new_v4()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".yode").join("remote")).unwrap();

        write_remote_transport_artifacts(&dir, "session-1234").unwrap();
        let initial = current_remote_transport_payload(&dir, "session-1234");
        assert_eq!(initial.connection_status, "disconnected");

        mark_remote_transport_connected(
            &dir,
            "session-1234",
            "/remote-control transport connect",
            Some("task-1"),
            false,
        )
        .unwrap();
        let connected_path = latest_remote_transport_state_artifact(&dir).unwrap();
        let connected = load_remote_transport_payload(&connected_path).unwrap();
        assert_eq!(connected.connection_status, "connected");
        assert_eq!(
            connected.latest_transport_task_id.as_deref(),
            Some("task-1")
        );

        mark_remote_transport_reconnecting(
            &dir,
            "session-1234",
            "/remote-control transport reconnect",
            Some("task-2"),
        )
        .unwrap();
        mark_remote_transport_failed(
            &dir,
            "session-1234",
            "/remote-control transport reconnect",
            "probe failed",
            true,
            Some("task-2"),
        )
        .unwrap();
        let failed =
            load_remote_transport_payload(&latest_remote_transport_state_artifact(&dir).unwrap())
                .unwrap();
        assert_eq!(failed.connection_status, "error");
        assert_eq!(failed.reconnect_attempts, 1);
        assert_eq!(failed.last_error.as_deref(), Some("probe failed"));

        mark_remote_transport_disconnected(
            &dir,
            "session-1234",
            "/remote-control transport disconnect",
        )
        .unwrap();
        note_remote_transport_dispatch(
            &dir,
            "session-1234",
            "/doctor remote",
            false,
            "transport disconnected",
        )
        .unwrap();
        let disconnected =
            load_remote_transport_payload(&latest_remote_transport_state_artifact(&dir).unwrap())
                .unwrap();
        assert_eq!(disconnected.connection_status, "disconnected");
        assert_eq!(
            disconnected.queue_gate.as_deref(),
            Some("blocked: transport disconnected")
        );

        let event_path = record_remote_transport_event(
            &dir,
            "session-1234",
            "disconnect",
            Some("q-1"),
            Some("task-2"),
            "operator disconnected transport after remote failure",
        )
        .unwrap();
        assert_eq!(
            latest_remote_transport_events_artifact(&dir).as_deref(),
            Some(event_path.as_path())
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn queue_attempts_increment_only_on_dispatch_like_states() {
        let dir =
            std::env::temp_dir().join(format!("yode-remote-queue-{}", uuid::Uuid::new_v4()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".yode").join("remote")).unwrap();
        write_remote_control_artifacts(&dir, "session-1234", "anthropic", "claude", "goal")
            .unwrap();

        mark_remote_queue_item(&dir, "latest", "dispatched", Some("sent".to_string()))
            .unwrap();
        mark_remote_queue_item(&dir, "latest", "completed", Some("done".to_string()))
            .unwrap();
        mark_remote_queue_item(&dir, "latest", "acked", Some("ok".to_string()))
            .unwrap();

        let (payload, _, index) = queue_item_target(&dir, "latest").unwrap().unwrap();
        let item = &payload.command_queue[index];
        assert_eq!(item.attempts, 1);
        assert_eq!(item.status, "acked");
        assert_eq!(item.last_result_preview.as_deref(), Some("ok"));

        let state = load_remote_control_payload(&latest_remote_control_state_artifact(&dir).unwrap())
            .unwrap();
        assert_eq!(state.command_queue[0].attempts, 1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn remote_queue_status_labels_are_operator_friendly() {
        assert_eq!(remote_queue_status_label("planned"), "queued");
        assert_eq!(remote_queue_status_label("queued"), "queued");
        assert_eq!(remote_queue_status_label("acked"), "acknowledged");
        assert_eq!(remote_queue_status_label("attention"), "needs-attention");
    }

    #[test]
    fn remote_live_session_sync_tracks_endpoint_and_continuity() {
        let dir = std::env::temp_dir().join(format!("yode-live-session-{}", uuid::Uuid::new_v4()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".yode").join("remote")).unwrap();

        write_remote_control_artifacts(&dir, "session-1234", "anthropic", "claude", "goal")
            .unwrap();
        mark_remote_transport_connected(
            &dir,
            "session-1234",
            "/remote-control transport connect",
            Some("task-1"),
            false,
        )
        .unwrap();
        write_remote_live_session_artifacts(&dir, "session-1234").unwrap();
        sync_remote_live_session_transport(&dir, "session-1234").unwrap();

        let payload = current_remote_live_session_payload(&dir, "session-1234");
        assert_eq!(payload.session_status, "live");
        assert_eq!(payload.transport_status, "connected");
        assert!(!payload.continuity_id.is_empty());
        assert!(!payload.endpoints.is_empty());
        assert!(payload.active_endpoint_id.is_some());
        assert!(latest_remote_live_session_state_artifact(&dir).is_some());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ingest_remote_queue_result_updates_live_session_and_transcript_sync() {
        let dir =
            std::env::temp_dir().join(format!("yode-live-ingest-{}", uuid::Uuid::new_v4()));
        let transcripts = dir.join(".yode").join("transcripts");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".yode").join("remote")).unwrap();
        std::fs::create_dir_all(&transcripts).unwrap();
        let transcript = transcripts.join("abc12345-compact-20260101-100000.md");
        std::fs::write(&transcript, "# Transcript\n").unwrap();

        write_remote_control_artifacts(&dir, "session-1234", "anthropic", "claude", "goal")
            .unwrap();
        mark_remote_transport_connected(
            &dir,
            "session-1234",
            "/remote-control transport connect",
            Some("task-1"),
            false,
        )
        .unwrap();
        sync_remote_live_session_transport(&dir, "session-1234").unwrap();
        mark_remote_queue_item(&dir, "latest", "dispatched", Some("sent".to_string()))
            .unwrap();
        bind_remote_queue_item_runtime(
            &dir,
            "latest",
            Some("task-1".to_string()),
            None,
            None,
        )
        .unwrap();

        let outcome = ingest_remote_queue_result(
            &dir,
            "session-1234",
            &super::RemoteQueueResultIngest {
                item: "latest".to_string(),
                status: "completed".to_string(),
                summary: "remote worker finished".to_string(),
                endpoint_id: Some("browser-1".to_string()),
                device_kind: Some("browser".to_string()),
                device_label: Some("Chrome".to_string()),
                transcript_path: None,
                result_id: Some("result-1".to_string()),
                source: Some("remote_worker".to_string()),
            },
        )
        .unwrap();

        assert_eq!(outcome.status, "completed");
        assert!(outcome.execution_path.exists());
        assert!(outcome.session_state_path.exists());
        assert!(outcome.transcript_sync_path.as_ref().is_some_and(|path| path.exists()));
        assert!(latest_remote_session_transcript_sync_artifact(&dir).is_some());

        let session =
            load_remote_live_session_payload(&latest_remote_live_session_state_artifact(&dir).unwrap())
                .unwrap();
        assert_eq!(session.latest_result_status.as_deref(), Some("completed"));
        assert_eq!(session.latest_result_id.as_deref(), Some("result-1"));
        assert_eq!(session.result_cursor, 1);
        assert_eq!(session.active_endpoint_id.as_deref(), Some("browser-1"));
        assert_eq!(session.transcript_sync_status, "synced");

        let queue = load_remote_control_payload(&latest_remote_control_state_artifact(&dir).unwrap())
            .unwrap();
        assert_eq!(queue.command_queue[0].status, "completed");
        assert_eq!(queue.command_queue[0].runtime_task_id.as_deref(), Some("task-1"));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
