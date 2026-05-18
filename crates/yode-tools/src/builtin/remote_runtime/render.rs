use std::path::Path;

use super::status::queue_status_label;
use super::types::{RemoteControlPayload, RemoteLiveSessionPayload, RemoteTransportPayload};

pub(super) fn render_remote_control_summary(
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
        format!("- Status: {}", queue_status_label(&payload.status)),
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
    ]
    .join("\n")
}

pub(super) fn render_remote_control_queue(payload: &RemoteControlPayload) -> String {
    let mut lines = vec![
        "# Remote Command Queue".to_string(),
        String::new(),
        format!("- Goal: {}", payload.goal),
        format!("- Status: {}", queue_status_label(&payload.status)),
        String::new(),
        "Commands:".to_string(),
    ];
    for (index, item) in payload.command_queue.iter().enumerate() {
        lines.push(format!(
            "- {}. {} [{}] attempts={}{}{}{}",
            index + 1,
            item.command,
            queue_status_label(&item.status),
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
                .unwrap_or_default(),
        ));
    }
    lines.join("\n")
}

pub(super) fn render_remote_transport_summary(
    payload: &RemoteTransportPayload,
    state_path: &Path,
) -> String {
    format!(
        "# Remote Transport\n\n- Session: {}\n- Remote dir: {}\n- Connection: {}\n- Connection id: {}\n- Connected at: {}\n- Disconnected at: {}\n- Handshake: {}\n- Summary: {}\n- Reconnect attempts: {}\n- Retry backoff: {}\n- Last command: {}\n- Queue gate: {}\n- Last error: {}\n- Latest transport task: {}\n- Latest event: {}\n- Latest event at: {}\n- Latest event artifact: {}\n- Resume cursor: {}\n- Latest remote control: {}\n- Latest remote execution: {}\n- State artifact: {}\n",
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
        payload.resume_cursor.unwrap_or(0),
        payload.latest_remote_control.as_deref().unwrap_or("none"),
        payload.latest_remote_execution.as_deref().unwrap_or("none"),
        state_path.display(),
    )
}

pub(super) fn render_remote_live_session_summary(
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
