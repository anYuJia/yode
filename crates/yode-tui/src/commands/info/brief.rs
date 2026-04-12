use crate::commands::context::CommandContext;
use crate::commands::{Command, CommandCategory, CommandMeta, CommandOutput, CommandResult};
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
        let latest_review_preview = latest_review
            .as_ref()
            .and_then(|path| preview_markdown(path, "## Result"));
        let latest_transcript_preview = latest_transcript
            .as_ref()
            .and_then(|path| preview_markdown(path, "## Summary Anchor"));
        let latest_tool_preview = latest_tool_artifact
            .as_ref()
            .and_then(|path| preview_markdown(path, "## Calls"));
        let running_tasks = tasks
            .into_iter()
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
        output.push_str(
            "\nUse /diagnostics, /tasks, /reviews, /tools, or /memory latest for detail.",
        );

        Ok(CommandOutput::Message(output))
    }
}
