mod compare;
mod document;
mod render;
mod transcripts;

use std::path::PathBuf;

#[cfg(test)]
use self::compare::build_transcript_compare_output;
use self::compare::{parse_compare_args, CompareArgs, CompareOptions};
#[cfg(test)]
use self::document::{memory_entry_age, parse_memory_document};
use self::render::{
    render_latest_transcript, render_memory_file, render_memory_status, render_transcript_compare,
    render_transcript_file, render_transcript_list, render_transcript_picker,
};
#[cfg(test)]
use self::transcripts::{
    extract_summary_preview, filtered_transcript_entries, fold_transcript_preview,
    parse_date_range_filter, read_transcript_metadata, resolve_compare_target,
    truncate_for_display, TranscriptListFilter, TranscriptMode,
};
use self::transcripts::{
    latest_transcript, parse_latest_compare_target, parse_list_filter, resolve_transcript_target,
};
pub(crate) use self::transcripts::{
    run_long_session_benchmark, transcript_cache_stats, warm_resume_transcript_caches,
    ResumeTranscriptCacheWarmupStats,
};
use crate::commands::context::CommandContext;
use crate::commands::{
    ArgCompletionSource, ArgDef, Command, CommandCategory, CommandMeta, CommandOutput,
    CommandResult,
};

const MAX_DISPLAY_CHARS: usize = 12_000;
const MAX_COMPARE_CONTENT_CHARS: usize = 200_000;
const TRANSCRIPT_PREVIEW_MESSAGE_HEAD_LINES: usize = 18;
const TRANSCRIPT_PREVIEW_TAIL_LINES: usize = 12;

pub struct MemoryCommand {
    meta: CommandMeta,
}

impl MemoryCommand {
    pub fn new() -> Self {
        Self {
            meta: CommandMeta {
                name: "memory",
                description: "Inspect live memory, compacted memory, and transcripts",
                aliases: &["mem"],
                args: vec![ArgDef {
                    name: "target".to_string(),
                    required: false,
                    hint: "[live|session|latest|list [filters...]|compare <a> <b> [--no-diff|--hunks N|--lines N]|<index>|<file>]".to_string(),
                    completions: ArgCompletionSource::Static(vec![
                        "live".to_string(),
                        "session".to_string(),
                        "latest".to_string(),
                        "pick".to_string(),
                        "list".to_string(),
                        "list recent".to_string(),
                        "list auto".to_string(),
                        "list manual".to_string(),
                        "list summary".to_string(),
                        "list failed".to_string(),
                        "list summary failed".to_string(),
                        "list today".to_string(),
                        "list yesterday".to_string(),
                        "compare".to_string(),
                        "compare latest latest-1".to_string(),
                    ]),
                }],
                category: CommandCategory::Info,
                hidden: false,
            },
        }
    }
}

impl Command for MemoryCommand {
    fn meta(&self) -> &CommandMeta {
        &self.meta
    }

    fn execute(&self, args: &str, ctx: &mut CommandContext) -> CommandResult {
        let project_root = PathBuf::from(&ctx.session.working_dir);
        let live_path = yode_core::session_memory::live_session_memory_path(&project_root);
        let session_path = yode_core::session_memory::session_memory_path(&project_root);
        let transcripts_dir = project_root.join(".yode").join("transcripts");
        let runtime = ctx
            .engine
            .try_lock()
            .ok()
            .map(|engine| engine.runtime_state());

        let args = args.trim();

        if args == "compare" || args.starts_with("compare ") {
            let compare = parse_compare_args(args).ok_or_else(|| {
                "Usage: /memory compare <a> <b> [--no-diff] [--hunks N] [--lines N]".to_string()
            })?;
            return render_transcript_compare(&transcripts_dir, &compare);
        }

        if args == "list" || args.starts_with("list ") {
            let filter = parse_list_filter(args)?;
            return Ok(CommandOutput::Message(render_transcript_list(
                &transcripts_dir,
                &filter,
            )));
        }

        match args {
            "" => Ok(CommandOutput::Message(render_memory_status(
                &live_path,
                &session_path,
                &transcripts_dir,
                runtime.as_ref(),
                ctx.session.resume_cache_warmup.as_ref(),
            ))),
            "live" => render_memory_file("Live session memory", &live_path),
            "session" => render_memory_file("Compaction memory", &session_path),
            "pick" => Ok(CommandOutput::Message(render_transcript_picker(
                &transcripts_dir,
            ))),
            _ if args.starts_with("latest compare ") => parse_latest_compare_target(args)
                .ok_or_else(|| "Usage: /memory latest compare <target>".to_string())
                .and_then(|target| {
                    render_transcript_compare(
                        &transcripts_dir,
                        &CompareArgs {
                            left_target: "latest".to_string(),
                            right_target: target.to_string(),
                            options: CompareOptions::default(),
                        },
                    )
                }),
            "latest" => {
                let latest = latest_transcript(&transcripts_dir).ok_or_else(|| {
                    "No transcript backups found. Transcript artifacts are written only after a compaction that actually removes or truncates content.".to_string()
                })?;
                render_latest_transcript(&latest)
            }
            target => {
                let transcript = resolve_transcript_target(&transcripts_dir, target)
                    .ok_or_else(|| format!("Unknown memory target: {}", target))?;
                render_transcript_file(&transcript)
            }
        }
    }
}

#[cfg(test)]
mod tests;
