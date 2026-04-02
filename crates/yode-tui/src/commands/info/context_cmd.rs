use crate::commands::context::CommandContext;
use crate::commands::{Command, CommandCategory, CommandMeta, CommandOutput, CommandResult};

pub struct ContextCommand {
    meta: CommandMeta,
}

impl ContextCommand {
    pub fn new() -> Self {
        Self {
            meta: CommandMeta {
                name: "context",
                description: "Show context window usage",
                aliases: &["ctx"],
                args: vec![],
                category: CommandCategory::Info,
                hidden: false,
            },
        }
    }
}

impl Command for ContextCommand {
    fn meta(&self) -> &CommandMeta {
        &self.meta
    }

    fn execute(&self, _args: &str, ctx: &mut CommandContext) -> CommandResult {
        let total_chars: usize = ctx.chat_entries.iter().map(|e| e.content.len()).sum();
        let est_tokens = total_chars / 4;
        let pct = if ctx.session.total_tokens > 0 {
            (est_tokens as f64 / 128_000.0 * 100.0).min(100.0)
        } else {
            0.0
        };
        Ok(CommandOutput::Message(format!(
            "Context window:\n  Chat entries:    {}\n  Est. context:    ~{} tokens\n  API tokens used: {}\n  Window usage:    {:.1}%",
            ctx.chat_entries.len(),
            est_tokens,
            ctx.session.total_tokens,
            pct,
        )))
    }
}
