use crate::commands::context::CommandContext;
use crate::commands::{Command, CommandCategory, CommandMeta, CommandOutput, CommandResult};
use crate::commands::artifact_nav::{
    artifact_freshness_badge, latest_coordinator_artifact, latest_remote_control_artifact,
    latest_remote_task_handoff_artifact, latest_runtime_orchestration_artifact,
    latest_workflow_execution_artifact,
};
use crate::commands::info::runtime_inspectors::preview_runtime_artifact;
use crate::runtime_display::format_turn_artifact_status;
use crate::runtime_timeline::build_runtime_timeline_lines;
use super::artifact_preview::{compact_tool_runtime_summary, latest_markdown_file, preview_markdown};

pub struct BriefCommand {
    meta: CommandMeta,
}

impl BriefCommand {
    pub fn new() -> Self {
        Self {
            meta: CommandMeta {
                name: "brief",
                description: "Show a compact briefing of recent runtime state and artifacts",
                aliases: &[],
                args: vec![],
                category: CommandCategory::Info,
                hidden: false,
            },
        }
    }
}

impl Command for BriefCommand {
    fn meta(&self) -> &CommandMeta {
        &self.meta
    }

    fn execute(&self, _args: &str, ctx: &mut CommandContext) -> CommandResult {
        let runtime = ctx
            .engine
            .try_lock()
            .ok()
            .map(|engine| (engine.runtime_state(), engine.runtime_tasks_snapshot()));
        let Some((state, tasks)) = runtime else {
            return Ok(CommandOutput::Message(
                "Brief unavailable: engine busy.".to_string(),
            ));
        };

        let working_dir = std::path::PathBuf::from(&ctx.session.working_dir);
        let latest_review = latest_markdown_file(&working_dir.join(".yode").join("reviews"));
        let latest_transcript =
            latest_markdown_file(&working_dir.join(".yode").join("transcripts"));
        let latest_tool_artifact = state
            .last_tool_turn_artifact_path
            .as_ref()
            .map(std::path::PathBuf::from);
        let latest_workflow = latest_workflow_execution_artifact(&working_dir);
        let latest_coordinate = latest_coordinator_artifact(&working_dir);
        let latest_orchestration = latest_runtime_orchestration_artifact(&working_dir);
        let latest_remote_control = latest_remote_control_artifact(&working_dir);
        let latest_remote_handoff = latest_remote_task_handoff_artifact(&working_dir);
        let latest_review_preview = latest_review
            .as_ref()
            .and_then(|path| preview_markdown(path, "## Result"));
        let latest_transcript_preview = latest_transcript
            .as_ref()
            .and_then(|path| preview_markdown(path, "## Summary Anchor"));
        let latest_tool_preview = latest_tool_artifact
            .as_ref()
            .and_then(|path| preview_markdown(path, "## Calls"));
        let recovery_preview =
            preview_runtime_artifact(state.last_recovery_artifact_path.as_deref(), "## Breadcrumbs");
        let timeline_lines = build_runtime_timeline_lines(&state, &tasks, 4);
        let running_tasks = tasks
            .iter()
            .filter(|task| matches!(task.status, yode_tools::RuntimeTaskStatus::Running))
            .collect::<Vec<_>>();

        let mut output = String::from("Brief:\n");
        output.push_str(&format!(
            "  Compact:   {} total (last mode: {})\n",
            state.total_compactions,
            state.last_compaction_mode.as_deref().unwrap_or("none")
        ));
        output.push_str(&format!(
            "  Recovery:  {}{}\n",
            state.recovery_state,
            state
                .last_failed_signature
                .as_ref()
                .map(|sig| format!(" [{}]", sig))
                .unwrap_or_default()
        ));
        output.push_str(&format!(
            "  Tools:     {}\n",
            compact_tool_runtime_summary(&state)
        ));
        output.push_str(&format!("  Tasks:     {} running\n", running_tasks.len()));
        for task in running_tasks.iter().take(3) {
            output.push_str(&format!(
                "    - {} [{}] {}{}\n",
                task.id,
                task.kind,
                task.description,
                task.last_progress
                    .as_ref()
                    .map(|progress| format!(" — {}", progress))
                    .unwrap_or_default()
            ));
        }
        output.push_str("  Timeline:\n");
        for line in timeline_lines {
            output.push_str(&format!("    - {}\n", line));
        }
        output.push_str(&format!(
            "  Latest review: {}{}\n",
            latest_review
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "none".to_string()),
            latest_review_preview
                .as_ref()
                .map(|preview| format!("\n    {}", preview))
                .unwrap_or_default()
        ));
        output.push_str(&format!(
            "  Latest tool artifact: {}{}\n",
            latest_tool_artifact
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "none".to_string()),
            latest_tool_preview
                .as_ref()
                .map(|preview| format!("\n    {}", preview))
                .unwrap_or_default()
        ));
        output.push_str(&format!(
            "  Latest turn artifact: {}\n",
            format_turn_artifact_status(state.last_turn_artifact_path.as_deref())
        ));
        output.push_str(&format!(
            "  Latest transcript: {}{}\n",
            latest_transcript
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "none".to_string()),
            latest_transcript_preview
                .as_ref()
                .map(|preview| format!("\n    {}", preview))
                .unwrap_or_default()
        ));
        output.push_str(&format!(
            "  Recovery preview: {}\n",
            recovery_preview
        ));
        output.push_str("  Orchestration:\n");
        for (label, path, alias) in [
            ("workflow", latest_workflow.as_ref(), "/inspect artifact latest-workflow"),
            (
                "coordinate",
                latest_coordinate.as_ref(),
                "/inspect artifact latest-coordinate",
            ),
            (
                "timeline",
                latest_orchestration.as_ref(),
                "/inspect artifact latest-orchestration",
            ),
        ] {
            output.push_str(&format!(
                "    - {}: {}{}\n",
                label,
                path.map(|path| path.display().to_string())
                    .unwrap_or_else(|| "none".to_string()),
                path.map(|path| format!(" [{} | {}]", artifact_freshness_badge(path), alias))
                    .unwrap_or_default()
            ));
        }
        output.push_str(
            "    - states: /inspect artifact latest-workflow-state | /inspect artifact latest-coordinate-state\n",
        );
        output.push_str(
            "    - checkpoints: /checkpoint save [label] | /checkpoint latest | /checkpoint restore-dry-run latest\n",
        );
        output.push_str(
            "    - branch/rewind: /checkpoint branch save <name> | /checkpoint branch latest | /checkpoint rewind latest\n",
        );
        output.push_str(&format!(
            "    - remote-control: {}{}\n",
            latest_remote_control
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "none".to_string()),
            latest_remote_control
                .as_ref()
                .map(|path| format!(" [{} | /remote-control latest]", artifact_freshness_badge(path)))
                .unwrap_or_default()
        ));
        output.push_str(&format!(
            "    - remote-handoff: {}\n",
            latest_remote_handoff
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "none".to_string())
        ));
        output.push_str(
            "\nUse /diagnostics, /tasks, /reviews, /tools, /memory latest, or /inspect artifact summary for detail.",
        );

        Ok(CommandOutput::Message(output))
    }
}
