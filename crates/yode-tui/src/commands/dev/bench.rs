use crate::commands::context::CommandContext;
use crate::commands::{
    ArgCompletionSource, ArgDef, Command, CommandCategory, CommandMeta, CommandOutput,
    CommandResult,
};

pub struct BenchCommand {
    meta: CommandMeta,
}

impl BenchCommand {
    pub fn new() -> Self {
        Self {
            meta: CommandMeta {
                name: "bench",
                description: "Run development benchmarks for long-session diagnostics",
                aliases: &[],
                args: vec![ArgDef {
                    name: "target".to_string(),
                    required: false,
                    hint: "[long-session]".to_string(),
                    completions: ArgCompletionSource::Static(vec!["long-session".to_string()]),
                }],
                category: CommandCategory::Development,
                hidden: false,
            },
        }
    }
}

impl Command for BenchCommand {
    fn meta(&self) -> &CommandMeta {
        &self.meta
    }

    fn execute(&self, args: &str, ctx: &mut CommandContext) -> CommandResult {
        let target = args.trim();
        match target {
            "" | "long-session" => {
                let project_root = std::path::PathBuf::from(&ctx.session.working_dir);
                let report = crate::commands::info::run_long_session_benchmark(&project_root);
                let compare_line = match (
                    report.compare_pair.as_ref(),
                    report.compare_ms,
                    report.compare_summary_only,
                ) {
                    (Some((left, right)), Some(ms), Some(summary_only)) => format!(
                        "  Compare latest:   {} ms (summary-only={})\n  Compare pair:     {} <> {}",
                        ms,
                        if summary_only { "yes" } else { "no" },
                        left,
                        right
                    ),
                    _ => "  Compare latest:   skipped (need at least 2 transcripts)".to_string(),
                };
                Ok(CommandOutput::Message(format!(
                    "Long-session benchmark\n  Transcript dir:   {}\n  Transcript count: {}\n  Latest lookup:    cold {} ms / hot {} ms\n  Failed filter:    cold {} ms / hot {} ms\n  Resume warmup:    {} ms ({} metadata, latest={})\n{}",
                    report.transcript_dir.display(),
                    report.transcript_count,
                    report.cold_latest_lookup_ms,
                    report.hot_latest_lookup_ms,
                    report.cold_failed_filter_ms,
                    report.hot_failed_filter_ms,
                    report.resume_warmup.duration_ms,
                    report.resume_warmup.metadata_entries_warmed,
                    if report.resume_warmup.latest_lookup_cached {
                        "yes"
                    } else {
                        "no"
                    },
                    compare_line,
                )))
            }
            other => Err(format!(
                "Unknown benchmark target '{}'. Use /bench long-session.",
                other
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::BenchCommand;
    use crate::commands::Command;

    #[test]
    fn bench_command_accepts_long_session_target() {
        let cmd = BenchCommand::new();
        assert_eq!(cmd.meta().name, "bench");
        assert!(cmd.meta().description.contains("benchmarks"));
    }
}
