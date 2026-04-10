use crate::commands::context::CommandContext;
use crate::commands::{
    ArgCompletionSource, ArgDef, Command, CommandCategory, CommandMeta, CommandOutput,
    CommandResult,
};

pub struct HistoryCommand {
    meta: CommandMeta,
}

impl HistoryCommand {
    pub fn new() -> Self {
        Self {
            meta: CommandMeta {
                name: "history",
                description: "Show, search, or reuse recent input history",
                aliases: &[],
                args: vec![
                    ArgDef {
                        name: "mode".to_string(),
                        required: false,
                        hint: "[count|pick|use <index>|search <text>]".to_string(),
                        completions: ArgCompletionSource::Static(vec![
                            "pick".to_string(),
                            "use".to_string(),
                            "search".to_string(),
                        ]),
                    },
                    ArgDef {
                        name: "value".to_string(),
                        required: false,
                        hint: "[count|index|query]".to_string(),
                        completions: ArgCompletionSource::None,
                    },
                ],
                category: CommandCategory::Utility,
                hidden: false,
            },
        }
    }
}

impl Command for HistoryCommand {
    fn meta(&self) -> &CommandMeta {
        &self.meta
    }

    fn execute(&self, args: &str, ctx: &mut CommandContext) -> CommandResult {
        let entries = ctx.input_history;
        if entries.is_empty() {
            return Ok(CommandOutput::Message("No input history yet.".to_string()));
        }

        let trimmed = args.trim();
        let parts = trimmed.split_whitespace().collect::<Vec<_>>();
        match parts.as_slice() {
            ["use", index] => {
                let index = index
                    .parse::<usize>()
                    .map_err(|_| "Usage: /history use <index>".to_string())?;
                if index == 0 || index > entries.len() {
                    return Err(format!("History index out of range: {}", index));
                }
                let selected = entries[index - 1].clone();
                ctx.input.set_text(&selected);
                Ok(CommandOutput::Message(format!(
                    "Loaded history #{} into the input box.",
                    index
                )))
            }
            ["search", query @ ..] if !query.is_empty() => {
                let needle = query.join(" ").to_lowercase();
                let mut lines = String::from("Matching input history:\n");
                let mut found = 0;
                for (idx, entry) in entries.iter().enumerate().rev() {
                    if entry.to_lowercase().contains(&needle) {
                        lines.push_str(&format!(
                            "  {:>3}. {}\n",
                            idx + 1,
                            history_preview(entry, Some(&needle), 110)
                        ));
                        found += 1;
                        if found >= 12 {
                            break;
                        }
                    }
                }
                if found == 0 {
                    lines.push_str("  (no matches)\n");
                } else {
                    lines.push_str("\nUse /history use <index> to load one into the input box.\n");
                }
                Ok(CommandOutput::Message(lines))
            }
            ["pick"] => {
                let count = 12.min(entries.len());
                let start = entries.len().saturating_sub(count);
                let mut lines = String::from("History picker:\n");
                for (i, entry) in entries[start..].iter().enumerate() {
                    lines.push_str(&format!(
                        "  {:>3}. {}\n",
                        start + i + 1,
                        history_preview(entry, None, 100)
                    ));
                }
                lines.push_str("\nUse /history use <index> to load one into the input box.\n");
                Ok(CommandOutput::Message(lines))
            }
            [] | [_] => {
                let count = trimmed.parse::<usize>().unwrap_or(10).min(50);
                let start = entries.len().saturating_sub(count);
                let mut lines = String::from("Recent input history:\n");
                for (i, entry) in entries[start..].iter().enumerate() {
                    lines.push_str(&format!(
                        "  {:>3}. {}\n",
                        start + i + 1,
                        history_preview(entry, None, 80)
                    ));
                }
                lines.push_str("\nUse /history pick, /history search <text>, or /history use <index>.\n");
                Ok(CommandOutput::Message(lines))
            }
            _ => Err("Usage: /history [count|pick|use <index>|search <text>]".to_string()),
        }
    }
}

fn history_preview(entry: &str, query: Option<&str>, max_chars: usize) -> String {
    let squashed = entry.split_whitespace().collect::<Vec<_>>().join(" ");
    let query = query.filter(|query| !query.is_empty());
    let preview = if let Some(query) = query {
        let lower = squashed.to_lowercase();
        if let Some(match_index) = lower.find(query) {
            let char_positions = squashed.char_indices().map(|(idx, _)| idx).collect::<Vec<_>>();
            let start_char = lower[..match_index].chars().count().saturating_sub(max_chars / 3);
            let end_char = (lower[..match_index].chars().count()
                + query.chars().count()
                + max_chars / 2)
                .min(squashed.chars().count());
            let start = *char_positions.get(start_char).unwrap_or(&0);
            let end = char_positions
                .get(end_char)
                .copied()
                .unwrap_or_else(|| squashed.len());
            let snippet = squashed[start..end].to_string();
            let prefix = if start > 0 { "..." } else { "" };
            let suffix = if end < squashed.len() { "..." } else { "" };
            format!("{}{}{}", prefix, snippet, suffix)
        } else {
            squashed
        }
    } else {
        squashed
    };

    if preview.chars().count() <= max_chars {
        preview
    } else {
        format!("{}...", preview.chars().take(max_chars).collect::<String>())
    }
}

#[cfg(test)]
mod tests {
    use super::history_preview;

    #[test]
    fn history_preview_focuses_on_query_context() {
        let entry = "first step second step compile failure third step final note";
        let preview = history_preview(entry, Some("failure"), 30);
        assert!(preview.contains("failure"));
        assert!(preview.starts_with("...") || preview.contains("compile"));
    }
}
