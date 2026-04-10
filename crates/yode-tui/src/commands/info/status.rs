mod helpers;
mod render;

use crate::commands::context::CommandContext;
use crate::commands::{Command, CommandCategory, CommandMeta, CommandOutput, CommandResult};

use super::cost::estimate_cost;
use self::helpers::latest_review_summary;
use self::render::{build_runtime_sections, build_status_message};

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
                    if stats.latest_lookup_cached { "yes" } else { "no" },
                    stats.duration_ms
                )
            })
            .unwrap_or_else(|| "none".to_string());
        let runtime = ctx
            .engine
            .try_lock()
            .ok()
            .map(|engine| engine.runtime_state());
        let runtime_sections =
            build_runtime_sections(runtime, latest_review.as_ref(), &always_allow);

        Ok(CommandOutput::Message(build_status_message(
            ctx,
            &runtime_sections,
            cost,
            &resume_warmup,
        )))
    }
}
