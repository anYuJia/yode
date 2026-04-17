use crate::commands::artifact_nav::{attach_inspector_actions, stale_artifact_actions};
use crate::commands::context::CommandContext;
use crate::commands::{
    ArgCompletionSource, ArgDef, Command, CommandCategory, CommandMeta, CommandOutput,
    CommandResult,
};

use super::checkpoint_workspace::{
    build_current_checkpoint_payload, checkpoint_completion_targets, checkpoint_operator_guide,
    checkpoint_restore_chat_entries, checkpoint_restore_messages, render_branch_list,
    render_branch_merge_preview, render_checkpoint_diff, render_checkpoint_list,
    render_restore_dry_run, render_rewind_anchor_list, render_rewind_safety_summary,
    render_rollback_anchor_list, render_rollback_preview, merge_checkpoint_payloads,
    resolve_branch_target, resolve_checkpoint_target, resolve_rewind_anchor_target,
    resolve_rollback_anchor_target, write_branch_merge_execution_artifact,
    write_branch_merge_preview, write_branch_snapshot, write_rewind_anchor,
    write_merge_rollback_anchor, write_restore_rollback_anchor, write_session_checkpoint,
};

pub struct CheckpointCommand {
    meta: CommandMeta,
}

impl CheckpointCommand {
    pub fn new() -> Self {
        Self {
            meta: CommandMeta {
                name: "checkpoint",
                description: "Save, inspect, diff, or preview restoring session checkpoints",
                aliases: &[],
                args: vec![ArgDef {
                    name: "action".to_string(),
                    required: false,
                    hint: "[save [label]|list|latest|diff <a> <b>|restore-dry-run <target>|branch ...|rewind ...]".to_string(),
                    completions: ArgCompletionSource::Dynamic(|ctx| {
                        checkpoint_completion_targets(ctx.working_dir)
                    }),
                }],
                category: CommandCategory::Session,
                hidden: false,
            },
        }
    }
}

impl Command for CheckpointCommand {
    fn meta(&self) -> &CommandMeta {
        &self.meta
    }

    fn execute(&self, args: &str, ctx: &mut CommandContext) -> CommandResult {
        let project_root = std::path::PathBuf::from(&ctx.session.working_dir);
        let trimmed = args.trim();
        let parts = trimmed.split_whitespace().collect::<Vec<_>>();
        let engine_snapshot = || -> Vec<yode_llm::types::Message> {
            ctx.engine
                .try_lock()
                .ok()
                .map(|engine| engine.messages().iter().skip(1).cloned().collect())
                .unwrap_or_default()
        };

        if let ["branch", "list"] = parts.as_slice() {
            return Ok(CommandOutput::Message(render_branch_list(&project_root)));
        }
        if matches!(parts.as_slice(), ["branch", "latest"] | ["branch", _]) {
            let target = parts.get(1).copied().unwrap_or("latest");
            if target != "save" && target != "diff" && target != "list" {
                let entry = resolve_branch_target(&project_root, target)
                    .ok_or_else(|| "No session branches found.".to_string())?;
                let footer = checkpoint_footer(&entry.summary_path);
                let doc = crate::commands::artifact_nav::open_artifact_inspector(
                    "Session branch inspector",
                    &entry.summary_path,
                    Some(footer),
                    vec![("kind".into(), "branch".into())],
                )
                .ok_or_else(|| format!("Failed to open branch {}.", entry.summary_path.display()))?;
                let mut doc = doc;
                attach_inspector_actions(
                    &mut doc,
                    vec![
                        ("diff".to_string(), "/checkpoint branch diff latest latest-1".to_string()),
                        ("rewind".to_string(), "/checkpoint rewind latest".to_string()),
                    ],
                );
                return Ok(CommandOutput::OpenInspector(doc));
            }
        }
        if let ["branch", "save", name] = parts.as_slice() {
            let current = build_current_checkpoint_payload(
                &project_root,
                &ctx.session.session_id,
                ctx.provider_name.as_str(),
                &ctx.session.model,
                "current session",
                ctx.chat_entries,
                &engine_snapshot(),
            );
            let artifacts = write_branch_snapshot(&project_root, name, &current, None)
                .map_err(|err| format!("Failed to write branch snapshot: {}", err))?;
            return Ok(CommandOutput::Message(format!(
                "Saved session branch.\nSummary: {}\nState: {}",
                artifacts.summary_path.display(),
                artifacts.state_path.display(),
            )));
        }
        if let ["branch", "diff", left, right] = parts.as_slice() {
            let left_entry = resolve_branch_target(&project_root, left)
                .ok_or_else(|| format!("Unknown branch target '{}'.", left))?;
            let right_entry = resolve_branch_target(&project_root, right)
                .ok_or_else(|| format!("Unknown branch target '{}'.", right))?;
            return Ok(CommandOutput::Message(render_checkpoint_diff(
                &left_entry.payload,
                &right_entry.payload,
                left,
                right,
            )));
        }
        if let ["branch", "merge-dry-run", target] = parts.as_slice() {
            let current = build_current_checkpoint_payload(
                &project_root,
                &ctx.session.session_id,
                ctx.provider_name.as_str(),
                &ctx.session.model,
                "current session",
                ctx.chat_entries,
                &engine_snapshot(),
            );
            let branch = resolve_branch_target(&project_root, target)
                .ok_or_else(|| format!("Unknown branch target '{}'.", target))?;
            let artifacts = write_branch_merge_preview(
                &project_root,
                &current,
                &branch.payload,
                target,
            )
            .map_err(|err| format!("Failed to write branch merge preview: {}", err))?;
            let preview =
                crate::commands::session::checkpoint_workspace::load_branch_merge_preview(&artifacts.state_path)
                    .map_err(|err| format!("Failed to load branch merge preview: {}", err))?;
            return Ok(CommandOutput::Message(render_branch_merge_preview(
                &preview,
                &artifacts.state_path,
            )));
        }
        if let ["branch", "merge", target] = parts.as_slice() {
            let current = build_current_checkpoint_payload(
                &project_root,
                &ctx.session.session_id,
                ctx.provider_name.as_str(),
                &ctx.session.model,
                "current session",
                ctx.chat_entries,
                &engine_snapshot(),
            );
            let branch = resolve_branch_target(&project_root, target)
                .ok_or_else(|| format!("Unknown branch target '{}'.", target))?;
            let _ = write_merge_rollback_anchor(&project_root, &current, target)
                .map_err(|err| format!("Failed to write merge rollback anchor: {}", err))?;
            let (merged_messages, merged_chat_entries) =
                merge_checkpoint_payloads(&current, &branch.payload);
            {
                let mut engine = ctx
                    .engine
                    .try_lock()
                    .map_err(|_| "Engine is busy, try again.".to_string())?;
                engine.restore_and_persist_messages(merged_messages);
            }
            *ctx.chat_entries = merged_chat_entries;
            ctx.push_system_message(format!(
                "Merged branch '{}' into the current session.",
                target
            ));
            let artifacts = write_branch_merge_execution_artifact(
                &project_root,
                &current,
                &branch.payload,
                target,
            )
            .map_err(|err| format!("Failed to write branch merge execution artifact: {}", err))?;
            return Ok(CommandOutput::Message(format!(
                "Branch merged into current session.\nSummary: {}\nState: {}",
                artifacts.summary_path.display(),
                artifacts.state_path.display(),
            )));
        }
        if let ["rollback", "list"] = parts.as_slice() {
            return Ok(CommandOutput::Message(render_rollback_anchor_list(&project_root)));
        }
        if matches!(parts.as_slice(), ["rollback", "latest"] | ["rollback", _]) {
            let target = parts.get(1).copied().unwrap_or("latest");
            if target != "dry-run" && target != "list" {
                if let Some(entry) = resolve_rollback_anchor_target(&project_root, target) {
                    let footer = checkpoint_footer(&entry.summary_path);
                    let doc = crate::commands::artifact_nav::open_artifact_inspector(
                        "Rollback anchor inspector",
                        &entry.summary_path,
                        Some(footer),
                        vec![("kind".into(), "rollback".into())],
                    )
                    .ok_or_else(|| {
                        format!("Failed to open rollback anchor {}.", entry.summary_path.display())
                    })?;
                    return Ok(CommandOutput::OpenInspector(doc));
                }
            }
        }
        if let ["rollback-dry-run", target] = parts.as_slice() {
            let entry = resolve_rollback_anchor_target(&project_root, target)
                .ok_or_else(|| format!("Unknown rollback target '{}'.", target))?;
            let current = build_current_checkpoint_payload(
                &project_root,
                &ctx.session.session_id,
                ctx.provider_name.as_str(),
                &ctx.session.model,
                "current session",
                ctx.chat_entries,
                &engine_snapshot(),
            );
            return Ok(CommandOutput::Message(render_rollback_preview(
                &current,
                &entry.payload,
                target,
            )));
        }
        if let ["rewind-anchor"] | ["rewind-anchor", "list"] = parts.as_slice() {
            return Ok(CommandOutput::Message(render_rewind_anchor_list(&project_root)));
        }
        if let ["rewind-anchor", "latest"] | ["rewind-anchor", _] = parts.as_slice() {
            let target = parts.get(1).copied().unwrap_or("latest");
            if target != "save" && target != "list" {
                if let Some(entry) = resolve_rewind_anchor_target(&project_root, target) {
                    let footer = checkpoint_footer(&entry.summary_path);
                    let doc = crate::commands::artifact_nav::open_artifact_inspector(
                        "Rewind anchor inspector",
                        &entry.summary_path,
                        Some(footer),
                        vec![("kind".into(), "rewind".into())],
                    )
                    .ok_or_else(|| {
                        format!("Failed to open rewind anchor {}.", entry.summary_path.display())
                    })?;
                    let mut doc = doc;
                    attach_inspector_actions(
                        &mut doc,
                        vec![
                            ("rewind".to_string(), "/checkpoint rewind latest".to_string()),
                            ("checkpoint".to_string(), "/checkpoint latest".to_string()),
                        ],
                    );
                    return Ok(CommandOutput::OpenInspector(doc));
                }
            }
        }
        if let ["rewind-anchor", "save", target] = parts.as_slice() {
            let current = build_current_checkpoint_payload(
                &project_root,
                &ctx.session.session_id,
                ctx.provider_name.as_str(),
                &ctx.session.model,
                "current session",
                ctx.chat_entries,
                &engine_snapshot(),
            );
            let entry = resolve_checkpoint_target(&project_root, target)
                .ok_or_else(|| format!("Unknown checkpoint target '{}'.", target))?;
            let artifacts = write_rewind_anchor(&project_root, &current, &entry.payload, target)
                .map_err(|err| format!("Failed to write rewind anchor: {}", err))?;
            return Ok(CommandOutput::Message(format!(
                "Saved rewind anchor.\nSummary: {}\nState: {}",
                artifacts.summary_path.display(),
                artifacts.state_path.display(),
            )));
        }
        if let ["rewind", target] = parts.as_slice() {
            let current = build_current_checkpoint_payload(
                &project_root,
                &ctx.session.session_id,
                ctx.provider_name.as_str(),
                &ctx.session.model,
                "current session",
                ctx.chat_entries,
                &engine_snapshot(),
            );
            let entry = resolve_checkpoint_target(&project_root, target)
                .ok_or_else(|| format!("Unknown checkpoint target '{}'.", target))?;
            let anchor = write_rewind_anchor(&project_root, &current, &entry.payload, target)
                .map_err(|err| format!("Failed to write rewind anchor: {}", err))?;
            return Ok(CommandOutput::Message(render_rewind_safety_summary(
                &current,
                &entry.payload,
                target,
                Some(&anchor.summary_path),
            )));
        }

        if trimmed.is_empty() || parts.first() == Some(&"save") {
            let label = parts
                .get(1..)
                .map(|parts| parts.join(" "))
                .filter(|label| !label.trim().is_empty())
                .unwrap_or_else(|| "manual checkpoint".to_string());
            let artifacts = write_session_checkpoint(
                &project_root,
                &ctx.session.session_id,
                ctx.provider_name.as_str(),
                &ctx.session.model,
                &label,
                ctx.chat_entries,
                &engine_snapshot(),
            )
            .map_err(|err| format!("Failed to write checkpoint: {}", err))?;
            return Ok(CommandOutput::Message(format!(
                "Saved session checkpoint.\nSummary: {}\nState: {}\n{}",
                artifacts.summary_path.display(),
                artifacts.state_path.display(),
                checkpoint_operator_guide(),
            )));
        }

        if trimmed == "list" {
            return Ok(CommandOutput::Message(render_checkpoint_list(&project_root)));
        }

        if matches!(parts.first(), Some(&"latest")) || parts.len() == 1 {
            let entry = resolve_checkpoint_target(&project_root, trimmed)
                .ok_or_else(|| "No session checkpoints found.".to_string())?;
            let footer = checkpoint_footer(&entry.summary_path);
            let doc = crate::commands::artifact_nav::open_artifact_inspector(
                "Session checkpoint inspector",
                &entry.summary_path,
                Some(footer),
                vec![("kind".into(), "checkpoint".into())],
            )
            .ok_or_else(|| format!("Failed to open checkpoint {}.", entry.summary_path.display()))?;
            let mut doc = doc;
            attach_inspector_actions(
                &mut doc,
                vec![
                    ("diff".to_string(), "/checkpoint diff latest latest-1".to_string()),
                    ("branch".to_string(), "/checkpoint branch save workstream-a".to_string()),
                    ("restore".to_string(), "/checkpoint restore-dry-run latest".to_string()),
                ],
            );
            return Ok(CommandOutput::OpenInspector(doc));
        }

        if let ["diff", left, right] = parts.as_slice() {
            let left_entry = resolve_checkpoint_target(&project_root, left)
                .ok_or_else(|| format!("Unknown checkpoint target '{}'.", left))?;
            let right_entry = resolve_checkpoint_target(&project_root, right)
                .ok_or_else(|| format!("Unknown checkpoint target '{}'.", right))?;
            return Ok(CommandOutput::Message(render_checkpoint_diff(
                &left_entry.payload,
                &right_entry.payload,
                left,
                right,
            )));
        }

        if let ["restore-dry-run", target] = parts.as_slice() {
            let entry = resolve_checkpoint_target(&project_root, target)
                .ok_or_else(|| format!("Unknown checkpoint target '{}'.", target))?;
            let current = build_current_checkpoint_payload(
                &project_root,
                &ctx.session.session_id,
                ctx.provider_name.as_str(),
                &ctx.session.model,
                "current session",
                ctx.chat_entries,
                &engine_snapshot(),
            );
            return Ok(CommandOutput::Message(render_restore_dry_run(
                &current,
                &entry.payload,
                target,
            )));
        }

        if let ["restore", target] = parts.as_slice() {
            let entry = resolve_checkpoint_target(&project_root, target)
                .ok_or_else(|| format!("Unknown checkpoint target '{}'.", target))?;
            let current = build_current_checkpoint_payload(
                &project_root,
                &ctx.session.session_id,
                ctx.provider_name.as_str(),
                &ctx.session.model,
                "current session",
                ctx.chat_entries,
                &engine_snapshot(),
            );
            let _ = write_restore_rollback_anchor(&project_root, &current, target)
                .map_err(|err| format!("Failed to write restore rollback anchor: {}", err))?;
            let restored_messages = checkpoint_restore_messages(&entry.payload);
            {
                let mut engine = ctx
                    .engine
                    .try_lock()
                    .map_err(|_| "Engine is busy, try again.".to_string())?;
                engine.restore_and_persist_messages(restored_messages);
            }
            *ctx.chat_entries = checkpoint_restore_chat_entries(&entry.payload);
            ctx.push_system_message(format!(
                "Session restored from checkpoint '{}'.",
                entry.payload.label
            ));
            return Ok(CommandOutput::Message(format!(
                "Restored session from checkpoint '{}'.",
                entry.payload.label
            )));
        }

        Err("Usage: /checkpoint [save [label]|list|latest|<index>|<file>|diff <a> <b>|restore <target>|restore-dry-run <target>|rollback [list|latest]|rollback-dry-run <target>|branch [list|latest|save <name>|diff <a> <b>|merge-dry-run <target>|merge <target>]|rewind-anchor [list|latest|save <target>]|rewind <target>]".to_string())
    }
}

fn checkpoint_footer(path: &std::path::Path) -> String {
    let mut lines = vec![
        "/checkpoint list".to_string(),
        "/checkpoint branch list".to_string(),
        "/checkpoint branch merge-dry-run latest".to_string(),
        "/checkpoint branch merge latest".to_string(),
        "/checkpoint rollback latest".to_string(),
        "/checkpoint rollback-dry-run latest".to_string(),
        "/checkpoint diff latest latest-1".to_string(),
        "/checkpoint rewind latest".to_string(),
        "/checkpoint restore-dry-run latest".to_string(),
        "/inspect artifact latest-checkpoint-state".to_string(),
    ];
    if let Some(stale) = stale_artifact_actions(path, &lines) {
        lines.push(stale);
    }
    lines.push(checkpoint_operator_guide().to_string());
    lines.join("\n")
}
