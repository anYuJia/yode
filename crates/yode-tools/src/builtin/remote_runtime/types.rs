use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct RemoteQueueItem {
    pub(super) id: String,
    pub(super) command: String,
    pub(super) status: String,
    pub(super) attempts: u32,
    pub(super) runtime_task_id: Option<String>,
    pub(super) transcript_path: Option<String>,
    pub(super) last_run_at: Option<String>,
    pub(super) last_result_preview: Option<String>,
    pub(super) execution_artifact: Option<String>,
    pub(super) acknowledged_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct RemoteControlPayload {
    pub(super) kind: String,
    pub(super) goal: String,
    pub(super) session_id: String,
    pub(super) provider: String,
    pub(super) model: String,
    pub(super) working_dir: String,
    pub(super) remote_dir: String,
    pub(super) created_at: String,
    pub(super) status: String,
    pub(super) command_queue: Vec<RemoteQueueItem>,
    pub(super) latest_remote_capability: Option<String>,
    pub(super) latest_remote_execution: Option<String>,
    pub(super) latest_checkpoint: Option<String>,
    pub(super) latest_orchestration: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct RemoteTransportPayload {
    pub(super) kind: String,
    pub(super) session_id: String,
    pub(super) remote_dir: String,
    pub(super) created_at: String,
    pub(super) handshake_status: String,
    pub(super) handshake_summary: String,
    pub(super) retry_backoff_secs: Vec<u64>,
    pub(super) connection_status: String,
    pub(super) connection_id: Option<String>,
    pub(super) connected_at: Option<String>,
    pub(super) disconnected_at: Option<String>,
    pub(super) reconnect_attempts: u32,
    pub(super) last_error: Option<String>,
    pub(super) last_command: Option<String>,
    pub(super) queue_gate: Option<String>,
    pub(super) last_transition_at: Option<String>,
    pub(super) latest_transport_task_id: Option<String>,
    pub(super) latest_event: Option<String>,
    pub(super) latest_event_at: Option<String>,
    pub(super) latest_event_artifact: Option<String>,
    pub(super) live_session_status: Option<String>,
    pub(super) continuity_id: Option<String>,
    pub(super) active_endpoint_id: Option<String>,
    pub(super) resume_cursor: Option<u64>,
    pub(super) latest_remote_control: Option<String>,
    pub(super) latest_remote_execution: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct RemoteSessionEndpoint {
    pub(super) endpoint_id: String,
    pub(super) device_kind: String,
    pub(super) device_label: String,
    pub(super) status: String,
    pub(super) connection_id: Option<String>,
    pub(super) last_seen_at: String,
    pub(super) last_result_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct RemoteLiveSessionPayload {
    pub(super) kind: String,
    pub(super) session_id: String,
    pub(super) continuity_id: String,
    pub(super) created_at: String,
    pub(super) updated_at: String,
    pub(super) session_status: String,
    pub(super) transport_status: String,
    pub(super) active_endpoint_id: Option<String>,
    pub(super) resume_count: u32,
    pub(super) last_resumed_at: Option<String>,
    pub(super) latest_queue_item_id: Option<String>,
    pub(super) latest_result_id: Option<String>,
    pub(super) latest_result_status: Option<String>,
    pub(super) latest_result_summary: Option<String>,
    pub(super) result_cursor: u64,
    pub(super) resume_cursor: u64,
    pub(super) latest_remote_control: Option<String>,
    pub(super) latest_transport_state: Option<String>,
    pub(super) latest_transport_events: Option<String>,
    pub(super) latest_transcript_path: Option<String>,
    pub(super) transcript_sync_status: String,
    pub(super) last_transcript_sync_at: Option<String>,
    pub(super) transcript_sync_artifact: Option<String>,
    pub(super) endpoints: Vec<RemoteSessionEndpoint>,
}

#[derive(Debug, Clone)]
pub(super) struct RemoteControlArtifactSet {
    pub(super) summary_path: PathBuf,
    pub(super) state_path: PathBuf,
    pub(super) queue_path: PathBuf,
}

#[derive(Debug, Clone)]
pub(super) struct RemoteTransportArtifactSet {
    pub(super) summary_path: PathBuf,
    pub(super) state_path: PathBuf,
}

#[derive(Debug, Clone)]
pub(super) struct RemoteLiveSessionArtifactSet {
    pub(super) summary_path: PathBuf,
    pub(super) state_path: PathBuf,
}
