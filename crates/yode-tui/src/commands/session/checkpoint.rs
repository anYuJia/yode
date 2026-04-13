use crate::commands::artifact_nav::stale_artifact_actions;
use crate::commands::context::CommandContext;
use crate::commands::{
    ArgCompletionSource, ArgDef, Command, CommandCategory, CommandMeta, CommandOutput,
    CommandResult,
};

use super::checkpoint_workspace::{
    build_current_checkpoint_payload, checkpoint_completion_targets, checkpoint_operator_guide,
    render_checkpoint_diff, render_checkpoint_list, render_restore_dry_run,
    resolve_checkpoint_target, write_session_checkpoint,
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
                    hint: "[save [label]|list|latest|diff <a> <b>|restore-dry-run <target>]".to_string(),
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
            );
            return Ok(CommandOutput::Message(render_restore_dry_run(
                &current,
                &entry.payload,
                target,
            )));
        }

        Err("Usage: /checkpoint [save [label]|list|latest|<index>|<file>|diff <a> <b>|restore-dry-run <target>]".to_string())
    }
}

fn checkpoint_footer(path: &std::path::Path) -> String {
    let mut lines = vec![
        "/checkpoint list".to_string(),
        "/checkpoint diff latest latest-1".to_string(),
        "/checkpoint restore-dry-run latest".to_string(),
        "/inspect artifact latest-checkpoint-state".to_string(),
    ];
    if let Some(stale) = stale_artifact_actions(path, &lines) {
        lines.push(stale);
    }
    lines.push(checkpoint_operator_guide().to_string());
    lines.join("\n")
}
