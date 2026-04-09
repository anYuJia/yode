use crate::commands::context::CommandContext;
use crate::commands::{
    ArgCompletionSource, ArgDef, Command, CommandCategory, CommandMeta, CommandOutput,
    CommandResult,
};

pub struct ToolsCommand {
    meta: CommandMeta,
}

impl ToolsCommand {
    pub fn new() -> Self {
        Self {
            meta: CommandMeta {
                name: "tools",
                description: "Show tool runtime diagnostics or list registered tools",
                aliases: &[],
                args: vec![ArgDef {
                    name: "view".to_string(),
                    required: false,
                    hint: "diag | list | verbose".to_string(),
                    completions: ArgCompletionSource::Static(vec![
                        "diag".to_string(),
                        "list".to_string(),
                        "verbose".to_string(),
                    ]),
                }],
                category: CommandCategory::Tools,
                hidden: false,
            },
        }
    }

    fn render_registry_list(&self, ctx: &mut CommandContext<'_>, verbose: bool) -> String {
        let defs = ctx.tools.definitions();
        let mut lines = vec![format!("Registered tools ({}):", defs.len())];
        for def in &defs {
            if verbose {
                lines.push(format!(
                    "  {} — {}\n    schema: {}",
                    def.name, def.description, def.parameters
                ));
            } else {
                lines.push(format!("  {} — {}", def.name, def.description));
            }
        }
        lines.join("\n")
    }
}

impl Command for ToolsCommand {
    fn meta(&self) -> &CommandMeta {
        &self.meta
    }

    fn execute(&self, args: &str, ctx: &mut CommandContext<'_>) -> CommandResult {
        let view = args.trim();
        if matches!(view, "list" | "verbose") {
            return Ok(CommandOutput::Message(
                self.render_registry_list(ctx, view == "verbose"),
            ));
        }

        let runtime = ctx
            .engine
            .try_lock()
            .ok()
            .map(|engine| engine.runtime_state());
        let Some(state) = runtime else {
            return Ok(CommandOutput::Message(
                "Tool diagnostics unavailable: engine busy. Use `/tools list` to inspect the registry."
                    .to_string(),
            ));
        };

        let error_counts = if state.tool_error_type_counts.is_empty() {
            "none".to_string()
        } else {
            state
                .tool_error_type_counts
                .iter()
                .map(|(kind, count)| format!("{}={}", kind, count))
                .collect::<Vec<_>>()
                .join(", ")
        };

        let mut traces = String::new();
        if state.tool_traces.is_empty() {
            traces.push_str("  Recent calls:    none\n");
        } else {
            traces.push_str(&format!(
                "  Recent calls:    {} turn / {} call(s)\n",
                state.tool_trace_scope,
                state.tool_traces.len()
            ));
            for trace in state.tool_traces.iter().take(8) {
                let status = if trace.success { "ok" } else { "error" };
                let error = trace.error_type.as_deref().unwrap_or("-");
                let truncation = trace
                    .truncation
                    .as_ref()
                    .map(|item| item.reason.as_str())
                    .unwrap_or("-");
                let diff = trace.diff_preview.as_deref().unwrap_or("-");
                traces.push_str(&format!(
                    "    - {} [{}] {}ms progress={} err={} trunc={} diff={}\n",
                    trace.tool_name,
                    status,
                    trace.duration_ms,
                    trace.progress_updates,
                    error,
                    truncation,
                    diff.lines().next().unwrap_or(diff),
                ));
            }
        }

        Ok(CommandOutput::Message(format!(
            "Tool diagnostics:\n  Registry tools:  {}\n  Session calls:    {}\n  Current turn:     {} calls / {} bytes / {} progress\n  Budget notices:   {} (warnings {})\n  Budget active:    notice={} warning={}\n  Parallel:         {} batches / {} calls (max {})\n  Truncations:      {} (last: {})\n  Error types:      {}\n  Repeat failures:  {}\n  Last progress:    {} / {}\n  Last progress at: {}\n  Last artifact:    {}\n  Last turn done:   {}\n{}\
\nUse `/tools list` or `/tools verbose` to inspect the full registry.",
            ctx.tools.definitions().len(),
            state.session_tool_calls_total,
            state.current_turn_tool_calls,
            state.current_turn_tool_output_bytes,
            state.current_turn_tool_progress_events,
            state.tool_budget_notice_count,
            state.tool_budget_warning_count,
            state.current_turn_budget_notice_emitted,
            state.current_turn_budget_warning_emitted,
            state.parallel_tool_batch_count,
            state.parallel_tool_call_count,
            state.max_parallel_batch_size,
            state.tool_truncation_count,
            state
                .last_tool_truncation_reason
                .as_deref()
                .unwrap_or("none"),
            error_counts,
            state
                .latest_repeated_tool_failure
                .as_deref()
                .unwrap_or("none"),
            state
                .last_tool_progress_tool
                .as_deref()
                .unwrap_or("none"),
            state
                .last_tool_progress_message
                .as_deref()
                .unwrap_or("none"),
            state
                .last_tool_progress_at
                .as_deref()
                .unwrap_or("none"),
            state
                .last_tool_turn_artifact_path
                .as_deref()
                .unwrap_or("none"),
            state
                .last_tool_turn_completed_at
                .as_deref()
                .unwrap_or("none"),
            traces,
        )))
    }
}
