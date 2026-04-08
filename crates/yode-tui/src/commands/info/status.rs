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
        let runtime_lines = if let Some(state) = runtime {
            format!(
                "\n  Query source:    {}\n  Autocompact:     {}\n  Compact fails:   {}\n  Live memory:     {}{}\n  Live memory file: {}\n  Last compact:    {}\n  Compact at:      {}\n  Compact summary: {}\n  Last compact mem: {}\n  Last transcript: {}\n  Last memory update: {}",
                state.query_source,
                if state.autocompact_disabled {
                    "disabled"
                } else {
                    "enabled"
                },
                state.compaction_failures,
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
                state
                    .last_compaction_mode
                    .unwrap_or_else(|| "none".to_string()),
                state
                    .last_compaction_at
                    .unwrap_or_else(|| "none".to_string()),
                state
                    .last_compaction_summary_excerpt
                    .unwrap_or_else(|| "none".to_string()),
                state
                    .last_compaction_session_memory_path
                    .unwrap_or_else(|| "none".to_string()),
                state
                    .last_compaction_transcript_path
                    .unwrap_or_else(|| "none".to_string()),
                state.last_session_memory_update_path.map(|path| {
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
            )
        } else {
            "\n  Runtime state:   engine busy".to_string()
        };

        Ok(CommandOutput::Message(format!(
            "Session status:\n  Session:         {}\n  Model:           {}\n  Working dir:     {}\n  Permission mode: {}\n  Tokens:          {} (in: {}, out: {})\n  Tool calls:      {}\n  Est. cost:       ${:.4}\n  Always-allow:    {}\n  Terminal:        {}{}",
            session_short,
            ctx.session.model,
            ctx.session.working_dir,
            ctx.session.permission_mode.label(),
            ctx.session.total_tokens,
            ctx.session.input_tokens,
            ctx.session.output_tokens,
            ctx.session.tool_call_count,
            cost,
            always_allow,
            ctx.terminal_caps.summary(),
            runtime_lines,
        )))
    }
}
