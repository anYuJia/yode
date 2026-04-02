use arboard::Clipboard;

use crate::app::ChatRole;
use crate::commands::context::CommandContext;
use crate::commands::{Command, CommandCategory, CommandMeta, CommandOutput, CommandResult};

pub struct CopyCommand {
    meta: CommandMeta,
}

impl CopyCommand {
    pub fn new() -> Self {
        Self {
            meta: CommandMeta {
                name: "copy",
                description: "Copy last assistant message to clipboard",
                aliases: &[],
                args: vec![],
                category: CommandCategory::Utility,
                hidden: false,
            },
        }
    }
}

impl Command for CopyCommand {
    fn meta(&self) -> &CommandMeta {
        &self.meta
    }

    fn execute(&self, _args: &str, ctx: &mut CommandContext) -> CommandResult {
        let last_assistant = ctx
            .chat_entries
            .iter()
            .rev()
            .find(|e| matches!(e.role, ChatRole::Assistant));

        if let Some(entry) = last_assistant {
            match Clipboard::new() {
                Ok(mut clipboard) => match clipboard.set_text(&entry.content) {
                    Ok(_) => {
                        let preview: String = entry.content.chars().take(50).collect();
                        let ellipsis = if entry.content.len() > 50 {
                            "..."
                        } else {
                            ""
                        };
                        Ok(CommandOutput::Message(format!(
                            "Copied to clipboard:\n  {}{}{}",
                            preview,
                            ellipsis,
                            if entry.content.len() > 50 {
                                format!("\n  ({} characters total)", entry.content.len())
                            } else {
                                String::new()
                            }
                        )))
                    }
                    Err(e) => Ok(CommandOutput::Message(format!(
                        "Failed to copy to clipboard: {}",
                        e
                    ))),
                },
                Err(e) => Ok(CommandOutput::Message(format!(
                    "Failed to access clipboard: {}",
                    e
                ))),
            }
        } else {
            Ok(CommandOutput::Message(
                "No assistant message to copy.".to_string(),
            ))
        }
    }
}
