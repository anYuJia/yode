mod artifacts;
mod params;
mod paths;
mod queue;
mod render;
mod status;
mod storage;
mod types;

use std::path::Path;
use std::sync::Arc;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::sync::Mutex;

use crate::runtime_tasks::RuntimeTaskStore;
use crate::tool::{Tool, ToolCapabilities, ToolContext, ToolResult};

use artifacts::{
    load_or_create_remote_control_payload_async, load_or_create_remote_live_session_payload_async,
    load_or_create_remote_transport_payload_async, probe_remote_transport_async,
    record_remote_transport_event_async, write_remote_control_payload_async,
    write_remote_live_session_payload_async, write_remote_queue_execution_artifact_async,
    write_remote_session_transcript_sync_artifact_async, write_remote_transport_payload_async,
};
use params::{RemoteQueueDispatchParams, RemoteQueueResultParams, RemoteTransportControlParams};
use paths::{
    latest_artifact_by_suffix_async, latest_remote_transport_events_artifact_async,
    latest_remote_transport_state_artifact_async, latest_transcript_artifact_async, now_string,
    remote_dir,
};
use queue::{insert_queue_item, resolve_queue_index};
use status::{
    normalize_result_status, queue_status_label, sanitize_label, summarize_queue_status,
    transport_block_reason, truncate_preview,
};
use types::{RemoteLiveSessionPayload, RemoteSessionEndpoint, RemoteTransportPayload};

pub struct RemoteQueueDispatchTool;
pub struct RemoteQueueResultTool;
pub struct RemoteTransportControlTool;

#[async_trait]
impl Tool for RemoteQueueDispatchTool {
    fn name(&self) -> &str {
        "remote_queue_dispatch"
    }

    fn user_facing_name(&self) -> &str {
        "Remote Queue Dispatch"
    }

    fn activity_description(&self, params: &Value) -> String {
        let target = params
            .get("command")
            .and_then(|value| value.as_str())
            .or_else(|| params.get("target").and_then(|value| value.as_str()))
            .unwrap_or("latest");
        format!("Dispatching remote queue item: {}", target)
    }

    fn description(&self) -> &str {
        "Dispatch an item from the remote control queue as a first-class tool. Optionally insert a new command into the queue before dispatching it."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "target": {
                    "type": "string",
                    "description": "Queue target to dispatch. Supports latest, 1-based index, queue item id, or exact command. Defaults to latest."
                },
                "command": {
                    "type": "string",
                    "description": "Optional command to insert at the head of the queue before dispatching."
                },
                "transcript_path": {
                    "type": "string",
                    "description": "Optional transcript path to associate with the runtime task and result flow."
                },
                "summary": {
                    "type": "string",
                    "description": "Optional preview text stored on the queue item and execution artifact."
                }
            }
        })
    }

    fn capabilities(&self) -> ToolCapabilities {
        ToolCapabilities {
            requires_confirmation: true,
            supports_auto_execution: false,
            read_only: false,
        }
    }

    async fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolResult> {
        let params: RemoteQueueDispatchParams = serde_json::from_value(params)?;
        let project_root = ctx
            .working_dir
            .as_deref()
            .ok_or_else(|| anyhow!("working_dir not available"))?;
        let session_id = ctx.session_id.as_deref().unwrap_or("remote-tool-session");
        let provider = ctx.provider.as_deref().unwrap_or("unknown");
        let model = ctx.model.as_deref().unwrap_or("unknown");

        let mut payload = load_or_create_remote_control_payload_async(
            project_root,
            session_id,
            provider,
            model,
            "tool-driven remote control queue",
        )
        .await;
        let mut transport =
            load_or_create_remote_transport_payload_async(project_root, session_id).await;
        if transport.connection_status != "connected" {
            transport.last_command = Some("remote_queue_dispatch".to_string());
            transport.queue_gate = Some(format!("blocked: {}", transport_block_reason(&transport)));
            transport.last_transition_at = Some(now_string());
            let transport_artifacts =
                write_remote_transport_payload_async(project_root, session_id, &mut transport)
                    .await?;
            let mut live_session =
                load_or_create_remote_live_session_payload_async(project_root, session_id).await;
            sync_live_session_with_transport(
                project_root,
                session_id,
                &transport,
                &mut live_session,
            )
            .await;
            let live_artifacts = write_remote_live_session_payload_async(
                project_root,
                session_id,
                &mut live_session,
            )
            .await?;
            return Ok(ToolResult::error_typed(
                format!(
                    "Remote transport is not connected. Use `remote_transport_control` with action=\"connect\" or action=\"reconnect\" first.\nTransport: {}\nLive session: {}",
                    transport_artifacts.state_path.display(),
                    live_artifacts.state_path.display()
                ),
                crate::tool::ToolErrorType::Execution,
                true,
                Some("connect remote transport before dispatching the queue".to_string()),
            ));
        }

        if let Some(command) = params
            .command
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            insert_queue_item(&mut payload, command.trim());
        }

        let target = params.target.as_deref().unwrap_or("latest");
        let index = resolve_queue_index(&payload, target)
            .ok_or_else(|| anyhow!("Unknown queue target '{}'.", target))?;
        let transcript_path = params
            .transcript_path
            .clone()
            .or(latest_transcript_artifact_async(project_root).await);

        let task = start_runtime_task(
            ctx.runtime_tasks.as_ref(),
            "remote-control".to_string(),
            "remote_queue_dispatch".to_string(),
            format!("queue {}", payload.command_queue[index].command),
            transcript_path.clone(),
        )
        .await;

        let task_id = task.as_ref().map(|task| task.id.clone());
        let preview = params.summary.clone().unwrap_or_else(|| {
            format!(
                "transport dispatched {}",
                payload.command_queue[index].command
            )
        });
        let item_id = {
            let item = &mut payload.command_queue[index];
            if !matches!(item.status.as_str(), "running" | "dispatched") {
                item.attempts = item.attempts.saturating_add(1);
            }
            item.status = "dispatched".to_string();
            item.last_run_at = Some(now_string());
            item.last_result_preview = Some(truncate_preview(&preview, 180));
            if transcript_path.is_some() {
                item.transcript_path = transcript_path.clone();
            }
            if task_id.is_some() {
                item.runtime_task_id = task_id.clone();
            }
            item.id.clone()
        };
        payload.status = summarize_queue_status(&payload.command_queue);

        let event_path = record_remote_transport_event_async(
            project_root,
            session_id,
            "dispatch",
            Some(item_id.as_str()),
            task_id.as_deref(),
            &format!(
                "queued {} for remote execution",
                payload.command_queue[index].command
            ),
        )
        .await?;
        let execution_path = write_remote_queue_execution_artifact_async(
            project_root,
            &payload.command_queue[index],
            "dispatched",
            &preview,
            Some(event_path.as_path()),
        )
        .await?;
        payload.command_queue[index].execution_artifact =
            Some(execution_path.display().to_string());

        transport.last_command = Some("remote_queue_dispatch".to_string());
        transport.queue_gate = Some(format!("ready: dispatch {}", item_id));
        transport.last_transition_at = Some(now_string());
        transport.latest_transport_task_id = task_id.clone();
        let transport_artifacts =
            write_remote_transport_payload_async(project_root, session_id, &mut transport).await?;
        let control_artifacts =
            write_remote_control_payload_async(project_root, &mut payload).await?;

        let mut live_session =
            load_or_create_remote_live_session_payload_async(project_root, session_id).await;
        live_session.latest_queue_item_id = Some(item_id.clone());
        live_session.updated_at = now_string();
        sync_live_session_with_transport(project_root, session_id, &transport, &mut live_session)
            .await;
        let live_artifacts =
            write_remote_live_session_payload_async(project_root, session_id, &mut live_session)
                .await?;

        Ok(ToolResult::success_with_metadata(
            format!(
                "Remote queue item dispatched.\nItem: {}\nTask: {}\nExecution: {}\nQueue: {}\nTransport: {}",
                item_id,
                task_id.as_deref().unwrap_or("none"),
                execution_path.display(),
                control_artifacts.state_path.display(),
                transport_artifacts.state_path.display(),
            ),
            json!({
                "item_id": item_id,
                "runtime_task_id": task_id,
                "execution_artifact": execution_path.display().to_string(),
                "transport_event_artifact": event_path.display().to_string(),
                "remote_control_summary_artifact": control_artifacts.summary_path.display().to_string(),
                "remote_control_state_artifact": control_artifacts.state_path.display().to_string(),
                "remote_control_queue_artifact": control_artifacts.queue_path.display().to_string(),
                "remote_transport_summary_artifact": transport_artifacts.summary_path.display().to_string(),
                "remote_transport_state_artifact": transport_artifacts.state_path.display().to_string(),
                "remote_live_session_summary_artifact": live_artifacts.summary_path.display().to_string(),
                "remote_live_session_state_artifact": live_artifacts.state_path.display().to_string(),
                "transcript_path": transcript_path,
                "tool_surface": "first_class_remote_queue_dispatch"
            }),
        ))
    }
}

#[async_trait]
impl Tool for RemoteQueueResultTool {
    fn name(&self) -> &str {
        "remote_queue_result"
    }

    fn user_facing_name(&self) -> &str {
        "Remote Queue Result"
    }

    fn activity_description(&self, params: &Value) -> String {
        let status = params
            .get("status")
            .and_then(|value| value.as_str())
            .unwrap_or("completed");
        format!("Recording remote queue result: {}", status)
    }

    fn description(&self) -> &str {
        "Record a completed, failed, or acknowledged result for a remote queue item as a first-class tool. This updates queue, execution, transport, and live-session artifacts together."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "target": {
                    "type": "string",
                    "description": "Queue target to update. Supports latest, 1-based index, item id, or exact command. Defaults to latest."
                },
                "status": {
                    "type": "string",
                    "enum": ["completed", "failed", "acknowledged"],
                    "description": "Result status to persist."
                },
                "summary": {
                    "type": "string",
                    "description": "Human summary for the result."
                },
                "transcript_path": {
                    "type": "string",
                    "description": "Optional transcript path to associate with this result."
                },
                "result_id": {
                    "type": "string",
                    "description": "Optional stable result identifier."
                },
                "endpoint_id": {
                    "type": "string",
                    "description": "Optional endpoint identifier for the result source."
                },
                "device_kind": {
                    "type": "string",
                    "description": "Optional endpoint device kind."
                },
                "device_label": {
                    "type": "string",
                    "description": "Optional endpoint display label."
                },
                "source": {
                    "type": "string",
                    "description": "Optional result source label."
                }
            },
            "required": ["status", "summary"]
        })
    }

    fn capabilities(&self) -> ToolCapabilities {
        ToolCapabilities {
            requires_confirmation: true,
            supports_auto_execution: false,
            read_only: false,
        }
    }

    async fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolResult> {
        let params: RemoteQueueResultParams = serde_json::from_value(params)?;
        let project_root = ctx
            .working_dir
            .as_deref()
            .ok_or_else(|| anyhow!("working_dir not available"))?;
        let session_id = ctx.session_id.as_deref().unwrap_or("remote-tool-session");
        let provider = ctx.provider.as_deref().unwrap_or("unknown");
        let model = ctx.model.as_deref().unwrap_or("unknown");
        let mut payload = load_or_create_remote_control_payload_async(
            project_root,
            session_id,
            provider,
            model,
            "tool-driven remote control queue",
        )
        .await;
        let mut transport =
            load_or_create_remote_transport_payload_async(project_root, session_id).await;
        let mut live_session =
            load_or_create_remote_live_session_payload_async(project_root, session_id).await;

        let target = params.target.as_deref().unwrap_or("latest");
        let index = resolve_queue_index(&payload, target)
            .ok_or_else(|| anyhow!("Unknown queue target '{}'.", target))?;
        let normalized_status = normalize_result_status(&params.status)?;
        let result_id = params
            .result_id
            .clone()
            .unwrap_or_else(|| format!("result-{}", uuid::Uuid::new_v4()));
        let now = now_string();

        let item_snapshot = {
            let item = &mut payload.command_queue[index];
            item.status = normalized_status.to_string();
            item.last_run_at = Some(now.clone());
            item.last_result_preview = Some(truncate_preview(&params.summary, 180));
            if normalized_status == "acked" {
                item.acknowledged_at = Some(now.clone());
            }
            if params.transcript_path.is_some() {
                item.transcript_path = params.transcript_path.clone();
            }
            item.clone()
        };

        if let (Some(store), Some(task_id)) = (
            ctx.runtime_tasks.as_ref(),
            item_snapshot.runtime_task_id.as_deref(),
        ) {
            let mut store = store.lock().await;
            store.update_progress(task_id, params.summary.clone());
            match normalized_status {
                "completed" => store.mark_completed(task_id),
                "failed" => store.mark_failed(task_id, params.summary.clone()),
                "acked" => {}
                _ => {}
            }
        }

        let event_path = record_remote_transport_event_async(
            project_root,
            session_id,
            match normalized_status {
                "completed" => "result_completed",
                "failed" => "result_failed",
                "acked" => "ack",
                _ => "result",
            },
            Some(item_snapshot.id.as_str()),
            item_snapshot.runtime_task_id.as_deref(),
            &params.summary,
        )
        .await?;
        let execution_path = write_remote_queue_execution_artifact_async(
            project_root,
            &item_snapshot,
            normalized_status,
            &params.summary,
            Some(event_path.as_path()),
        )
        .await?;
        payload.command_queue[index].execution_artifact =
            Some(execution_path.display().to_string());
        payload.status = summarize_queue_status(&payload.command_queue);

        transport.last_command = Some("remote_queue_result".to_string());
        transport.queue_gate = Some(format!("ready: {} {}", normalized_status, item_snapshot.id));
        transport.last_transition_at = Some(now.clone());
        let transport_artifacts =
            write_remote_transport_payload_async(project_root, session_id, &mut transport).await?;
        let control_artifacts =
            write_remote_control_payload_async(project_root, &mut payload).await?;

        live_session.latest_queue_item_id = Some(item_snapshot.id.clone());
        if normalized_status != "acked" {
            live_session.latest_result_id = Some(result_id.clone());
            live_session.latest_result_status = Some(normalized_status.to_string());
            live_session.latest_result_summary = Some(params.summary.clone());
            live_session.result_cursor = live_session.result_cursor.saturating_add(1);
        }
        live_session.updated_at = now.clone();
        let endpoint = local_remote_endpoint(
            &transport,
            params.endpoint_id.as_deref(),
            params.device_kind.as_deref(),
            params.device_label.as_deref(),
            Some(result_id.as_str()),
        );
        upsert_remote_session_endpoint(&mut live_session, endpoint.clone());
        live_session.active_endpoint_id = Some(endpoint.endpoint_id.clone());
        let transcript_sync_path = if normalized_status == "acked" {
            None
        } else {
            let transcript_path = params
                .transcript_path
                .clone()
                .or_else(|| item_snapshot.transcript_path.clone());
            write_remote_session_transcript_sync_artifact_async(
                project_root,
                session_id,
                item_snapshot.id.as_str(),
                transcript_path.as_deref(),
                result_id.as_str(),
                Some(endpoint.endpoint_id.as_str()),
            )
            .await?
        };
        if let Some(path) = transcript_sync_path.as_ref() {
            live_session.transcript_sync_status = "synced".to_string();
            live_session.last_transcript_sync_at = Some(now.clone());
            live_session.transcript_sync_artifact = Some(path.display().to_string());
        }
        sync_live_session_with_transport(project_root, session_id, &transport, &mut live_session)
            .await;
        let live_artifacts =
            write_remote_live_session_payload_async(project_root, session_id, &mut live_session)
                .await?;

        Ok(ToolResult::success_with_metadata(
            format!(
                "Remote queue result recorded.\nItem: {}\nStatus: {}\nExecution: {}\nQueue: {}\nLive session: {}",
                item_snapshot.id,
                queue_status_label(normalized_status),
                execution_path.display(),
                control_artifacts.state_path.display(),
                live_artifacts.state_path.display(),
            ),
            json!({
                "item_id": item_snapshot.id,
                "status": normalized_status,
                "result_id": result_id,
                "execution_artifact": execution_path.display().to_string(),
                "transport_event_artifact": event_path.display().to_string(),
                "transcript_sync_artifact": transcript_sync_path.as_ref().map(|path| path.display().to_string()),
                "remote_control_summary_artifact": control_artifacts.summary_path.display().to_string(),
                "remote_control_state_artifact": control_artifacts.state_path.display().to_string(),
                "remote_control_queue_artifact": control_artifacts.queue_path.display().to_string(),
                "remote_transport_summary_artifact": transport_artifacts.summary_path.display().to_string(),
                "remote_transport_state_artifact": transport_artifacts.state_path.display().to_string(),
                "remote_live_session_summary_artifact": live_artifacts.summary_path.display().to_string(),
                "remote_live_session_state_artifact": live_artifacts.state_path.display().to_string(),
                "result_source": params.source,
                "tool_surface": "first_class_remote_queue_result"
            }),
        ))
    }
}

#[async_trait]
impl Tool for RemoteTransportControlTool {
    fn name(&self) -> &str {
        "remote_transport_control"
    }

    fn user_facing_name(&self) -> &str {
        "Remote Transport"
    }

    fn activity_description(&self, params: &Value) -> String {
        let action = params
            .get("action")
            .and_then(|value| value.as_str())
            .unwrap_or("status");
        format!("Remote transport action: {}", action)
    }

    fn description(&self) -> &str {
        "Connect, reconnect, disconnect, or inspect the remote transport as a first-class tool. This tool keeps transport and live-session artifacts in sync."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["status", "connect", "disconnect", "reconnect"],
                    "description": "Transport action to execute."
                },
                "detail": {
                    "type": "string",
                    "description": "Optional operator detail stored in the transport event log."
                }
            },
            "required": ["action"]
        })
    }

    fn capabilities(&self) -> ToolCapabilities {
        ToolCapabilities {
            requires_confirmation: true,
            supports_auto_execution: false,
            read_only: false,
        }
    }

    async fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolResult> {
        let params: RemoteTransportControlParams = serde_json::from_value(params)?;
        let project_root = ctx
            .working_dir
            .as_deref()
            .ok_or_else(|| anyhow!("working_dir not available"))?;
        let session_id = ctx.session_id.as_deref().unwrap_or("remote-tool-session");
        let mut transport =
            load_or_create_remote_transport_payload_async(project_root, session_id).await;
        let mut live_session =
            load_or_create_remote_live_session_payload_async(project_root, session_id).await;

        let task = if matches!(params.action.as_str(), "connect" | "reconnect") {
            start_runtime_task(
                ctx.runtime_tasks.as_ref(),
                "remote-transport".to_string(),
                "remote_transport_control".to_string(),
                format!("{} remote transport", params.action),
                None,
            )
            .await
        } else {
            None
        };
        let task_id = task.as_ref().map(|task| task.id.clone());

        match params.action.as_str() {
            "status" => {}
            "connect" => {
                probe_remote_transport_async(project_root).await?;
                transport.connection_status = "connected".to_string();
                transport.connection_id = Some(format!("transport-{}", uuid::Uuid::new_v4()));
                transport.connected_at = Some(now_string());
                transport.disconnected_at = None;
                transport.last_error = None;
                transport.last_command = Some("remote_transport_control:connect".to_string());
                transport.queue_gate = Some("ready: transport connected".to_string());
                transport.last_transition_at = Some(now_string());
                transport.latest_transport_task_id = task_id.clone();
            }
            "disconnect" => {
                transport.connection_status = "disconnected".to_string();
                transport.connection_id = None;
                transport.disconnected_at = Some(now_string());
                transport.last_error = None;
                transport.last_command = Some("remote_transport_control:disconnect".to_string());
                transport.queue_gate = Some("blocked: transport disconnected".to_string());
                transport.last_transition_at = Some(now_string());
            }
            "reconnect" => {
                probe_remote_transport_async(project_root).await?;
                transport.connection_status = "connected".to_string();
                transport.connection_id = Some(format!("transport-{}", uuid::Uuid::new_v4()));
                transport.connected_at = Some(now_string());
                transport.disconnected_at = None;
                transport.reconnect_attempts = transport.reconnect_attempts.saturating_add(1);
                transport.last_error = None;
                transport.last_command = Some("remote_transport_control:reconnect".to_string());
                transport.queue_gate = Some("ready: transport reconnected".to_string());
                transport.last_transition_at = Some(now_string());
                transport.latest_transport_task_id = task_id.clone();
            }
            other => return Err(anyhow!("Unsupported action '{}'.", other)),
        }

        let detail = params
            .detail
            .clone()
            .unwrap_or_else(|| format!("transport {}", params.action));
        let event_path = record_remote_transport_event_async(
            project_root,
            session_id,
            params.action.as_str(),
            None,
            task_id.as_deref(),
            detail.as_str(),
        )
        .await?;
        if let (Some(store), Some(task_id)) = (ctx.runtime_tasks.as_ref(), task_id.as_deref()) {
            let mut store = store.lock().await;
            store.update_progress(task_id, detail.clone());
            store.mark_completed(task_id);
        }

        let transport_artifacts =
            write_remote_transport_payload_async(project_root, session_id, &mut transport).await?;
        sync_live_session_with_transport(project_root, session_id, &transport, &mut live_session)
            .await;
        let live_artifacts =
            write_remote_live_session_payload_async(project_root, session_id, &mut live_session)
                .await?;

        Ok(ToolResult::success_with_metadata(
            format!(
                "Remote transport {}.\nTransport: {}\nLive session: {}",
                params.action,
                transport_artifacts.state_path.display(),
                live_artifacts.state_path.display(),
            ),
            json!({
                "action": params.action,
                "runtime_task_id": task_id,
                "transport_event_artifact": event_path.display().to_string(),
                "remote_transport_summary_artifact": transport_artifacts.summary_path.display().to_string(),
                "remote_transport_state_artifact": transport_artifacts.state_path.display().to_string(),
                "remote_live_session_summary_artifact": live_artifacts.summary_path.display().to_string(),
                "remote_live_session_state_artifact": live_artifacts.state_path.display().to_string(),
                "tool_surface": "first_class_remote_transport_control"
            }),
        ))
    }
}

fn start_runtime_task(
    runtime_tasks: Option<&Arc<Mutex<RuntimeTaskStore>>>,
    kind: String,
    source_tool: String,
    description: String,
    transcript_path: Option<String>,
) -> std::pin::Pin<
    Box<dyn std::future::Future<Output = Option<crate::runtime_tasks::RuntimeTask>> + Send + '_>,
> {
    Box::pin(async move {
        let store = runtime_tasks?;
        let output_path =
            std::env::temp_dir().join(format!("yode-remote-{}.log", uuid::Uuid::new_v4()));
        let mut store = store.lock().await;
        let (task, _) = store.create_with_transcript(
            kind,
            source_tool,
            description.clone(),
            output_path.display().to_string(),
            transcript_path,
        );
        store.mark_running(&task.id);
        store.update_progress(&task.id, description);
        Some(task)
    })
}

async fn sync_live_session_with_transport(
    project_root: &Path,
    session_id: &str,
    transport: &RemoteTransportPayload,
    payload: &mut RemoteLiveSessionPayload,
) {
    payload.transport_status = transport.connection_status.clone();
    payload.session_status = if payload.latest_result_status.as_deref() == Some("failed") {
        "attention".to_string()
    } else {
        match transport.connection_status.as_str() {
            "connected" => "live".to_string(),
            "reconnecting" => "resuming".to_string(),
            "error" => "attention".to_string(),
            _ => "idle".to_string(),
        }
    };
    payload.latest_transport_state = latest_remote_transport_state_artifact_async(project_root)
        .await
        .map(|path| path.display().to_string());
    payload.latest_transport_events = latest_remote_transport_events_artifact_async(project_root)
        .await
        .map(|path| path.display().to_string());
    payload.latest_remote_control =
        latest_artifact_by_suffix_async(&remote_dir(project_root), "remote-control.md")
            .await
            .map(|path| path.display().to_string());
    let endpoint = local_remote_endpoint(
        transport,
        None,
        None,
        None,
        payload.latest_result_id.as_deref(),
    );
    upsert_remote_session_endpoint(payload, endpoint.clone());
    if payload.active_endpoint_id.is_none() {
        payload.active_endpoint_id = Some(endpoint.endpoint_id);
    }
    if payload.resume_cursor == 0 {
        payload.resume_cursor = transport.resume_cursor.unwrap_or(0);
    }
    if payload.session_id != session_id {
        payload.session_id = session_id.to_string();
    }
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

fn default_remote_endpoint_id() -> String {
    std::env::var("HOSTNAME")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .map(|value| format!("local-{}", sanitize_label(&value)))
        .unwrap_or_else(|| "local-terminal".to_string())
}

#[cfg(test)]
mod tests {
    use super::paths::{
        latest_remote_control_state_artifact_async,
        latest_remote_live_session_state_artifact_async,
        latest_remote_transport_event_log_artifact_async,
        latest_remote_transport_state_artifact_async,
    };
    use super::{RemoteQueueDispatchTool, RemoteQueueResultTool, RemoteTransportControlTool};
    use crate::runtime_tasks::RuntimeTaskStore;
    use crate::tool::{Tool, ToolContext};
    use serde_json::json;
    use std::path::Path;
    use std::sync::Arc;
    use tempfile::tempdir;
    use tokio::sync::Mutex;

    fn test_context(dir: &Path) -> ToolContext {
        let mut ctx = ToolContext::empty();
        ctx.working_dir = Some(dir.to_path_buf());
        ctx.session_id = Some("session-12345678".to_string());
        ctx.provider = Some("anthropic".to_string());
        ctx.model = Some("claude".to_string());
        ctx.runtime_tasks = Some(Arc::new(Mutex::new(RuntimeTaskStore::new())));
        ctx
    }

    #[tokio::test]
    async fn transport_tool_connects_and_writes_live_session_state() {
        let dir = tempdir().unwrap();
        let tool = RemoteTransportControlTool;
        let result = tool
            .execute(json!({"action":"connect"}), &test_context(dir.path()))
            .await
            .unwrap();
        assert!(!result.is_error);
        assert!(latest_remote_transport_state_artifact_async(dir.path())
            .await
            .is_some());
        assert!(latest_remote_live_session_state_artifact_async(dir.path())
            .await
            .is_some());
    }

    #[tokio::test]
    async fn dispatch_and_result_tools_write_compatible_artifacts() {
        let dir = tempdir().unwrap();
        let ctx = test_context(dir.path());
        RemoteTransportControlTool
            .execute(json!({"action":"connect"}), &ctx)
            .await
            .unwrap();
        let dispatch = RemoteQueueDispatchTool
            .execute(
                json!({"command":"echo remote hello","summary":"dispatch remote hello"}),
                &ctx,
            )
            .await
            .unwrap();
        assert!(!dispatch.is_error);
        assert!(latest_remote_control_state_artifact_async(dir.path())
            .await
            .is_some());

        let result = RemoteQueueResultTool
            .execute(
                json!({"target":"latest","status":"completed","summary":"remote hello finished"}),
                &ctx,
            )
            .await
            .unwrap();
        assert!(!result.is_error);
        let live_state = std::fs::read_to_string(
            latest_remote_live_session_state_artifact_async(dir.path())
                .await
                .unwrap(),
        )
        .unwrap();
        assert!(live_state.contains("\"latest_result_status\": \"completed\""));

        let event_log = latest_remote_transport_event_log_artifact_async(dir.path())
            .await
            .unwrap();
        let entries = std::fs::read_to_string(event_log)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0]["cursor"], 1);
        assert_eq!(entries[0]["event"], "connect");
        assert_eq!(entries[1]["cursor"], 2);
        assert_eq!(entries[1]["event"], "dispatch");
        assert_eq!(entries[2]["cursor"], 3);
        assert_eq!(entries[2]["event"], "result_completed");
        let transport_state = std::fs::read_to_string(
            latest_remote_transport_state_artifact_async(dir.path())
                .await
                .unwrap(),
        )
        .unwrap();
        assert!(transport_state.contains("\"resume_cursor\": 3"));
    }

    #[tokio::test]
    async fn dispatch_requires_connected_transport() {
        let dir = tempdir().unwrap();
        let ctx = test_context(dir.path());
        let result = RemoteQueueDispatchTool
            .execute(json!({"command":"echo blocked"}), &ctx)
            .await
            .unwrap();
        assert!(result.is_error);
        assert!(result.recoverable);
        assert!(result
            .suggestion
            .as_deref()
            .unwrap_or("")
            .contains("connect remote transport"));
        assert!(latest_remote_transport_state_artifact_async(dir.path())
            .await
            .is_some());
        assert!(latest_remote_live_session_state_artifact_async(dir.path())
            .await
            .is_some());
    }

    #[tokio::test]
    async fn reconnect_increments_transport_attempts() {
        let dir = tempdir().unwrap();
        let ctx = test_context(dir.path());
        RemoteTransportControlTool
            .execute(json!({"action":"connect"}), &ctx)
            .await
            .unwrap();
        let result = RemoteTransportControlTool
            .execute(json!({"action":"reconnect","detail":"retry"}), &ctx)
            .await
            .unwrap();
        assert!(!result.is_error);
        let state = std::fs::read_to_string(
            latest_remote_transport_state_artifact_async(dir.path())
                .await
                .unwrap(),
        )
        .unwrap();
        assert!(state.contains("\"reconnect_attempts\": 1"));
    }
}
