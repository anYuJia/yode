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
            let tool_error_counts = if state.tool_error_type_counts.is_empty() {
                "none".to_string()
            } else {
                state
                    .tool_error_type_counts
                    .iter()
                    .map(|(kind, count)| format!("{}={}", kind, count))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            format!(
                "\n\nCompact:\n  Query source:    {}\n  Autocompact:     {}\n  Compact fails:   {}\n  Compact count:   {} (auto {}, manual {})\n  Breaker reason:  {}\n  Last compact:    {}\n  Compact at:      {}\n  Compact summary: {}\n  Last compact mem: {}\n  Last transcript: {}\n\nMemory:\n  Live memory:     {}{}\n  Live memory file: {}\n  Memory updates:  {}\n  Last memory update: {}\n\nRecovery:\n  State:           {}\n  Single-step:     {}\n  Reanchor:        {}\n  Need guidance:   {}\n  Last signature:  {}\n  Last permission: {} [{}]\n  Permission why:  {}\n  Recent denials:  {}\n\nTools:\n  Session tools:   {}\n  Current turn:    {} calls / {} bytes\n  Budget notices:  {} (warning {})\n  Budget active:   notice={} warning={}\n  Progress events: {} (last: {} / {})\n  Parallel:        {} batches / {} calls (max {})\n  Truncations:     {} (last: {})\n  Error types:     {}\n  Repeat fail:     {}\n  Tool traces:     {} turn / {} calls\n  Tool artifact:   {}\n  Tool turn done:  {}\n  Failed tools:    {}\n  Always-allow:    {}\n\nHooks:\n  Hook runs:       {}\n  Hook timeouts:   {}\n  Hook exec errs:  {}\n  Hook exits!=0:   {}\n  Hook wakes:      {}\n  Last hook fail:  {}\n  Last hook at:    {}\n  Last hook timeout: {}",
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
                state
                    .last_session_memory_update_path
                    .as_ref()
                    .map(|path| {
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
                    })
                    .unwrap_or_else(|| "none".to_string()),
                state.recovery_state,
                state.recovery_single_step_count,
                state.recovery_reanchor_count,
                state.recovery_need_user_guidance_count,
                state.last_failed_signature.as_deref().unwrap_or("none"),
                state.last_permission_tool.as_deref().unwrap_or("none"),
                state.last_permission_action.as_deref().unwrap_or("none"),
                state
                    .last_permission_explanation
                    .as_deref()
                    .unwrap_or("none"),
                if state.recent_permission_denials.is_empty() {
                    "none".to_string()
                } else {
                    state.recent_permission_denials.join(" | ")
                },
                state.session_tool_calls_total,
                state.current_turn_tool_calls,
                state.current_turn_tool_output_bytes,
                state.tool_budget_notice_count,
                state.tool_budget_warning_count,
                state.current_turn_budget_notice_emitted,
                state.current_turn_budget_warning_emitted,
                state.tool_progress_event_count,
                state
                    .last_tool_progress_tool
                    .as_deref()
                    .unwrap_or("none"),
                state
                    .last_tool_progress_message
                    .as_deref()
                    .unwrap_or("none"),
                state.parallel_tool_batch_count,
                state.parallel_tool_call_count,
                state.max_parallel_batch_size,
                state.tool_truncation_count,
                state
                    .last_tool_truncation_reason
                    .as_deref()
                    .unwrap_or("none"),
                tool_error_counts,
                state
                    .latest_repeated_tool_failure
                    .as_deref()
                    .unwrap_or("none"),
                state.tool_trace_scope,
                state.tool_traces.len(),
                state
                    .last_tool_turn_artifact_path
                    .as_deref()
                    .unwrap_or("none"),
                state
                    .last_tool_turn_completed_at
                    .as_deref()
                    .unwrap_or("none"),
                state.tracked_failed_tool_results,
                always_allow,
                state.hook_total_executions,
                state.hook_timeout_count,
                state.hook_execution_error_count,
                state.hook_nonzero_exit_count,
                state.hook_wake_notification_count,
                state
                    .last_hook_failure_command
                    .as_ref()
                    .map(|command| {
                        format!(
                            "{} [{}]: {}",
                            command,
                            state
                                .last_hook_failure_event
                                .as_deref()
                                .unwrap_or("unknown"),
                            state
                                .last_hook_failure_reason
                                .as_deref()
                                .unwrap_or("unknown")
                        )
                    })
                    .unwrap_or_else(|| "none".to_string()),
                state
                    .last_hook_failure_at
                    .as_deref()
                    .unwrap_or("none"),
                state
                    .last_hook_timeout_command
                    .as_deref()
                    .unwrap_or("none"),
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
