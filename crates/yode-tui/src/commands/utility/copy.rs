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
                description: "Copy last assistant message to clipboard (or /copy N for Nth-latest)",
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

    fn execute(&self, args: &str, ctx: &mut CommandContext) -> CommandResult {
        // Parse optional index argument (1-based from latest)
        let index: Option<usize> = args.trim().parse().ok();

        // Collect all assistant messages
        let assistant_messages: Vec<&str> = ctx
            .chat_entries
            .iter()
            .filter(|e| matches!(e.role, ChatRole::Assistant))
            .map(|e| e.content.as_str())
            .collect();

        if assistant_messages.is_empty() {
            return Ok(CommandOutput::Message(
                "No assistant message to copy.".to_string(),
            ));
        }

        // Get the message to copy
        let content = if let Some(idx) = index {
            // Index from latest (1 = latest, 2 = second latest, etc.)
            if idx == 0 || idx > assistant_messages.len() {
                return Ok(CommandOutput::Message(format!(
                    "Invalid index: {} (valid range: 1-{})",
                    idx,
                    assistant_messages.len()
                )));
            }
            assistant_messages[assistant_messages.len() - idx]
        } else {
            // Default: latest message
            assistant_messages.last().unwrap()
        };

        match Clipboard::new() {
            Ok(mut clipboard) => match clipboard.set_text(content) {
                Ok(_) => {
                    let preview: String = content.chars().take(50).collect();
                    let ellipsis = if content.len() > 50 { "..." } else { "" };
                    let msg = if let Some(idx) = index {
                        format!("Copied message #{} to clipboard:", idx)
                    } else {
                        "Copied latest message to clipboard:".to_string()
                    };
                    Ok(CommandOutput::Message(format!(
                        "{}\n  {}{}{}",
                        msg,
                        preview,
                        ellipsis,
                        if content.len() > 50 {
                            format!("\n  ({} characters total)", content.len())
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
    }
}
