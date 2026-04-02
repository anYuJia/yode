use crate::commands::context::CommandContext;
use crate::commands::{ArgCompletionSource, ArgDef, Command, CommandCategory, CommandMeta, CommandOutput, CommandResult};

pub struct HistoryCommand {
    meta: CommandMeta,
}

impl HistoryCommand {
    pub fn new() -> Self {
        Self {
            meta: CommandMeta {
                name: "history",
                description: "Show recent input history",
                aliases: &[],
                args: vec![ArgDef {
                    name: "count".to_string(),
                    required: false,
                    hint: "[count]".to_string(),
                    completions: ArgCompletionSource::None,
                }],
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
        let count = args.trim().parse::<usize>().unwrap_or(10).min(50);
        let start = entries.len().saturating_sub(count);
        if entries.is_empty() {
            Ok(CommandOutput::Message(
                "No input history yet.".to_string(),
            ))
        } else {
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
            Ok(CommandOutput::Message(lines))
        }
    }
}
