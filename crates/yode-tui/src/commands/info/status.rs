mod helpers;
mod render;
mod sections;

use crate::commands::context::CommandContext;
use crate::commands::{Command, CommandCategory, CommandMeta, CommandOutput, CommandResult};

use self::helpers::latest_review_summary;
use self::render::{build_runtime_sections, build_status_message};
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
            &runtime_sections,
            cost,
            &resume_warmup,
            startup_profile,
        )))
    }
}

fn write_runtime_task_inventory_artifact(
    project_root: &std::path::Path,
    session_id: &str,
    tasks: Vec<yode_tools::RuntimeTask>,
) -> Option<String> {
    if tasks.is_empty() {
        return None;
    }
    let dir = project_root.join(".yode").join("status");
    std::fs::create_dir_all(&dir).ok()?;
    let short_session = session_id.chars().take(8).collect::<String>();
    let path = dir.join(format!("{}-runtime-tasks.md", short_session));
    let mut body = format!("# Runtime Task Inventory\n\n- Total tasks: {}\n\n", tasks.len());
    for task in tasks {
        body.push_str(&format!(
            "## {}\n\n- Kind: {}\n- Status: {:?}\n- Description: {}\n- Output: {}\n- Transcript: {}\n\n",
            task.id,
            task.kind,
            task.status,
            task.description,
            task.output_path,
            task.transcript_path.as_deref().unwrap_or("none"),
        ));
    }
    std::fs::write(&path, body).ok()?;
    Some(path.display().to_string())
}
