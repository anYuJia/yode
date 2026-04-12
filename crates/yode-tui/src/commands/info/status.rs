mod helpers;
mod render;
mod sections;

use crate::commands::context::CommandContext;
use crate::commands::{Command, CommandCategory, CommandMeta, CommandOutput, CommandResult};
use crate::runtime_artifacts::write_runtime_task_inventory_artifact;

use super::startup_artifacts::{
    latest_mcp_startup_failures, latest_provider_inventory, latest_startup_artifact_link,
    latest_startup_manifest,
};
use self::helpers::latest_review_summary;
use self::render::{build_provider_section, build_runtime_sections, build_status_message};
use self::sections::StatusArtifactLinks;
use super::cost::estimate_cost;

pub struct StatusCommand {
    meta: CommandMeta,
}

impl StatusCommand {
    pub fn new() -> Self {
        Self {
            meta: CommandMeta {
                name: "status",
                description: "Show session status",
                aliases: &[],
                args: vec![],
                category: CommandCategory::Info,
                hidden: false,
            },
        }
    }
}

impl Command for StatusCommand {
    fn meta(&self) -> &CommandMeta {
        &self.meta
    }

    fn execute(&self, _args: &str, ctx: &mut CommandContext) -> CommandResult {
        let always_allow = if ctx.session.always_allow_tools.is_empty() {
            "none".to_string()
        } else {
            ctx.session.always_allow_tools.join(", ")
        };
        let cost = estimate_cost(
            &ctx.session.model,
            ctx.session.input_tokens,
            ctx.session.output_tokens,
        );
        let working_dir = std::path::PathBuf::from(&ctx.session.working_dir);
        let latest_review = latest_review_summary(&working_dir.join(".yode").join("reviews"));
        let resume_warmup = ctx
            .session
            .resume_cache_warmup
            .as_ref()
            .map(|stats| {
                format!(
                    "{} transcripts / {} metadata / latest={} / {} ms",
                    stats.transcript_count,
                    stats.metadata_entries_warmed,
                    if stats.latest_lookup_cached {
                        "yes"
                    } else {
                        "no"
                    },
                    stats.duration_ms
                )
            })
            .unwrap_or_else(|| "none".to_string());
        let startup_profile = ctx.session.startup_profile.as_deref().unwrap_or("none");
        let inventory = ctx.tools.inventory();
        let provider_inventory = latest_provider_inventory(&working_dir);
        let startup_manifest = latest_startup_manifest(&working_dir);
        let mcp_startup_failures = latest_mcp_startup_failures(&working_dir);
        let runtime = ctx
            .engine
            .try_lock()
            .ok()
            .map(|engine| engine.runtime_state());
        let runtime_task_artifact = write_runtime_task_inventory_artifact(
            &working_dir,
            &ctx.session.session_id,
            ctx.engine
                .try_lock()
                .ok()
                .map(|engine| engine.runtime_tasks_snapshot())
                .unwrap_or_default(),
        );
        let artifact_links = StatusArtifactLinks {
            review_artifact: latest_review
                .as_ref()
                .map(|summary| summary.path.display().to_string()),
            startup_profile_artifact: latest_startup_artifact_link(&working_dir, "startup-profile.txt"),
            startup_manifest_artifact: startup_manifest
                .as_ref()
                .map(|summary| summary.path.display().to_string()),
            provider_inventory_artifact: provider_inventory
                .as_ref()
                .map(|summary| summary.path.display().to_string()),
            resume_warmup_artifact: latest_startup_artifact_link(&working_dir, "resume-warmup.json"),
            mcp_failure_artifact: mcp_startup_failures
                .as_ref()
                .map(|summary| summary.path.display().to_string()),
            tool_artifact: runtime
                .as_ref()
                .and_then(|state| state.last_tool_turn_artifact_path.clone()),
            recovery_artifact: runtime
                .as_ref()
                .and_then(|state| state.last_recovery_artifact_path.clone()),
            permission_artifact: runtime
                .as_ref()
                .and_then(|state| state.last_permission_artifact_path.clone()),
            transcript_artifact: runtime
                .as_ref()
                .and_then(|state| state.last_compaction_transcript_path.clone()),
            runtime_task_artifact,
        };
        let provider_section = build_provider_section(
            ctx.provider_name.as_str(),
            &ctx.session.model,
            provider_inventory.as_ref(),
        );
        let runtime_sections =
            build_runtime_sections(
                runtime,
                latest_review.as_ref(),
                &always_allow,
                &inventory,
                &artifact_links,
            );

        Ok(CommandOutput::Message(build_status_message(
            ctx,
            &provider_section,
            &runtime_sections,
            cost,
            &resume_warmup,
            startup_profile,
        )))
    }
}
