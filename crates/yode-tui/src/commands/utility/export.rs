use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

use crate::app::ChatRole;
use crate::commands::context::CommandContext;
use crate::commands::{Command, CommandCategory, CommandMeta, CommandOutput, CommandResult};

pub struct ExportCommand {
    meta: CommandMeta,
}

impl ExportCommand {
    pub fn new() -> Self {
        Self {
            meta: CommandMeta {
                name: "export",
                description: "Export conversation to a file",
                aliases: &[],
                args: vec![],
                category: CommandCategory::Utility,
                hidden: false,
            },
        }
    }
}

impl Command for ExportCommand {
    fn meta(&self) -> &CommandMeta {
        &self.meta
    }

    fn execute(&self, args: &str, ctx: &mut CommandContext) -> CommandResult {
        if ctx.chat_entries.is_empty() {
            return Ok(CommandOutput::Message(
                "No conversation to export.".to_string(),
            ));
        }

        // Generate default filename from first user message or timestamp
        let filename = if args.trim().is_empty() {
            let first_prompt = ctx
                .chat_entries
                .iter()
                .find(|e| matches!(e.role, ChatRole::User))
                .map(|e| {
                    let text = e.content.split('\n').next().unwrap_or("");
                    sanitize_filename(text)
                })
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| timestamp_filename());

            format!("{}.txt", first_prompt)
        } else {
            let filename = args.trim();
            if filename.ends_with(".txt") {
                filename.to_string()
            } else {
                format!("{}.txt", filename)
            }
        };

        // Get current working directory
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let filepath = cwd.join(&filename);

        // Render conversation to text
        let content = render_conversation(ctx);

        // Write to file
        match File::create(&filepath) {
            Ok(mut file) => {
                if let Err(e) = file.write_all(content.as_bytes()) {
                    return Ok(CommandOutput::Message(format!(
                        "Failed to write file: {}",
                        e
                    )));
                }
                Ok(CommandOutput::Message(format!(
                    "Conversation exported to: {}",
                    filepath.display()
                )))
            }
            Err(e) => Ok(CommandOutput::Message(format!(
                "Failed to create file: {}",
                e
            ))),
        }
    }
}

/// Render conversation to plain text format
fn render_conversation(ctx: &CommandContext) -> String {
    let mut output = String::new();

    output.push_str("Conversation exported from Yode\n");
    output.push_str(&format!("Session: {}\n", ctx.session.session_id));
    output.push_str(&format!(
        "Date: {}\n",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
    ));
    output.push_str(&format!("Model: {}\n\n", ctx.session.model));
    output.push_str(&"=".repeat(60));
    output.push_str("\n\n");

    for entry in ctx.chat_entries.iter() {
        let role_label = match &entry.role {
            ChatRole::User => "User",
            ChatRole::Assistant => "Assistant",
            ChatRole::ToolCall { name, .. } => &format!("[Tool: {}]", name),
            ChatRole::ToolResult { name, is_error, .. } => {
                if *is_error {
                    &format!("[Tool Error: {}]", name)
                } else {
                    &format!("[Tool Result: {}]", name)
                }
            }
            ChatRole::Error => "[Error]",
            ChatRole::System => "[System]",
            ChatRole::SubAgentCall { description } => &format!("[SubAgent: {}]", description),
            ChatRole::SubAgentToolCall { name } => &format!("[SubAgent Tool: {}]", name),
            ChatRole::SubAgentResult => "[SubAgent Result]",
            ChatRole::AskUser { id } => &format!("[AskUser: {}]", id),
        };

        output.push_str(&format!("--- {}\n", role_label));
        output.push_str(&entry.content);
        output.push_str("\n\n");
    }

    // Add stats summary
    output.push_str(&"=".repeat(60));
    output.push_str("\n\n");
    output.push_str("Statistics:\n");
    output.push_str(&format!("  Input tokens:  {}\n", ctx.session.input_tokens));
    output.push_str(&format!("  Output tokens: {}\n", ctx.session.output_tokens));
    output.push_str(&format!("  Total tokens:  {}\n", ctx.session.total_tokens));
    output.push_str(&format!(
        "  Tool calls:    {}\n",
        ctx.session.tool_call_count
    ));

    output
}

/// Sanitize string for use as filename
fn sanitize_filename(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == ' ' || *c == '-')
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<&str>>()
        .join("-")
        .chars()
        .take(50)
        .collect()
}

/// Generate timestamp-based filename
fn timestamp_filename() -> String {
    chrono::Local::now().format("%Y-%m-%d-%H%M%S").to_string()
}
