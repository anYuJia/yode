use crate::commands::context::CommandContext;
use crate::commands::{
    ArgCompletionSource, ArgDef, Command, CommandCategory, CommandMeta, CommandOutput,
    CommandResult,
};
use yode_tools::builtin::review_common::ReviewStatusSnapshot;

pub struct ShipCommand {
    meta: CommandMeta,
}

impl ShipCommand {
    pub fn new() -> Self {
        Self {
            meta: CommandMeta {
                name: "ship",
                description: "Prefill a review_then_commit prompt into the input box",
                aliases: &[],
                args: vec![ArgDef {
                    name: "message".to_string(),
                    required: false,
                    hint: "[commit message]".to_string(),
                    completions: ArgCompletionSource::None,
                }],
                category: CommandCategory::Development,
                hidden: false,
            },
        }
    }
}

impl Command for ShipCommand {
    fn meta(&self) -> &CommandMeta {
        &self.meta
    }

    fn execute(&self, args: &str, ctx: &mut CommandContext) -> CommandResult {
        let trimmed = args.trim();
        let (force_override, message) = if let Some(rest) = trimmed.strip_prefix("force ") {
            (true, rest.trim())
        } else {
            (false, trimmed)
        };
        let message = if message.is_empty() {
            "describe the current change".to_string()
        } else {
            message.to_string()
        };

        let latest_status = latest_review_status(
            &std::path::PathBuf::from(&ctx.session.working_dir)
                .join(".yode")
                .join("reviews")
                .join("latest-status.json"),
        );

        if let Some(status) = latest_status.as_ref() {
            if status.status == "findings" && !force_override {
                ctx.input.set_text(&format!(
                    "Use `review_changes` with focus=\"current workspace changes\". Latest review artifact {} reported {} finding(s); confirm they are fixed before any commit attempt.",
                    status
                        .artifact_path
                        .as_deref()
                        .unwrap_or("unknown artifact"),
                    status.findings_count
                ));
                return Ok(CommandOutput::Message(
                    "Latest review status still reports findings. Loaded a follow-up review prompt instead of a commit flow. Use `/ship force <message>` only if you intentionally want to override.".to_string(),
                ));
            }
        }

        let mut prompt = format!(
            "Use `review_then_commit` with message=\"{}\" and focus=\"current workspace changes\". If review finds issues, stop and summarize them instead of committing.",
            message
        );
        if force_override {
            prompt.push_str(
                " Set allow_findings_commit=true because the user explicitly requested an override.",
            );
        }
        ctx.input.set_text(&prompt);
        Ok(CommandOutput::Message(if force_override {
            "Loaded a forced review-then-commit prompt into the input box.".to_string()
        } else {
            "Loaded a review-then-commit prompt into the input box.".to_string()
        }))
    }
}

fn latest_review_status(path: &std::path::Path) -> Option<ReviewStatusSnapshot> {
    let content = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

#[cfg(test)]
mod tests {
    use super::latest_review_status;
    use yode_tools::builtin::review_common::ReviewStatusSnapshot;

    #[test]
    fn latest_review_status_reads_snapshot() {
        let path =
            std::env::temp_dir().join(format!("yode-ship-status-{}.json", std::process::id()));
        let snapshot = ReviewStatusSnapshot {
            kind: "review".to_string(),
            title: "current changes".to_string(),
            timestamp: "2026-01-01 12:00:00".to_string(),
            status: "findings".to_string(),
            findings_count: 2,
            artifact_path: Some("/tmp/review.md".to_string()),
        };
        std::fs::write(&path, serde_json::to_string(&snapshot).unwrap()).unwrap();
        let loaded = latest_review_status(&path).unwrap();
        assert_eq!(loaded.status, "findings");
        assert_eq!(loaded.findings_count, 2);
        let _ = std::fs::remove_file(path);
    }
}
