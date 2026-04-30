use std::path::{Path, PathBuf};

use anyhow::Result;

use super::paths::{
    latest_artifact_by_suffix, latest_remote_control_state_artifact,
    latest_remote_live_session_state_artifact, latest_remote_transport_events_artifact,
    latest_remote_transport_state_artifact, latest_transcript_artifact, load_json, now_string,
    remote_dir, short_session, timestamp_slug,
};
use super::queue::default_queue_items;
use super::render::{
    render_remote_control_queue, render_remote_control_summary, render_remote_live_session_summary,
    render_remote_transport_summary,
};
use super::status::truncate_preview;
use super::types::{
    RemoteControlArtifactSet, RemoteControlPayload, RemoteLiveSessionArtifactSet,
    RemoteLiveSessionPayload, RemoteQueueItem, RemoteTransportArtifactSet, RemoteTransportPayload,
};

pub(super) fn load_or_create_remote_control_payload(
    project_root: &Path,
    session_id: &str,
    provider: &str,
    model: &str,
    goal: &str,
) -> RemoteControlPayload {
    latest_remote_control_state_artifact(project_root)
        .and_then(|path| load_json::<RemoteControlPayload>(&path).ok())
        .unwrap_or_else(|| RemoteControlPayload {
            kind: "remote_control_session".to_string(),
            goal: goal.to_string(),
            session_id: session_id.to_string(),
            provider: provider.to_string(),
            model: model.to_string(),
            working_dir: project_root.display().to_string(),
            remote_dir: remote_dir(project_root).display().to_string(),
            created_at: now_string(),
            status: "queued".to_string(),
            command_queue: default_queue_items(),
            latest_remote_capability: None,
            latest_remote_execution: None,
            latest_checkpoint: None,
            latest_orchestration: None,
        })
}

pub(super) fn load_or_create_remote_transport_payload(
    project_root: &Path,
    session_id: &str,
) -> RemoteTransportPayload {
    latest_remote_transport_state_artifact(project_root)
        .and_then(|path| load_json::<RemoteTransportPayload>(&path).ok())
        .unwrap_or_else(|| RemoteTransportPayload {
            kind: "remote_transport_state".to_string(),
            session_id: session_id.to_string(),
            remote_dir: remote_dir(project_root).display().to_string(),
            created_at: now_string(),
            handshake_status: String::new(),
            handshake_summary: String::new(),
            retry_backoff_secs: vec![1, 2, 5, 10, 30],
            connection_status: "disconnected".to_string(),
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
        })
}

pub(super) fn load_or_create_remote_live_session_payload(
    project_root: &Path,
    session_id: &str,
) -> RemoteLiveSessionPayload {
    latest_remote_live_session_state_artifact(project_root)
        .and_then(|path| load_json::<RemoteLiveSessionPayload>(&path).ok())
        .unwrap_or_else(|| RemoteLiveSessionPayload {
            kind: "remote_live_session".to_string(),
            session_id: session_id.to_string(),
            continuity_id: format!("continuity-{}", uuid::Uuid::new_v4()),
            created_at: now_string(),
            updated_at: now_string(),
            session_status: "idle".to_string(),
            transport_status: "disconnected".to_string(),
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
            latest_transcript_path: latest_transcript_artifact(project_root),
            transcript_sync_status: "pending".to_string(),
            last_transcript_sync_at: None,
            transcript_sync_artifact: None,
            endpoints: Vec::new(),
        })
}

pub(super) fn write_remote_control_payload(
    project_root: &Path,
    payload: &mut RemoteControlPayload,
) -> Result<RemoteControlArtifactSet> {
    let dir = remote_dir(project_root);
    std::fs::create_dir_all(&dir)?;
    payload.remote_dir = dir.display().to_string();
    payload.latest_remote_capability =
        latest_artifact_by_suffix(&dir, "remote-workflow-capability.json")
            .map(|path| path.display().to_string());
    payload.latest_remote_execution = latest_artifact_by_suffix(&dir, "remote-queue-execution.md")
        .map(|path| path.display().to_string());
    payload.latest_checkpoint =
        latest_artifact_by_suffix(&project_root.join(".yode").join("checkpoints"), ".md")
            .map(|path| path.display().to_string());
    payload.latest_orchestration = latest_artifact_by_suffix(
        &project_root.join(".yode").join("status"),
        "runtime-orchestration.md",
    )
    .map(|path| path.display().to_string());
    let stamp = timestamp_slug();
    let short_session = short_session(&payload.session_id);
    let summary_path = dir.join(format!("{}-{}-remote-control.md", stamp, short_session));
    let state_path = dir.join(format!(
        "{}-{}-remote-control-session.json",
        stamp, short_session
    ));
    let queue_path = dir.join(format!(
        "{}-{}-remote-command-queue.md",
        stamp, short_session
    ));
    std::fs::write(&state_path, serde_json::to_string_pretty(payload)?)?;
    std::fs::write(
        &summary_path,
        render_remote_control_summary(payload, &state_path, &queue_path),
    )?;
    std::fs::write(&queue_path, render_remote_control_queue(payload))?;
    Ok(RemoteControlArtifactSet {
        summary_path,
        state_path,
        queue_path,
    })
}

pub(super) fn write_remote_transport_payload(
    project_root: &Path,
    session_id: &str,
    payload: &mut RemoteTransportPayload,
) -> Result<RemoteTransportArtifactSet> {
    let dir = remote_dir(project_root);
    std::fs::create_dir_all(&dir)?;
    payload.session_id = session_id.to_string();
    payload.remote_dir = dir.display().to_string();
    payload.handshake_status = match payload.connection_status.as_str() {
        "connected" => "connected".to_string(),
        "reconnecting" => "reconnecting".to_string(),
        "error" => "error".to_string(),
        _ => "ready".to_string(),
    };
    payload.handshake_summary = remote_transport_handshake_summary(payload);
    payload.latest_remote_control =
        latest_artifact_by_suffix(&dir, "remote-control.md").map(|path| path.display().to_string());
    payload.latest_remote_execution = latest_artifact_by_suffix(&dir, "remote-queue-execution.md")
        .map(|path| path.display().to_string());
    let stamp = timestamp_slug();
    let short_session = short_session(session_id);
    let summary_path = dir.join(format!("{}-{}-remote-transport.md", stamp, short_session));
    let state_path = dir.join(format!(
        "{}-{}-remote-transport-state.json",
        stamp, short_session
    ));
    std::fs::write(&state_path, serde_json::to_string_pretty(payload)?)?;
    std::fs::write(
        &summary_path,
        render_remote_transport_summary(payload, &state_path),
    )?;
    Ok(RemoteTransportArtifactSet {
        summary_path,
        state_path,
    })
}

pub(super) fn write_remote_live_session_payload(
    project_root: &Path,
    session_id: &str,
    payload: &mut RemoteLiveSessionPayload,
) -> Result<RemoteLiveSessionArtifactSet> {
    let dir = remote_dir(project_root);
    std::fs::create_dir_all(&dir)?;
    payload.session_id = session_id.to_string();
    payload.updated_at = now_string();
    payload.latest_remote_control =
        latest_artifact_by_suffix(&dir, "remote-control.md").map(|path| path.display().to_string());
    payload.latest_transport_state = latest_artifact_by_suffix(&dir, "remote-transport-state.json")
        .map(|path| path.display().to_string());
    payload.latest_transport_events = latest_remote_transport_events_artifact(project_root)
        .map(|path| path.display().to_string());
    if payload.latest_transcript_path.is_none() {
        payload.latest_transcript_path = latest_transcript_artifact(project_root);
    }
    if payload.transcript_sync_status.is_empty() {
        payload.transcript_sync_status = if payload.latest_transcript_path.is_some() {
            "pending".to_string()
        } else {
            "missing".to_string()
        };
    }
    let stamp = timestamp_slug();
    let short_session = short_session(session_id);
    let summary_path = dir.join(format!(
        "{}-{}-remote-live-session.md",
        stamp, short_session
    ));
    let state_path = dir.join(format!(
        "{}-{}-remote-live-session-state.json",
        stamp, short_session
    ));
    std::fs::write(&state_path, serde_json::to_string_pretty(payload)?)?;
    std::fs::write(
        &summary_path,
        render_remote_live_session_summary(payload, &state_path),
    )?;
    Ok(RemoteLiveSessionArtifactSet {
        summary_path,
        state_path,
    })
}

pub(super) fn write_remote_queue_execution_artifact(
    project_root: &Path,
    item: &RemoteQueueItem,
    phase: &str,
    output_preview: &str,
    transport_event_artifact: Option<&Path>,
) -> Result<PathBuf> {
    let dir = remote_dir(project_root);
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!(
        "{}-{}-remote-queue-execution.md",
        timestamp_slug(),
        item.id
    ));
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

pub(super) fn write_remote_session_transcript_sync_artifact(
    project_root: &Path,
    session_id: &str,
    item_id: &str,
    transcript_path: Option<&str>,
    result_id: &str,
    endpoint_id: Option<&str>,
) -> Result<Option<PathBuf>> {
    let Some(transcript_path) = transcript_path else {
        return Ok(None);
    };
    let dir = remote_dir(project_root);
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!(
        "{}-remote-session-transcript-sync.md",
        short_session(session_id)
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

pub(super) fn record_remote_transport_event(
    project_root: &Path,
    session_id: &str,
    kind: &str,
    item_id: Option<&str>,
    task_id: Option<&str>,
    detail: &str,
) -> Result<PathBuf> {
    let dir = remote_dir(project_root);
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!(
        "{}-remote-transport-events.md",
        short_session(session_id)
    ));
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
        std::fs::write(&path, format!("# Remote Transport Events\n\n{}", line))?;
    }
    Ok(path)
}

pub(super) fn probe_remote_transport(project_root: &Path) -> Result<()> {
    let dir = remote_dir(project_root);
    std::fs::create_dir_all(&dir)?;
    let probe = dir.join(".transport-probe");
    std::fs::write(&probe, b"ok")?;
    std::fs::remove_file(&probe)?;
    Ok(())
}

fn remote_transport_handshake_summary(payload: &RemoteTransportPayload) -> String {
    match payload.connection_status.as_str() {
        "connected" => format!(
            "connected via {}; queue gate {}",
            payload.connection_id.as_deref().unwrap_or("unknown"),
            payload.queue_gate.as_deref().unwrap_or("ready")
        ),
        "reconnecting" => format!(
            "reconnecting; attempts {}; backoff {:?}",
            payload.reconnect_attempts, payload.retry_backoff_secs
        ),
        "error" => format!(
            "error: {}",
            payload.last_error.as_deref().unwrap_or("unknown")
        ),
        _ => "ready for connect".to_string(),
    }
}
