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
        let runtime = ctx
            .engine
            .try_lock()
            .ok()
            .map(|engine| engine.runtime_state());
        let total_chars: usize = ctx.chat_entries.iter().map(|e| e.content.len()).sum();
        let est_tokens = runtime
            .as_ref()
            .map(|state| state.estimated_context_tokens)
            .unwrap_or(total_chars / 4);
        let context_window = runtime
            .as_ref()
            .map(|state| state.context_window_tokens)
            .unwrap_or(128_000);
        let threshold = runtime
            .as_ref()
            .map(|state| state.compaction_threshold_tokens)
            .unwrap_or((context_window as f64 * 0.75) as usize);
        let pct = (est_tokens as f64 / context_window as f64 * 100.0).min(100.0);
        let runtime_lines = if let Some(state) = runtime {
            format!(
                "\n  Messages:        {}\n  Compaction line: ~{} tokens\n  Query source:    {}\n  Autocompact:     {}\n  Compact count:   {} (auto {}, manual {})\n  Breaker reason:  {}\n  Live memory:     {}\n  Session tools:   {}\n  Memory updates:  {}",
                state.message_count,
                state.compaction_threshold_tokens,
                state.query_source,
                if state.autocompact_disabled {
                    "disabled"
                } else {
                    "enabled"
                },
                state.total_compactions,
                state.auto_compactions,
                state.manual_compactions,
                state
                    .last_compaction_breaker_reason
                    .as_deref()
                    .unwrap_or("none"),
                state.live_session_memory_path,
                state.session_tool_calls_total,
                state.session_memory_update_count,
            )
        } else {
            String::new()
        };
        Ok(CommandOutput::Message(format!(
            "Context window:\n  Chat entries:    {}\n  Est. context:    ~{} tokens\n  API tokens used: {}\n  Window size:     {} tokens\n  Compact at:      ~{} tokens\n  Window usage:    {:.1}%{}",
            ctx.chat_entries.len(),
            est_tokens,
            ctx.session.total_tokens,
            context_window,
            threshold,
            pct,
            runtime_lines,
        )))
    }
}
