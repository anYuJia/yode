use crate::commands::context::CommandContext;
use crate::commands::{Command, CommandCategory, CommandMeta, CommandOutput, CommandResult};
use yode_tools::builtin::review_common::review_output_has_findings;

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
                content
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
            content
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

#[cfg(test)]
mod tests {
    use super::{extract_review_result_body, review_artifact_badge};

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
}
