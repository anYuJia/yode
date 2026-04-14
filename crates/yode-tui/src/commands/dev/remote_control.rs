use crate::commands::artifact_nav::{
    attach_inspector_actions, open_artifact_inspector, stale_artifact_actions,
};
use crate::commands::context::CommandContext;
use crate::commands::{
    ArgCompletionSource, ArgDef, Command, CommandCategory, CommandMeta, CommandOutput,
    CommandResult,
};

use super::remote_control_workspace::{
    bind_remote_queue_item_runtime, export_remote_control_bundle,
    latest_remote_command_queue_artifact, latest_remote_control_artifact,
    mark_remote_queue_item, queue_item_target,
    render_remote_control_doctor, render_remote_retry_summary, render_remote_task_inventory,
    write_remote_control_artifacts, write_remote_queue_execution_artifact,
    write_remote_task_handoff_artifact,
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
                    hint: "[plan [goal]|latest|queue|run <item>|retry <item>|ack <item>|tasks|follow <id>|retry-summary|handoff <id>|doctor|bundle]".to_string(),
                    completions: ArgCompletionSource::Static(vec![
                        "plan".to_string(),
                        "latest".to_string(),
                        "queue".to_string(),
                        "run".to_string(),
                        "retry".to_string(),
                        "ack".to_string(),
                        "tasks".to_string(),
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
                    ("queue".to_string(), "/remote-control queue".to_string()),
                    ("tasks".to_string(), "/remote-control tasks".to_string()),
                    ("doctor".to_string(), "/remote-control doctor".to_string()),
                ],
            );
            return Ok(CommandOutput::OpenInspector(doc));
        }

        if let ["run", target] = parts.as_slice() {
            let (payload, _, index) = queue_item_target(&project_root, target)
                .map_err(|err| format!("Failed to resolve queue item: {}", err))?
                .ok_or_else(|| format!("Unknown queue target '{}'.", target))?;
            let item = payload.command_queue[index].clone();
            let runtime_task = {
                let engine = ctx
                    .engine
                    .try_lock()
                    .map_err(|_| "Engine is busy, try again.".to_string())?;
                let output_dir = project_root.join(".yode").join("tasks");
                let _ = std::fs::create_dir_all(&output_dir);
                let output_path = output_dir.join(format!("remote-queue-{}.log", uuid::Uuid::new_v4()));
                let transcript_path = item.transcript_path.clone().or_else(|| {
                    queue_item_target(&project_root, target)
                        .ok()
                        .flatten()
                        .map(|_| yode_tools::runtime_tasks::latest_transcript_artifact_path(&project_root))
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
                engine.mark_runtime_task_running(&runtime_task.id);
                engine.update_runtime_task_progress(
                    &runtime_task.id,
                    format!("dispatching {}", item.command),
                );
            }
            let _ = mark_remote_queue_item(&project_root, target, "running", None)
                .map_err(|err| format!("Failed to mark queue item running: {}", err))?;
            let _ = bind_remote_queue_item_runtime(
                &project_root,
                target,
                Some(runtime_task.id.clone()),
                runtime_task.transcript_path.clone(),
                None,
            )
            .map_err(|err| format!("Failed to bind queue runtime task: {}", err))?;
            let command = item.command.trim_start_matches('/').trim();
            let (cmd_name, cmd_args) = match command.find(' ') {
                Some(pos) => (&command[..pos], command[pos + 1..].trim()),
                None => (command, ""),
            };
            let output = ctx
                .cmd_registry
                .execute_command(cmd_name, cmd_args, ctx)
                .ok_or_else(|| format!("Command '{}' not found.", cmd_name))??;
            let preview = match &output {
                CommandOutput::Message(body) => body.lines().take(3).collect::<Vec<_>>().join(" | "),
                CommandOutput::Messages(lines) => lines.iter().take(3).cloned().collect::<Vec<_>>().join(" | "),
                CommandOutput::OpenInspector(doc) => doc
                    .active_panel()
                    .map(|panel| panel.lines.iter().take(3).cloned().collect::<Vec<_>>().join(" | "))
                    .unwrap_or_else(|| "inspector".to_string()),
                CommandOutput::Silent => "silent".to_string(),
                CommandOutput::StartWizard(_) => "wizard".to_string(),
                CommandOutput::ReloadProvider { .. } => "reload-provider".to_string(),
            };
            let (payload, _) = mark_remote_queue_item(
                &project_root,
                target,
                "completed",
                Some(preview.clone()),
            )
            .map_err(|err| format!("Failed to mark queue item completed: {}", err))?
            .ok_or_else(|| format!("Unknown queue target '{}'.", target))?;
            let item = payload.command_queue[index].clone();
            let execution = write_remote_queue_execution_artifact(&project_root, &item, &preview)
                .map_err(|err| format!("Failed to write remote queue execution artifact: {}", err))?;
            {
                let engine = ctx
                    .engine
                    .try_lock()
                    .map_err(|_| "Engine is busy, try again.".to_string())?;
                engine.update_runtime_task_progress(&runtime_task.id, preview.clone());
                engine.mark_runtime_task_completed(&runtime_task.id);
            }
            let _ = bind_remote_queue_item_runtime(
                &project_root,
                target,
                Some(runtime_task.id.clone()),
                runtime_task.transcript_path.clone(),
                Some(execution.display().to_string()),
            )
            .map_err(|err| format!("Failed to update queue execution artifact: {}", err))?;
            return Ok(CommandOutput::Message(format!(
                "Remote queue item executed.\nItem: {}\nTask: {}\nExecution: {}",
                item.id,
                runtime_task.id,
                execution.display()
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
            if let Some(task_id) = item.runtime_task_id {
                let engine = ctx
                    .engine
                    .try_lock()
                    .map_err(|_| "Engine is busy, try again.".to_string())?;
                engine.mark_runtime_task_failed(&task_id, "re-queued for retry");
            }
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
                    ("run".to_string(), "/remote-control run latest".to_string()),
                    ("retry".to_string(), "/remote-control retry latest".to_string()),
                    ("ack".to_string(), "/remote-control ack latest".to_string()),
                    ("tasks".to_string(), "/remote-control tasks".to_string()),
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
            return Ok(CommandOutput::Message(render_remote_control_doctor(&project_root)));
        }

        if trimmed == "bundle" {
            let bundle = export_remote_control_bundle(&project_root)
                .map_err(|err| format!("Failed to export remote control bundle: {}", err))?;
            return Ok(CommandOutput::Message(match bundle {
                Some(bundle) => format!(
                    "Remote control bundle exported to: {}\nInspect: /inspect artifact bundle",
                    bundle.display()
                ),
                None => "Remote control bundle unavailable: no remote control artifacts yet.".to_string(),
            }));
        }

        Err("Usage: /remote-control [plan [goal]|latest|queue|run <item>|retry <item>|ack <item>|tasks|follow <id>|retry-summary|handoff <id>|doctor|bundle]".to_string())
    }
}

fn remote_control_footer(path: &std::path::Path) -> String {
    let mut lines = vec![
        "/remote-control queue".to_string(),
        "/remote-control tasks".to_string(),
        "/remote-control retry-summary".to_string(),
        "/remote-control doctor".to_string(),
        "/remote-control bundle".to_string(),
        "/inspect artifact latest-remote-task-handoff".to_string(),
        "/inspect artifact latest-remote-control-state".to_string(),
    ];
    if let Some(stale) = stale_artifact_actions(path, &lines) {
        lines.push(stale);
    }
    lines.join("\n")
}
