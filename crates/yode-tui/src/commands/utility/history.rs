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
                        let preview: String = entry.chars().take(100).collect();
                        let ellipsis = if entry.len() > 100 { "..." } else { "" };
                        lines.push_str(&format!("  {:>3}. {}{}\n", idx + 1, preview, ellipsis));
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
                    let preview: String = entry.chars().take(100).collect();
                    let ellipsis = if entry.len() > 100 { "..." } else { "" };
                    lines.push_str(&format!(
                        "  {:>3}. {}{}\n",
                        start + i + 1,
                        preview,
                        ellipsis
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
                    let preview: String = entry.chars().take(80).collect();
                    let ellipsis = if entry.len() > 80 { "..." } else { "" };
                    lines.push_str(&format!(
                        "  {:>3}. {}{}\n",
                        start + i + 1,
                        preview,
                        ellipsis
                    ));
                }
                lines.push_str("\nUse /history pick, /history search <text>, or /history use <index>.\n");
                Ok(CommandOutput::Message(lines))
            }
            _ => Err("Usage: /history [count|pick|use <index>|search <text>]".to_string()),
        }
    }
}
