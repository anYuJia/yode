use crate::commands::artifact_nav::{
    attach_inspector_actions, open_artifact_inspector, stale_artifact_actions,
};
use crate::commands::context::CommandContext;
use crate::commands::{
    ArgCompletionSource, ArgDef, Command, CommandCategory, CommandMeta, CommandOutput,
    CommandResult,
};

use super::remote_control_workspace::{
    bind_remote_queue_item_runtime, current_remote_transport_payload, export_remote_control_bundle,
    ingest_remote_queue_result, latest_remote_command_queue_artifact,
    latest_remote_control_artifact, latest_remote_live_session_artifact,
    load_remote_queue_result_ingest, mark_remote_live_session_dispatch,
    latest_remote_transport_artifact, mark_remote_queue_item, mark_remote_transport_connected,
    mark_remote_transport_disconnected, mark_remote_transport_failed,
    mark_remote_transport_reconnecting, note_remote_transport_dispatch, queue_item_target,
    record_remote_transport_event, render_remote_control_doctor, render_remote_retry_summary,
    sync_remote_live_session_transport,
    render_remote_task_inventory, write_remote_control_artifacts,
    write_remote_live_session_artifacts,
    write_remote_queue_execution_artifact, write_remote_task_handoff_artifact,
    write_remote_transport_artifacts,
};

pub struct RemoteControlCommand {
    meta: CommandMeta,
}

impl RemoteControlCommand {
    pub fn new() -> Self {
        Self {
            meta: CommandMeta {
                name: "remote-control",
                description: "Plan and inspect remote control session artifacts",
                aliases: &[],
                args: vec![ArgDef {
                    name: "action".to_string(),
                    required: false,
                    hint: "[plan [goal]|latest|session [status|sync]|transport [status|connect|disconnect|reconnect]|queue|dispatch <item>|run <item>|ingest <file>|complete <item> [summary]|fail <item> [reason]|retry <item>|ack <item>|tasks|monitor|follow <id>|retry-summary|handoff <id>|doctor|bundle]".to_string(),
                    completions: ArgCompletionSource::Static(vec![
                        "plan".to_string(),
                        "latest".to_string(),
                        "session".to_string(),
                        "session status".to_string(),
                        "session sync".to_string(),
                        "transport".to_string(),
                        "transport status".to_string(),
                        "transport connect".to_string(),
                        "transport disconnect".to_string(),
                        "transport reconnect".to_string(),
                        "queue".to_string(),
                        "dispatch".to_string(),
                        "run".to_string(),
                        "ingest".to_string(),
                        "complete".to_string(),
                        "fail".to_string(),
                        "retry".to_string(),
                        "ack".to_string(),
                        "tasks".to_string(),
                        "monitor".to_string(),
                        "follow".to_string(),
                        "retry-summary".to_string(),
                        "handoff".to_string(),
                        "doctor".to_string(),
                        "bundle".to_string(),
                    ]),
                }],
                category: CommandCategory::Development,
                hidden: false,
            },
        }
    }
}

impl Command for RemoteControlCommand {
    fn meta(&self) -> &CommandMeta {
        &self.meta
    }

    fn execute(&self, args: &str, ctx: &mut CommandContext) -> CommandResult {
        let project_root = std::path::PathBuf::from(&ctx.session.working_dir);
        let trimmed = args.trim();
        let parts = trimmed.split_whitespace().collect::<Vec<_>>();

        if matches!(parts.first(), None | Some(&"plan")) {
            let goal = parts
                .get(1..)
                .map(|parts| parts.join(" "))
                .unwrap_or_default();
            let artifacts = write_remote_control_artifacts(
                &project_root,
                &ctx.session.session_id,
                ctx.provider_name.as_str(),
                &ctx.session.model,
                &goal,
            )
            .map_err(|err| format!("Failed to write remote control artifacts: {}", err))?;
            let _ = write_remote_live_session_artifacts(&project_root, &ctx.session.session_id);
            return Ok(CommandOutput::Message(format!(
                "Planned remote control session.\nSummary: {}\nState: {}\nQueue: {}",
                artifacts.summary_path.display(),
                artifacts.state_path.display(),
                artifacts.queue_path.display(),
            )));
        }

        if trimmed == "latest" {
            let path = latest_remote_control_artifact(&project_root)
                .ok_or_else(|| "No remote control artifacts found.".to_string())?;
            let footer = remote_control_footer(&path);
            let doc = open_artifact_inspector(
                "Remote control inspector",
                &path,
                Some(footer),
                vec![("kind".into(), "remote_control".into())],
            )
            .ok_or_else(|| format!("Failed to open remote control artifact {}.", path.display()))?;
            let mut doc = doc;
            attach_inspector_actions(
                &mut doc,
                vec![
                    (
                        "transport".to_string(),
                        "/remote-control transport".to_string(),
                    ),
                    ("session".to_string(), "/remote-control session".to_string()),
                    ("queue".to_string(), "/remote-control queue".to_string()),
                    ("tasks".to_string(), "/remote-control tasks".to_string()),
                    ("monitor".to_string(), "/remote-control monitor".to_string()),
                    ("follow".to_string(), "/remote-control follow latest".to_string()),
                    ("doctor".to_string(), "/remote-control doctor".to_string()),
                ],
            );
            return Ok(CommandOutput::OpenInspector(doc));
        }

        if matches!(parts.as_slice(), ["session"] | ["session", "status"]) {
            let _ = write_remote_live_session_artifacts(&project_root, &ctx.session.session_id);
            return open_remote_live_session_inspector(&project_root);
        }

        if trimmed == "monitor" {
            let _ = write_remote_live_session_artifacts(&project_root, &ctx.session.session_id);
            return open_remote_live_session_inspector(&project_root);
        }

        if let ["session", "sync"] = parts.as_slice() {
            let (_, state_path) = sync_remote_live_session_transport(
                &project_root,
                &ctx.session.session_id,
            )
            .map_err(|err| format!("Failed to sync remote live session: {}", err))?;
            return Ok(CommandOutput::Message(format!(
                "Remote live session synced.\nState: {}",
                state_path.display()
            )));
        }

        if matches!(parts.as_slice(), ["transport"] | ["transport", "status"]) {
            let _ = write_remote_transport_artifacts(&project_root, &ctx.session.session_id);
            return open_remote_transport_inspector(&project_root);
        }

        if let ["transport", "connect"] = parts.as_slice() {
            let transport =
                current_remote_transport_payload(&project_root, &ctx.session.session_id);
            if transport.connection_status == "connected" {
                let _ = write_remote_transport_artifacts(&project_root, &ctx.session.session_id);
                return Ok(CommandOutput::Message(
                    "Remote transport already connected. Use `/remote-control transport reconnect` to refresh it.".to_string(),
                ));
            }
            let (task_id, output_path) =
                start_remote_transport_task(ctx, &project_root, "connect remote transport")?;
            match probe_remote_transport(&project_root) {
                Ok(summary) => {
                    let _ = std::fs::write(&output_path, format!("connected\n{}\n", summary));
                    let (_, state_path) = mark_remote_transport_connected(
                        &project_root,
                        &ctx.session.session_id,
                        "/remote-control transport connect",
                        Some(task_id.as_str()),
                        false,
                    )
                    .map_err(|err| format!("Failed to update remote transport state: {}", err))?;
                    let _ = record_remote_transport_event(
                        &project_root,
                        &ctx.session.session_id,
                        "connect",
                        None,
                        Some(task_id.as_str()),
                        &summary,
                    );
                    let _ = sync_remote_live_session_transport(&project_root, &ctx.session.session_id);
                    finish_remote_transport_task(ctx, &task_id, None, &summary);
                    return Ok(CommandOutput::Message(format!(
                        "Remote transport connected.\nTask: {}\nState: {}",
                        task_id,
                        state_path.display()
                    )));
                }
                Err(err) => {
                    let _ = std::fs::write(&output_path, format!("connect failed\n{}\n", err));
                    let err = err.to_string();
                    let _ = mark_remote_transport_failed(
                        &project_root,
                        &ctx.session.session_id,
                        "/remote-control transport connect",
                        &err,
                        false,
                        Some(task_id.as_str()),
                    );
                    let _ = record_remote_transport_event(
                        &project_root,
                        &ctx.session.session_id,
                        "connect_failed",
                        None,
                        Some(task_id.as_str()),
                        &err,
                    );
                    let _ = sync_remote_live_session_transport(&project_root, &ctx.session.session_id);
                    finish_remote_transport_task(ctx, &task_id, Some(&err), &err);
                    return Ok(CommandOutput::Message(format!(
                        "Remote transport connect failed: {}",
                        err
                    )));
                }
            }
        }

        if let ["transport", "disconnect"] = parts.as_slice() {
            let (_, state_path) = mark_remote_transport_disconnected(
                &project_root,
                &ctx.session.session_id,
                "/remote-control transport disconnect",
            )
            .map_err(|err| format!("Failed to update remote transport state: {}", err))?;
            let _ = record_remote_transport_event(
                &project_root,
                &ctx.session.session_id,
                "disconnect",
                None,
                None,
                "operator disconnected remote transport",
            );
            let _ = sync_remote_live_session_transport(&project_root, &ctx.session.session_id);
            return Ok(CommandOutput::Message(format!(
                "Remote transport disconnected.\nState: {}",
                state_path.display()
            )));
        }

        if let ["transport", "reconnect"] = parts.as_slice() {
            let (task_id, output_path) =
                start_remote_transport_task(ctx, &project_root, "reconnect remote transport")?;
            let _ = mark_remote_transport_reconnecting(
                &project_root,
                &ctx.session.session_id,
                "/remote-control transport reconnect",
                Some(task_id.as_str()),
            );
            match probe_remote_transport(&project_root) {
                Ok(summary) => {
                    let _ = std::fs::write(&output_path, format!("reconnected\n{}\n", summary));
                    let (_, state_path) = mark_remote_transport_connected(
                        &project_root,
                        &ctx.session.session_id,
                        "/remote-control transport reconnect",
                        Some(task_id.as_str()),
                        true,
                    )
                    .map_err(|err| format!("Failed to update remote transport state: {}", err))?;
                    let _ = record_remote_transport_event(
                        &project_root,
                        &ctx.session.session_id,
                        "reconnect",
                        None,
                        Some(task_id.as_str()),
                        &summary,
                    );
                    let _ = sync_remote_live_session_transport(&project_root, &ctx.session.session_id);
                    finish_remote_transport_task(ctx, &task_id, None, &summary);
                    return Ok(CommandOutput::Message(format!(
                        "Remote transport reconnected.\nTask: {}\nState: {}",
                        task_id,
                        state_path.display()
                    )));
                }
                Err(err) => {
                    let _ = std::fs::write(&output_path, format!("reconnect failed\n{}\n", err));
                    let err = err.to_string();
                    let _ = mark_remote_transport_failed(
                        &project_root,
                        &ctx.session.session_id,
                        "/remote-control transport reconnect",
                        &err,
                        true,
                        Some(task_id.as_str()),
                    );
                    let _ = record_remote_transport_event(
                        &project_root,
                        &ctx.session.session_id,
                        "reconnect_failed",
                        None,
                        Some(task_id.as_str()),
                        &err,
                    );
                    let _ = sync_remote_live_session_transport(&project_root, &ctx.session.session_id);
                    finish_remote_transport_task(ctx, &task_id, Some(&err), &err);
                    return Ok(CommandOutput::Message(format!(
                        "Remote transport reconnect failed: {}",
                        err
                    )));
                }
            }
        }

        if matches!(parts.as_slice(), ["run", _] | ["dispatch", _]) {
            let target = parts[1];
            let (payload, _, index) = queue_item_target(&project_root, target)
                .map_err(|err| format!("Failed to resolve queue item: {}", err))?
                .ok_or_else(|| format!("Unknown queue target '{}'.", target))?;
            let item = payload.command_queue[index].clone();
            let transport =
                current_remote_transport_payload(&project_root, &ctx.session.session_id);
            if transport.connection_status != "connected" {
                let reason = transport_block_reason(&transport);
                let _ = note_remote_transport_dispatch(
                    &project_root,
                    &ctx.session.session_id,
                    &item.command,
                    false,
                    &reason,
                );
                return Ok(CommandOutput::Message(format!(
                    "Remote queue item blocked by transport: {}.\nRun `/remote-control transport connect` or `/remote-control transport reconnect` first.",
                    reason
                )));
            }
            let _ = note_remote_transport_dispatch(
                &project_root,
                &ctx.session.session_id,
                &item.command,
                true,
                &format!("dispatch ready for {}", item.id),
            );
            let runtime_task = start_remote_queue_task(ctx, &project_root, &item, target)?;
            let dispatch_preview = format!("transport dispatched {}", item.command);
            let _ = mark_remote_queue_item(
                &project_root,
                target,
                "dispatched",
                Some(dispatch_preview.clone()),
            )
            .map_err(|err| format!("Failed to mark queue item dispatched: {}", err))?;
            let _ = bind_remote_queue_item_runtime(
                &project_root,
                target,
                Some(runtime_task.id.clone()),
                runtime_task.transcript_path.clone(),
                None,
            )
            .map_err(|err| format!("Failed to bind queue runtime task: {}", err))?;
            let _ = mark_remote_live_session_dispatch(
                &project_root,
                &ctx.session.session_id,
                item.id.as_str(),
                Some(runtime_task.id.as_str()),
            );
            let event_path = record_remote_transport_event(
                &project_root,
                &ctx.session.session_id,
                "dispatch",
                Some(item.id.as_str()),
                Some(runtime_task.id.as_str()),
                &format!("queued {} for remote execution", item.command),
            )
            .map_err(|err| format!("Failed to record remote transport event: {}", err))?;
            {
                let engine = ctx
                    .engine
                    .try_lock()
                    .map_err(|_| "Engine is busy, try again.".to_string())?;
                engine.update_runtime_task_progress(
                    &runtime_task.id,
                    format!("dispatched over transport: {}", item.command),
                );
            }
            let (payload, _, index) = queue_item_target(&project_root, target)
                .map_err(|err| format!("Failed to reload queue item: {}", err))?
                .ok_or_else(|| format!("Unknown queue target '{}'.", target))?;
            let item = payload.command_queue[index].clone();
            let execution = write_remote_queue_execution_artifact(
                &project_root,
                &item,
                "dispatched",
                &dispatch_preview,
                Some(event_path.as_path()),
            )
            .map_err(|err| format!("Failed to write remote queue execution artifact: {}", err))?;
            let _ = bind_remote_queue_item_runtime(
                &project_root,
                target,
                Some(runtime_task.id.clone()),
                runtime_task.transcript_path.clone(),
                Some(execution.display().to_string()),
            )
            .map_err(|err| format!("Failed to update queue execution artifact: {}", err))?;
            return Ok(CommandOutput::Message(format!(
                "Remote queue item dispatched.\nItem: {}\nTask: {}\nExecution: {}\nComplete with: /remote-control complete {} <summary>",
                item.id,
                runtime_task.id,
                execution.display(),
                item.id
            )));
        }

        if let ["ingest", file] = parts.as_slice() {
            let ingest = load_remote_queue_result_ingest(std::path::Path::new(file))
                .map_err(|err| format!("Failed to load remote queue result ingest file: {}", err))?;
            let outcome = ingest_remote_queue_result(&project_root, &ctx.session.session_id, &ingest)
                .map_err(|err| format!("Failed to ingest remote queue result: {}", err))?;
            return Ok(CommandOutput::Message(format!(
                "Remote queue result ingested.\nItem: {}\nStatus: {}\nExecution: {}\nSession: {}{}",
                outcome.item_id,
                outcome.status,
                outcome.execution_path.display(),
                outcome.session_state_path.display(),
                outcome
                    .transcript_sync_path
                    .as_ref()
                    .map(|path| format!("\nTranscript sync: {}", path.display()))
                    .unwrap_or_default()
            )));
        }

        if let Some((target, tail)) = parse_target_tail(trimmed, "complete") {
            let (payload, _, index) = queue_item_target(&project_root, &target)
                .map_err(|err| format!("Failed to resolve queue item: {}", err))?
                .ok_or_else(|| format!("Unknown queue target '{}'.", target))?;
            let item = payload.command_queue[index].clone();
            let summary = if tail.is_empty() {
                format!("remote completion confirmed for {}", item.command)
            } else {
                tail
            };
            let ingest = super::remote_control_workspace::RemoteQueueResultIngest {
                item: target.clone(),
                status: "completed".to_string(),
                summary: summary.clone(),
                endpoint_id: None,
                device_kind: None,
                device_label: None,
                transcript_path: item.transcript_path.clone(),
                result_id: None,
                source: Some("operator_complete".to_string()),
            };
            let outcome = ingest_remote_queue_result(&project_root, &ctx.session.session_id, &ingest)
                .map_err(|err| format!("Failed to ingest completed remote result: {}", err))?;
            if let Some(task_id) = item.runtime_task_id.as_deref() {
                if let Ok(engine) = ctx.engine.try_lock() {
                    engine.update_runtime_task_progress(task_id, summary.clone());
                    engine.mark_runtime_task_completed(task_id);
                }
            }
            return Ok(CommandOutput::Message(format!(
                "Remote queue item completed.\nItem: {}\nExecution: {}",
                outcome.item_id,
                outcome.execution_path.display()
            )));
        }

        if let Some((target, tail)) = parse_target_tail(trimmed, "fail") {
            let (payload, _, index) = queue_item_target(&project_root, &target)
                .map_err(|err| format!("Failed to resolve queue item: {}", err))?
                .ok_or_else(|| format!("Unknown queue target '{}'.", target))?;
            let item = payload.command_queue[index].clone();
            let reason = if tail.is_empty() {
                format!("remote execution failed for {}", item.command)
            } else {
                tail
            };
            let ingest = super::remote_control_workspace::RemoteQueueResultIngest {
                item: target.clone(),
                status: "failed".to_string(),
                summary: reason.clone(),
                endpoint_id: None,
                device_kind: None,
                device_label: None,
                transcript_path: item.transcript_path.clone(),
                result_id: None,
                source: Some("operator_fail".to_string()),
            };
            let outcome = ingest_remote_queue_result(&project_root, &ctx.session.session_id, &ingest)
                .map_err(|err| format!("Failed to ingest failed remote result: {}", err))?;
            if let Some(task_id) = item.runtime_task_id.as_deref() {
                if let Ok(engine) = ctx.engine.try_lock() {
                    engine.update_runtime_task_progress(task_id, reason.clone());
                    engine.mark_runtime_task_failed(task_id, reason.clone());
                }
            }
            return Ok(CommandOutput::Message(format!(
                "Remote queue item failed.\nItem: {}\nExecution: {}",
                outcome.item_id,
                outcome.execution_path.display()
            )));
        }

        if let ["retry", target] = parts.as_slice() {
            let (payload, _, index) = queue_item_target(&project_root, target)
                .map_err(|err| format!("Failed to resolve queue item: {}", err))?
                .ok_or_else(|| format!("Unknown queue target '{}'.", target))?;
            let item = payload.command_queue[index].clone();
            let _ = mark_remote_queue_item(
                &project_root,
                target,
                "queued",
                Some(format!("retry queued for {}", item.command)),
            )
            .map_err(|err| format!("Failed to queue retry: {}", err))?;
            if let Some(ref task_id) = item.runtime_task_id {
                let engine = ctx
                    .engine
                    .try_lock()
                    .map_err(|_| "Engine is busy, try again.".to_string())?;
                engine.mark_runtime_task_failed(&task_id, "re-queued for retry");
            }
            let _ = record_remote_transport_event(
                &project_root,
                &ctx.session.session_id,
                "retry",
                Some(item.id.as_str()),
                item.runtime_task_id.as_deref(),
                &format!("retry queued for {}", item.command),
            );
            return Ok(CommandOutput::Message(format!(
                "Remote queue item re-queued: {}",
                item.id
            )));
        }

        if let ["ack", target] = parts.as_slice() {
            let (payload, _, index) = queue_item_target(&project_root, target)
                .map_err(|err| format!("Failed to resolve queue item: {}", err))?
                .ok_or_else(|| format!("Unknown queue target '{}'.", target))?;
            let item = payload.command_queue[index].clone();
            let _ = mark_remote_queue_item(
                &project_root,
                target,
                "acked",
                Some("acknowledged".to_string()),
            )
            .map_err(|err| format!("Failed to acknowledge queue item: {}", err))?;
            let _ = record_remote_transport_event(
                &project_root,
                &ctx.session.session_id,
                "ack",
                Some(item.id.as_str()),
                item.runtime_task_id.as_deref(),
                "operator acknowledged remote queue result",
            );
            return Ok(CommandOutput::Message(format!(
                "Remote queue item acknowledged: {}",
                item.id
            )));
        }

        if trimmed == "queue" {
            let path = latest_remote_command_queue_artifact(&project_root)
                .ok_or_else(|| "No remote command queue artifact found.".to_string())?;
            let doc = open_artifact_inspector(
                "Remote command queue",
                &path,
                Some("/remote-control latest | /remote-control doctor".to_string()),
                vec![("kind".into(), "remote_queue".into())],
            )
            .ok_or_else(|| format!("Failed to open remote queue artifact {}.", path.display()))?;
            let mut doc = doc;
            attach_inspector_actions(
                &mut doc,
                vec![
                    ("latest".to_string(), "/remote-control latest".to_string()),
                    (
                        "transport".to_string(),
                        "/remote-control transport".to_string(),
                    ),
                    ("session".to_string(), "/remote-control session".to_string()),
                    (
                        "connect".to_string(),
                        "/remote-control transport connect".to_string(),
                    ),
                    ("run".to_string(), "/remote-control run latest".to_string()),
                    (
                        "dispatch".to_string(),
                        "/remote-control dispatch latest".to_string(),
                    ),
                    (
                        "complete".to_string(),
                        "/remote-control complete latest remote completion confirmed".to_string(),
                    ),
                    (
                        "fail".to_string(),
                        "/remote-control fail latest remote failure recorded".to_string(),
                    ),
                    (
                        "retry".to_string(),
                        "/remote-control retry latest".to_string(),
                    ),
                    ("ack".to_string(), "/remote-control ack latest".to_string()),
                    ("tasks".to_string(), "/remote-control tasks".to_string()),
                    ("monitor".to_string(), "/remote-control monitor".to_string()),
                    ("follow".to_string(), "/remote-control follow latest".to_string()),
                    ("bundle".to_string(), "/remote-control bundle".to_string()),
                ],
            );
            return Ok(CommandOutput::OpenInspector(doc));
        }

        if trimmed == "tasks" {
            let tasks = ctx
                .engine
                .try_lock()
                .ok()
                .map(|engine| engine.runtime_tasks_snapshot())
                .unwrap_or_default();
            return Ok(CommandOutput::Message(render_remote_task_inventory(&tasks)));
        }

        if let ["follow", id] = parts.as_slice() {
            let id = if *id == "latest" {
                ctx.engine
                    .try_lock()
                    .ok()
                    .and_then(|engine| {
                        engine
                            .runtime_tasks_snapshot()
                            .into_iter()
                            .max_by(|a, b| a.last_progress_at.cmp(&b.last_progress_at))
                            .map(|task| task.id)
                    })
                    .ok_or_else(|| "No runtime task available.".to_string())?
            } else {
                id.to_string()
            };
            ctx.input.set_text(&format!(
                "Use `task_output` with task_id=\"{}\", follow=true, and timeout_secs=120. Summarize final status, retries, artifact paths, and the most important output.",
                id
            ));
            return Ok(CommandOutput::Message(format!(
                "Loaded a remote task follow prompt for {}.",
                id
            )));
        }

        if trimmed == "retry-summary" {
            let tasks = ctx
                .engine
                .try_lock()
                .ok()
                .map(|engine| engine.runtime_tasks_snapshot())
                .unwrap_or_default();
            return Ok(CommandOutput::Message(render_remote_retry_summary(&tasks)));
        }

        if let ["handoff", id] = parts.as_slice() {
            let task = {
                let engine = ctx
                    .engine
                    .try_lock()
                    .map_err(|_| "Engine is busy, try again.".to_string())?;
                let task_id = if *id == "latest" {
                    engine
                        .runtime_tasks_snapshot()
                        .into_iter()
                        .max_by(|a, b| a.last_progress_at.cmp(&b.last_progress_at))
                        .map(|task| task.id)
                        .ok_or_else(|| "No runtime task available.".to_string())?
                } else {
                    id.to_string()
                };
                engine
                    .runtime_task_snapshot(&task_id)
                    .ok_or_else(|| format!("Task '{}' not found.", task_id))?
            };
            let path = write_remote_task_handoff_artifact(
                &project_root,
                &ctx.session.session_id,
                &task,
            )
            .map_err(|err| format!("Failed to write remote task handoff artifact: {}", err))?;
            return Ok(CommandOutput::Message(format!(
                "Remote task handoff written: {}",
                path.display()
            )));
        }

        if trimmed == "doctor" {
            let _ = write_remote_transport_artifacts(&project_root, &ctx.session.session_id);
            let _ = write_remote_live_session_artifacts(&project_root, &ctx.session.session_id);
            return Ok(CommandOutput::Message(render_remote_control_doctor(
                &project_root,
            )));
        }

        if trimmed == "bundle" {
            let bundle = export_remote_control_bundle(&project_root)
                .map_err(|err| format!("Failed to export remote control bundle: {}", err))?;
            return Ok(CommandOutput::Message(match bundle {
                Some(bundle) => format!(
                    "Remote control bundle exported to: {}\nInspect: /inspect artifact bundle",
                    bundle.display()
                ),
                None => "Remote control bundle unavailable: no remote control artifacts yet."
                    .to_string(),
            }));
        }

        Err("Usage: /remote-control [plan [goal]|latest|session [status|sync]|transport [status|connect|disconnect|reconnect]|queue|dispatch <item>|run <item>|ingest <file>|complete <item> [summary]|fail <item> [reason]|retry <item>|ack <item>|tasks|monitor|follow <id>|retry-summary|handoff <id>|doctor|bundle]".to_string())
    }
}

fn remote_control_footer(path: &std::path::Path) -> String {
    let mut lines = vec![
        "/remote-control transport".to_string(),
        "/remote-control transport connect".to_string(),
        "/remote-control session".to_string(),
        "/remote-control monitor".to_string(),
        "/remote-control dispatch latest".to_string(),
        "/remote-control queue".to_string(),
        "/remote-control follow latest".to_string(),
        "/remote-control complete latest remote completion confirmed".to_string(),
        "/remote-control fail latest remote failure recorded".to_string(),
        "/remote-control tasks".to_string(),
        "/tasks monitor".to_string(),
        "/remote-control retry-summary".to_string(),
        "/remote-control doctor".to_string(),
        "/remote-control bundle".to_string(),
        "/inspect artifact latest-remote-task-handoff".to_string(),
        "/inspect artifact latest-remote-control-state".to_string(),
        "/inspect artifact latest-remote-live-session-state".to_string(),
    ];
    if let Some(stale) = stale_artifact_actions(path, &lines) {
        lines.push(stale);
    }
    lines.join("\n")
}

fn open_remote_transport_inspector(project_root: &std::path::Path) -> CommandResult {
    let path = latest_remote_transport_artifact(project_root)
        .ok_or_else(|| "No remote transport artifacts found.".to_string())?;
    let doc = open_artifact_inspector(
        "Remote transport",
        &path,
        Some(
            "/remote-control transport connect | /remote-control transport reconnect | /inspect artifact latest-remote-transport-state"
                .to_string(),
        ),
        vec![("kind".into(), "remote_transport".into())],
    )
    .ok_or_else(|| format!("Failed to open remote transport artifact {}.", path.display()))?;
    let mut doc = doc;
    attach_inspector_actions(
        &mut doc,
        vec![
            (
                "connect".to_string(),
                "/remote-control transport connect".to_string(),
            ),
            (
                "reconnect".to_string(),
                "/remote-control transport reconnect".to_string(),
            ),
            (
                "disconnect".to_string(),
                "/remote-control transport disconnect".to_string(),
            ),
            (
                "events".to_string(),
                "/inspect artifact latest-remote-transport-events".to_string(),
            ),
            ("session".to_string(), "/remote-control session".to_string()),
            ("doctor".to_string(), "/remote-control doctor".to_string()),
            ("latest".to_string(), "/remote-control latest".to_string()),
        ],
    );
    Ok(CommandOutput::OpenInspector(doc))
}

fn open_remote_live_session_inspector(project_root: &std::path::Path) -> CommandResult {
    let path = latest_remote_live_session_artifact(project_root)
        .ok_or_else(|| "No remote live session artifacts found.".to_string())?;
    let doc = open_artifact_inspector(
        "Remote live session",
        &path,
        Some(
            "/remote-control session sync | /remote-control transport | /inspect artifact latest-remote-live-session-state"
                .to_string(),
        ),
        vec![("kind".into(), "remote_live_session".into())],
    )
    .ok_or_else(|| format!("Failed to open remote live session artifact {}.", path.display()))?;
    let mut doc = doc;
    attach_inspector_actions(
        &mut doc,
        vec![
            ("sync".to_string(), "/remote-control session sync".to_string()),
            ("transport".to_string(), "/remote-control transport".to_string()),
            ("queue".to_string(), "/remote-control queue".to_string()),
            ("monitor".to_string(), "/remote-control monitor".to_string()),
            ("follow".to_string(), "/remote-control follow latest".to_string()),
            (
                "state".to_string(),
                "/inspect artifact latest-remote-live-session-state".to_string(),
            ),
            ("doctor".to_string(), "/remote-control doctor".to_string()),
        ],
    );
    Ok(CommandOutput::OpenInspector(doc))
}

fn start_remote_transport_task(
    ctx: &mut CommandContext,
    project_root: &std::path::Path,
    description: &str,
) -> Result<(String, std::path::PathBuf), String> {
    let output_dir = project_root.join(".yode").join("tasks");
    let _ = std::fs::create_dir_all(&output_dir);
    let output_path = output_dir.join(format!("remote-transport-{}.log", uuid::Uuid::new_v4()));
    let task = {
        let engine = ctx
            .engine
            .try_lock()
            .map_err(|_| "Engine is busy, try again.".to_string())?;
        engine.create_runtime_task(
            "remote-transport",
            "remote-control",
            description,
            &output_path.display().to_string(),
            None,
        )
    }
    .ok_or_else(|| "Failed to allocate runtime task for remote transport.".to_string())?;
    {
        let engine = ctx
            .engine
            .try_lock()
            .map_err(|_| "Engine is busy, try again.".to_string())?;
        engine.mark_runtime_task_running(&task.id);
        engine.update_runtime_task_progress(&task.id, format!("starting {}", description));
    }
    Ok((task.id, output_path))
}

fn finish_remote_transport_task(
    ctx: &mut CommandContext,
    task_id: &str,
    error: Option<&str>,
    detail: &str,
) {
    if let Ok(engine) = ctx.engine.try_lock() {
        engine.update_runtime_task_progress(task_id, detail.to_string());
        if let Some(error) = error {
            engine.mark_runtime_task_failed(task_id, error.to_string());
        } else {
            engine.mark_runtime_task_completed(task_id);
        }
    }
}

fn probe_remote_transport(project_root: &std::path::Path) -> anyhow::Result<String> {
    let remote_dir = project_root.join(".yode").join("remote");
    std::fs::create_dir_all(&remote_dir)?;
    let probe = remote_dir.join(".transport-probe");
    std::fs::write(&probe, b"ok")?;
    std::fs::remove_file(&probe)?;
    let latest_session = latest_remote_control_artifact(project_root)
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "none".to_string());
    Ok(format!(
        "remote artifact dir writable at {} / latest remote control={}",
        remote_dir.display(),
        latest_session
    ))
}

fn start_remote_queue_task(
    ctx: &mut CommandContext,
    project_root: &std::path::Path,
    item: &super::remote_control_workspace::RemoteQueueItem,
    target: &str,
) -> Result<yode_tools::RuntimeTask, String> {
    let task = {
        let engine = ctx
            .engine
            .try_lock()
            .map_err(|_| "Engine is busy, try again.".to_string())?;
        let output_dir = project_root.join(".yode").join("tasks");
        let _ = std::fs::create_dir_all(&output_dir);
        let output_path = output_dir.join(format!("remote-queue-{}.log", uuid::Uuid::new_v4()));
        let transcript_path = item.transcript_path.clone().or_else(|| {
            queue_item_target(project_root, target)
                .ok()
                .flatten()
                .map(|_| yode_tools::runtime_tasks::latest_transcript_artifact_path(project_root))
                .flatten()
        });
        engine.create_runtime_task(
            "remote-control",
            "remote-control",
            &format!("queue {}", item.command),
            &output_path.display().to_string(),
            transcript_path,
        )
    }
    .ok_or_else(|| "Failed to allocate runtime task for remote queue item.".to_string())?;
    {
        let engine = ctx
            .engine
            .try_lock()
            .map_err(|_| "Engine is busy, try again.".to_string())?;
        engine.mark_runtime_task_running(&task.id);
        engine.update_runtime_task_progress(&task.id, format!("dispatching {}", item.command));
    }
    Ok(task)
}

fn parse_target_tail(input: &str, keyword: &str) -> Option<(String, String)> {
    let rest = input.trim().strip_prefix(keyword)?.trim();
    if rest.is_empty() {
        return None;
    }
    let mut parts = rest.split_whitespace();
    let target = parts.next()?.to_string();
    let tail = rest[target.len()..].trim().to_string();
    Some((target, tail))
}

fn transport_block_reason(
    payload: &super::remote_control_workspace::RemoteTransportPayload,
) -> String {
    match payload.connection_status.as_str() {
        "error" => format!(
            "transport error ({})",
            payload.last_error.as_deref().unwrap_or("unknown")
        ),
        "reconnecting" => "transport reconnecting".to_string(),
        "connected" => "transport connected".to_string(),
        status => format!("transport {}", status),
    }
}
