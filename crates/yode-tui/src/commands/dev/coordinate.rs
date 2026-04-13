use crate::commands::context::CommandContext;
use crate::commands::artifact_nav::{
    artifact_freshness_badge, latest_coordinator_artifact, open_artifact_inspector,
    recent_artifacts_by_suffix, stale_artifact_actions, write_runtime_orchestration_timeline_artifact,
};
use crate::commands::{
    ArgCompletionSource, ArgDef, Command, CommandCategory, CommandMeta, CommandOutput,
    CommandResult,
};
use super::coordinate_workspace::{
    coordinator_dry_run_prompt, coordinator_jump_targets, write_coordinator_stub_artifact,
    write_coordinator_summary_artifact,
};

pub struct CoordinateCommand {
    meta: CommandMeta,
}

impl CoordinateCommand {
    pub fn new() -> Self {
        Self {
            meta: CommandMeta {
                name: "coordinate",
                description: "Prefill a coordinate_agents prompt into the input box",
                aliases: &[],
                args: vec![ArgDef {
                    name: "goal".to_string(),
                    required: false,
                    hint: "[goal|latest|summary|history|timeline]".to_string(),
                    completions: ArgCompletionSource::None,
                }],
                category: CommandCategory::Development,
                hidden: false,
            },
        }
    }
}

impl Command for CoordinateCommand {
    fn meta(&self) -> &CommandMeta {
        &self.meta
    }

    fn execute(&self, args: &str, ctx: &mut CommandContext) -> CommandResult {
        let trimmed = args.trim();
        let project_root = std::path::PathBuf::from(&ctx.session.working_dir);

        if matches!(trimmed, "latest" | "summary") {
            let path = latest_coordinator_artifact(&project_root)
                .ok_or_else(|| "No coordinator artifacts found.".to_string())?;
            let footer = coordinate_artifact_footer(&path);
            let doc = open_artifact_inspector(
                "Coordinator inspector",
                &path,
                Some(footer),
                vec![("kind".into(), "coordinate".into())],
            )
            .ok_or_else(|| format!("Failed to open coordinator artifact {}.", path.display()))?;
            return Ok(CommandOutput::OpenInspector(doc));
        }

        if trimmed == "history" {
            let artifacts = recent_artifacts_by_suffix(
                &project_root.join(".yode").join("status"),
                "coordinate-summary.md",
                6,
            );
            if artifacts.is_empty() {
                return Ok(CommandOutput::Message(
                    "No coordinator summary artifacts found.".to_string(),
                ));
            }
            let mut output = String::from("Coordinator artifact history:\n");
            for path in artifacts {
                output.push_str(&format!(
                    "  - [{}] {}\n",
                    artifact_freshness_badge(&path),
                    path.display()
                ));
            }
            output.push_str("\nUse `/coordinate latest` or `/inspect artifact latest-coordinate`.");
            return Ok(CommandOutput::Message(output));
        }

        if trimmed == "timeline" {
            let path = write_runtime_orchestration_timeline_artifact(
                &project_root,
                &ctx.session.session_id,
            )
            .ok_or_else(|| "Failed to write runtime orchestration timeline.".to_string())?;
            let doc = open_artifact_inspector(
                "Runtime orchestration timeline",
                std::path::Path::new(&path),
                Some("/coordinate latest | /inspect workflows latest".to_string()),
                vec![("kind".into(), "orchestration".into())],
            )
            .ok_or_else(|| format!("Failed to open timeline artifact {}.", path))?;
            return Ok(CommandOutput::OpenInspector(doc));
        }

        let goal = if trimmed.is_empty() {
            "complete the current task via multiple independent workstreams".to_string()
        } else {
            trimmed.to_string()
        };
        ctx.input.set_text(&coordinator_dry_run_prompt(&goal));
        let artifact = write_coordinator_stub_artifact(
            &project_root,
            &ctx.session.session_id,
            &goal,
        );
        let timeline =
            write_runtime_orchestration_timeline_artifact(&project_root, &ctx.session.session_id);
        let summary = write_coordinator_summary_artifact(
            &project_root,
            &ctx.session.session_id,
            &goal,
            artifact.as_deref(),
            timeline.as_deref(),
        );
        Ok(CommandOutput::Message(format!(
            "Loaded a coordinator-agent prompt into the input box.\nDry run: {}\nSummary: {}\nTimeline: {}",
            artifact.unwrap_or_else(|| "none".to_string()),
            summary.unwrap_or_else(|| "none".to_string()),
            timeline.unwrap_or_else(|| "none".to_string()),
        )))
    }
}

fn coordinate_artifact_footer(path: &std::path::Path) -> String {
    let mut lines = coordinator_jump_targets();
    lines.push("/coordinate history".to_string());
    if let Some(stale) = stale_artifact_actions(path, &lines) {
        lines.push(stale);
    }
    lines.join("\n")
}
