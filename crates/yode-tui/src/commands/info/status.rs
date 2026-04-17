mod helpers;
mod render;
mod sections;

use crate::commands::context::CommandContext;
use crate::commands::{Command, CommandCategory, CommandMeta, CommandOutput, CommandResult};
use crate::commands::artifact_nav::{
    latest_agent_team_monitor_artifact, latest_coordinator_artifact,
    latest_hook_deferred_artifact, latest_permission_governance_artifact,
    latest_remote_live_session_artifact, latest_runtime_orchestration_artifact,
    latest_workflow_execution_artifact,
};
use crate::runtime_artifacts::{
    write_hook_failure_artifact, write_runtime_task_inventory_artifact,
};
use crate::ui::status_summary::{
    context_window_summary_text, runtime_status_snapshot_from_parts,
    session_runtime_summary_text, tool_runtime_summary_text,
};

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
        let runtime_tasks = ctx
            .engine
            .try_lock()
            .ok()
            .map(|engine| engine.runtime_tasks_snapshot())
            .unwrap_or_default();
        let fallback_context_tokens: usize = ctx.chat_entries.iter().map(|e| e.content.len()).sum::<usize>() / 4;
        let runtime_snapshot = runtime_status_snapshot_from_parts(
            &working_dir,
            runtime.clone(),
            runtime_tasks
                .iter()
                .filter(|task| matches!(task.status, yode_tools::RuntimeTaskStatus::Running))
                .count(),
        );
        let runtime_summary =
            session_runtime_summary_text(&runtime_snapshot, fallback_context_tokens);
        let context_summary =
            context_window_summary_text(runtime_snapshot.state.as_ref(), fallback_context_tokens);
        let tool_summary = runtime_snapshot
            .state
            .as_ref()
            .map(tool_runtime_summary_text)
            .unwrap_or_else(|| "engine busy".to_string());
        let runtime_task_artifact = write_runtime_task_inventory_artifact(
            &working_dir,
            &ctx.session.session_id,
            runtime_snapshot.state.as_ref(),
            runtime_tasks.clone(),
        );
        let hook_artifact = runtime
            .as_ref()
            .and_then(|state| write_hook_failure_artifact(&working_dir, &ctx.session.session_id, state));
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
            settings_scope_artifact: latest_startup_artifact_link(&working_dir, "settings-scopes.json"),
            managed_mcp_artifact: latest_startup_artifact_link(&working_dir, "managed-mcp-inventory.json"),
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
            permission_governance_artifact: latest_permission_governance_artifact(&working_dir)
                .map(|path| path.display().to_string()),
            transcript_artifact: runtime
                .as_ref()
                .and_then(|state| state.last_compaction_transcript_path.clone()),
            runtime_task_artifact,
            hook_artifact,
            hook_deferred_artifact: latest_hook_deferred_artifact(&working_dir)
                .map(|path| path.display().to_string()),
            team_monitor_artifact: latest_agent_team_monitor_artifact(&working_dir)
                .map(|path| path.display().to_string()),
            remote_live_artifact: latest_remote_live_session_artifact(&working_dir)
                .map(|path| path.display().to_string()),
            tool_search_activation_artifact: latest_startup_artifact_link(
                &working_dir,
                "tool-search-activation.json",
            ),
            workflow_artifact: latest_workflow_execution_artifact(&working_dir)
                .map(|path| path.display().to_string()),
            coordinator_artifact: latest_coordinator_artifact(&working_dir)
                .map(|path| path.display().to_string()),
            orchestration_artifact: latest_runtime_orchestration_artifact(&working_dir)
                .map(|path| path.display().to_string()),
        };
        let provider_section = build_provider_section(
            ctx.provider_name.as_str(),
            &ctx.session.model,
            provider_inventory.as_ref(),
        );
        let runtime_sections =
            build_runtime_sections(
                &working_dir,
                runtime,
                &runtime_tasks,
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
            &runtime_summary,
            &context_summary,
            &tool_summary,
        )))
    }
}
