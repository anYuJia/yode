use crate::commands::context::CommandContext;
use crate::commands::{Command, CommandCategory, CommandMeta, CommandOutput, CommandResult};

use super::cost::estimate_cost;

pub struct StatusCommand {
    meta: CommandMeta,
}

impl StatusCommand {
    pub fn new() -> Self {
        Self {
            meta: CommandMeta {
                name: "status",
                description: "Show session status",
                aliases: &[],
                args: vec![],
                category: CommandCategory::Info,
                hidden: false,
            },
        }
    }
}

impl Command for StatusCommand {
    fn meta(&self) -> &CommandMeta {
        &self.meta
    }

    fn execute(&self, _args: &str, ctx: &mut CommandContext) -> CommandResult {
        let session_short = &ctx.session.session_id[..ctx.session.session_id.len().min(8)];
        let always_allow = if ctx.session.always_allow_tools.is_empty() {
            "none".to_string()
        } else {
            ctx.session.always_allow_tools.join(", ")
        };
        let cost = estimate_cost(
            &ctx.session.model,
            ctx.session.input_tokens,
            ctx.session.output_tokens,
        );
        let runtime = ctx
            .engine
            .try_lock()
            .ok()
            .map(|engine| engine.runtime_state());
        let runtime_sections = if let Some(state) = runtime {
            format!(
                "\n\nCompact:\n  Query source:    {}\n  Autocompact:     {}\n  Compact fails:   {}\n  Compact count:   {} (auto {}, manual {})\n  Breaker reason:  {}\n  Last compact:    {}\n  Compact at:      {}\n  Compact summary: {}\n  Last compact mem: {}\n  Last transcript: {}\n\nMemory:\n  Live memory:     {}{}\n  Live memory file: {}\n  Memory updates:  {}\n  Last memory update: {}\n\nTools:\n  Session tools:   {}\n  Failed tools:    {}\n  Always-allow:    {}",
                state.query_source,
                if state.autocompact_disabled {
                    "disabled"
                } else {
                    "enabled"
                },
                state.compaction_failures,
                state.total_compactions,
                state.auto_compactions,
                state.manual_compactions,
                state
                    .last_compaction_breaker_reason
                    .as_deref()
                    .unwrap_or("none"),
                state
                    .last_compaction_mode
                    .as_deref()
                    .unwrap_or("none"),
                state
                    .last_compaction_at
                    .as_deref()
                    .unwrap_or("none"),
                state
                    .last_compaction_summary_excerpt
                    .as_deref()
                    .unwrap_or("none"),
                state
                    .last_compaction_session_memory_path
                    .as_deref()
                    .unwrap_or("none"),
                state
                    .last_compaction_transcript_path
                    .as_deref()
                    .unwrap_or("none"),
                if state.live_session_memory_initialized {
                    "initialized"
                } else {
                    "cold"
                },
                if state.live_session_memory_updating {
                    " (updating)"
                } else {
                    ""
                },
                state.live_session_memory_path,
                state.session_memory_update_count,
                state.last_session_memory_update_path.as_ref().map(|path| {
                    format!(
                        "{} ({}, {})",
                        path,
                        state
                            .last_session_memory_update_at
                            .as_deref()
                            .unwrap_or("unknown time"),
                        if state.last_session_memory_generated_summary {
                            "summary"
                        } else {
                            "snapshot"
                        }
                    )
                }).unwrap_or_else(|| "none".to_string()),
                state.session_tool_calls_total,
                state.tracked_failed_tool_results,
                always_allow,
            )
        } else {
            format!(
                "\n\nCompact:\n  Runtime state:   engine busy\n\nMemory:\n  Runtime state:   engine busy\n\nTools:\n  Always-allow:    {}",
                always_allow,
            )
        };

        Ok(CommandOutput::Message(format!(
            "Session status:\n  Session:         {}\n  Model:           {}\n  Working dir:     {}\n  Permission mode: {}\n  Tokens:          {} (in: {}, out: {})\n  Tool calls:      {}\n  Est. cost:       ${:.4}\n  Terminal:        {}{}",
            session_short,
            ctx.session.model,
            ctx.session.working_dir,
            ctx.session.permission_mode.label(),
            ctx.session.total_tokens,
            ctx.session.input_tokens,
            ctx.session.output_tokens,
            ctx.session.tool_call_count,
            cost,
            ctx.terminal_caps.summary(),
            runtime_sections,
        )))
    }
}
