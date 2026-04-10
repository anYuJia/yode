use crate::commands::context::CommandContext;
use crate::commands::{Command, CommandCategory, CommandMeta, CommandOutput, CommandResult};
use yode_tools::builtin::review_common::review_output_has_findings;

const REVIEW_ARTIFACT_PREVIEW_HEAD_LINES: usize = 80;
const REVIEW_ARTIFACT_PREVIEW_TAIL_LINES: usize = 30;
const REVIEW_ARTIFACT_PREVIEW_MAX_LINES: usize = 140;

pub struct ReviewsCommand {
    meta: CommandMeta,
}

impl ReviewsCommand {
    pub fn new() -> Self {
        Self {
            meta: CommandMeta {
                name: "reviews",
                description: "List or open review artifacts under .yode/reviews, optionally filtered by kind",
                aliases: &[],
                args: vec![],
                category: CommandCategory::Development,
                hidden: false,
            },
        }
    }
}

impl Command for ReviewsCommand {
    fn meta(&self) -> &CommandMeta {
        &self.meta
    }

    fn execute(&self, args: &str, ctx: &mut CommandContext) -> CommandResult {
        let dir = std::path::PathBuf::from(&ctx.session.working_dir)
            .join(".yode")
            .join("reviews");
        let mut entries = std::fs::read_dir(&dir)
            .ok()
            .into_iter()
            .flat_map(|entries| entries.filter_map(Result::ok))
            .map(|entry| entry.path())
            .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("md"))
            .collect::<Vec<_>>();
        entries.sort_by(|a, b| b.file_name().cmp(&a.file_name()));

        let trimmed = args.trim();
        let parts = trimmed.split_whitespace().collect::<Vec<_>>();
        let kind_filter = match parts.as_slice() {
            ["summary", kind] => Some(*kind),
            ["latest", kind] => Some(*kind),
            ["list", kind] => Some(*kind),
            [kind] if !kind.chars().all(|c| c.is_ascii_digit()) && *kind != "latest" => Some(*kind),
            _ => None,
        };
        if let Some(kind) = kind_filter {
            entries.retain(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .map(|name| name.starts_with(&format!("{}-", kind)))
                    .unwrap_or(false)
            });
        }

        if trimmed.is_empty() || matches!(parts.as_slice(), ["list"] | ["list", _]) {
            if entries.is_empty() {
                return Ok(CommandOutput::Message(format!(
                    "No review artifacts{} found in {}.",
                    kind_filter
                        .map(|kind| format!(" for '{}'", kind))
                        .unwrap_or_default(),
                    dir.display()
                )));
            }
            let mut output = format!(
                "Review artifacts{} in {}:\n",
                kind_filter
                    .map(|kind| format!(" [{}]", kind))
                    .unwrap_or_default(),
                dir.display()
            );
            for (idx, path) in entries.iter().take(12).enumerate() {
                let badge = std::fs::read_to_string(path)
                    .ok()
                    .map(|content| review_artifact_badge(&content))
                    .unwrap_or("unknown");
                output.push_str(&format!(
                    "  {:>2}. [{}] {}\n",
                    idx + 1,
                    badge,
                    path.display()
                ));
            }
            output.push_str("\nUse /reviews <index>, /reviews latest, or /reviews latest <kind>.");
            return Ok(CommandOutput::Message(output));
        }

        if matches!(parts.as_slice(), ["summary"] | ["summary", _]) {
            if entries.is_empty() {
                return Ok(CommandOutput::Message(format!(
                    "No review artifacts{} found in {}.",
                    kind_filter
                        .map(|kind| format!(" for '{}'", kind))
                        .unwrap_or_default(),
                    dir.display()
                )));
            }
            let summary = summarize_review_artifacts(&entries);
            return Ok(CommandOutput::Message(summary));
        }

        if matches!(parts.as_slice(), ["latest"] | ["latest", _]) {
            if entries.is_empty() {
                return Err("No review artifacts available.".to_string());
            }
            let path = &entries[0];
            let content = std::fs::read_to_string(path)
                .map_err(|err| format!("Failed to read {}: {}", path.display(), err))?;
            return Ok(CommandOutput::Message(format!(
                "Latest review artifact{} [{}]\nPath: {}\n\n{}",
                kind_filter
                    .map(|kind| format!(" [{}]", kind))
                    .unwrap_or_default(),
                review_artifact_badge(&content),
                path.display(),
                fold_review_artifact_preview(&content)
            )));
        }

        let index = trimmed
            .parse::<usize>()
            .map_err(|_| {
                "Usage: /reviews | /reviews list [kind] | /reviews latest [kind] | /reviews <index>"
                    .to_string()
            })?;
        if index == 0 || index > entries.len() {
            return Err(format!("Review artifact index out of range: {}", index));
        }
        let path = &entries[index - 1];
        let content = std::fs::read_to_string(path)
            .map_err(|err| format!("Failed to read {}: {}", path.display(), err))?;
        Ok(CommandOutput::Message(format!(
            "Review artifact {} [{}]\nPath: {}\n\n{}",
            index,
            review_artifact_badge(&content),
            path.display(),
            fold_review_artifact_preview(&content)
        )))
    }
}

fn review_artifact_badge(content: &str) -> &'static str {
    let body = extract_review_result_body(content).unwrap_or(content);
    if body.trim().is_empty() {
        return "unknown";
    }
    if review_output_has_findings(body) {
        "findings"
    } else {
        "clean"
    }
}

fn extract_review_result_body(content: &str) -> Option<&str> {
    let start = content.find("```text\n")?;
    let body_start = start + "```text\n".len();
    let end = content[body_start..].find("\n```")?;
    Some(&content[body_start..body_start + end])
}

fn summarize_review_artifacts(entries: &[std::path::PathBuf]) -> String {
    let mut clean = 0usize;
    let mut findings = 0usize;
    let mut unknown = 0usize;
    let mut by_kind = std::collections::BTreeMap::<String, usize>::new();

    for path in entries {
        let kind = path
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(|name| name.split('-').next())
            .unwrap_or("unknown")
            .to_string();
        *by_kind.entry(kind).or_default() += 1;

        match std::fs::read_to_string(path)
            .ok()
            .map(|content| review_artifact_badge(&content))
            .unwrap_or("unknown")
        {
            "clean" => clean += 1,
            "findings" => findings += 1,
            _ => unknown += 1,
        }
    }

    let mut output = format!(
        "Review artifact summary:\n  Total:    {}\n  Clean:    {}\n  Findings: {}\n  Unknown:  {}\n",
        entries.len(),
        clean,
        findings,
        unknown
    );
    output.push_str("\nBy kind:\n");
    for (kind, count) in by_kind {
        output.push_str(&format!("  - {}: {}\n", kind, count));
    }
    output.push_str("\nUse /reviews latest [kind] to inspect the latest artifact.");
    output
}

fn fold_review_artifact_preview(content: &str) -> String {
    let lines = content.lines().collect::<Vec<_>>();
    if lines.len() <= REVIEW_ARTIFACT_PREVIEW_MAX_LINES {
        return content.to_string();
    }

    let head_count = REVIEW_ARTIFACT_PREVIEW_HEAD_LINES.min(lines.len());
    let tail_count = REVIEW_ARTIFACT_PREVIEW_TAIL_LINES.min(lines.len().saturating_sub(head_count));
    let omitted = lines.len().saturating_sub(head_count + tail_count);
    let mut output = lines[..head_count].join("\n");
    output.push_str(&format!(
        "\n\n... [review artifact preview folded: {} middle lines omitted] ...\n\n",
        omitted
    ));
    if tail_count > 0 {
        output.push_str(&lines[lines.len() - tail_count..].join("\n"));
    }
    output
}

#[cfg(test)]
mod tests {
    use super::{
        extract_review_result_body, fold_review_artifact_preview, review_artifact_badge,
        summarize_review_artifacts,
    };

    #[test]
    fn review_artifact_badge_detects_clean_output() {
        let content = "# Review Artifact\n\n## Result\n\n```text\nNo issues found.\nResidual risk: none.\n```\n";
        assert_eq!(review_artifact_badge(content), "clean");
    }

    #[test]
    fn review_artifact_badge_detects_findings_output() {
        let content = "# Review Artifact\n\n## Result\n\n```text\n1. Missing regression test\n```\n";
        assert_eq!(review_artifact_badge(content), "findings");
    }

    #[test]
    fn extract_review_result_body_reads_text_fence() {
        let content = "before\n```text\nhello\nworld\n```\nafter";
        assert_eq!(extract_review_result_body(content), Some("hello\nworld"));
    }

    #[test]
    fn summarize_review_artifacts_counts_statuses_and_kinds() {
        let dir = std::env::temp_dir().join(format!("yode-review-summary-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let clean = dir.join("review-20260101.md");
        let finding = dir.join("verification-20260102.md");
        std::fs::write(
            &clean,
            "# Review Artifact\n\n## Result\n\n```text\nNo issues found.\n```\n",
        )
        .unwrap();
        std::fs::write(
            &finding,
            "# Review Artifact\n\n## Result\n\n```text\n1. Missing test\n```\n",
        )
        .unwrap();
        let output = summarize_review_artifacts(&[clean, finding]);
        assert!(output.contains("Clean:    1"));
        assert!(output.contains("Findings: 1"));
        assert!(output.contains("review: 1"));
        assert!(output.contains("verification: 1"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn review_artifact_preview_folds_long_output() {
        let mut content = String::from("# Review Artifact\n\n");
        for i in 0..200 {
            content.push_str(&format!("line {}\n", i));
        }

        let folded = fold_review_artifact_preview(&content);
        assert!(folded.contains("review artifact preview folded"));
        assert!(folded.contains("line 199"));
    }
}
