use crate::commands::context::CommandContext;
use crate::commands::{Command, CommandCategory, CommandMeta, CommandOutput, CommandResult};

pub struct DiagnosticsCommand {
    meta: CommandMeta,
}

impl DiagnosticsCommand {
    pub fn new() -> Self {
        Self {
            meta: CommandMeta {
                name: "diagnostics",
                description: "Show a unified diagnostics overview",
                aliases: &["diag"],
                args: vec![],
                category: CommandCategory::Info,
                hidden: false,
            },
        }
    }
}

impl Command for DiagnosticsCommand {
    fn meta(&self) -> &CommandMeta {
        &self.meta
    }

    fn execute(&self, _args: &str, ctx: &mut CommandContext) -> CommandResult {
        let runtime = ctx
            .engine
            .try_lock()
            .ok()
            .map(|engine| (engine.runtime_state(), engine.runtime_tasks_snapshot()));
        let Some((state, tasks)) = runtime else {
            return Ok(CommandOutput::Message(
                "Diagnostics unavailable: engine busy.".to_string(),
            ));
        };

        let running_tasks = tasks
            .iter()
            .filter(|task| matches!(task.status, yode_tools::RuntimeTaskStatus::Running))
            .count();
        let recent_denials = if state.recent_permission_denials.is_empty() {
            "none".to_string()
        } else {
            state.recent_permission_denials.join(" | ")
        };
        let tool_errors = if state.tool_error_type_counts.is_empty() {
            "none".to_string()
        } else {
            state
                .tool_error_type_counts
                .iter()
                .map(|(kind, count)| format!("{}={}", kind, count))
                .collect::<Vec<_>>()
                .join(", ")
        };

        Ok(CommandOutput::Message(format!(
            "Diagnostics overview:\n\nContext:\n  Query source:   {}\n  Compact count:  {} (auto {}, manual {})\n  Breaker reason: {}\n  Compact tokens: {}\n\nMemory:\n  Live memory:    {}{}\n  Memory updates: {}\n  Last memory:    {}\n\nRecovery:\n  State:          {}\n  Last signature: {}\n  Last permission: {} [{}]\n  Denials:        {}\n\nTools:\n  Session calls:  {}\n  Progress:       {}\n  Parallel:       {} batches / {} calls\n  Truncations:    {}\n  Errors:         {}\n  Last artifact:  {}\n\nTasks:\n  Total:          {}\n  Running:        {}\n\nHooks:\n  Total runs:     {}\n  Timeouts:       {}\n  Wake notices:   {}",
            state.query_source,
            state.total_compactions,
            state.auto_compactions,
            state.manual_compactions,
            state
                .last_compaction_breaker_reason
                .as_deref()
                .unwrap_or("none"),
            state
                .last_compaction_prompt_tokens
                .map(|value| value.to_string())
                .unwrap_or_else(|| "none".to_string()),
            if state.live_session_memory_initialized {
                "warm"
            } else {
                "cold"
            },
            if state.live_session_memory_updating {
                " (updating)"
            } else {
                ""
            },
            state.session_memory_update_count,
            state
                .last_session_memory_update_path
                .as_deref()
                .unwrap_or("none"),
            state.recovery_state,
            state.last_failed_signature.as_deref().unwrap_or("none"),
            state.last_permission_tool.as_deref().unwrap_or("none"),
            state.last_permission_action.as_deref().unwrap_or("none"),
            recent_denials,
            state.session_tool_calls_total,
            state.tool_progress_event_count,
            state.parallel_tool_batch_count,
            state.parallel_tool_call_count,
            state.tool_truncation_count,
            tool_errors,
            state
                .last_tool_turn_artifact_path
                .as_deref()
                .unwrap_or("none"),
            tasks.len(),
            running_tasks,
            state.hook_total_executions,
            state.hook_timeout_count,
            state.hook_wake_notification_count,
        )))
    }
}
