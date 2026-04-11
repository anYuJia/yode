use super::mcp::parse_mcp_tool_name;
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
            let display_name = if let Some((server, tool)) = parse_mcp_tool_name(&def.name) {
                format!("{} [mcp:{}]", tool, server)
            } else {
                def.name.clone()
            };
            if verbose {
                lines.push(format!(
                    "  {} — {}\n    schema: {}",
                    display_name, def.description, def.parameters
                ));
            } else {
                lines.push(format!("  {} — {}", display_name, def.description));
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
        let failure_clusters = failure_cluster_summary(&state.tool_traces);
        let read_history = if state.read_file_history.is_empty() {
            "none".to_string()
        } else {
            state.read_file_history.join(" | ")
        };
        let duplication_hints = if state.command_tool_duplication_hints.is_empty() {
            "none".to_string()
        } else {
            state.command_tool_duplication_hints.join(" | ")
        };
        let hook_tool_timeline = format!(
            "{} hook run(s) / {} recent tool call(s)",
            state.hook_total_executions,
            state.tool_traces.len()
        );

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
                let diff_lines = trace
                    .diff_preview
                    .as_ref()
                    .map(|diff| diff.lines().count())
                    .unwrap_or(0);
                let preview = tool_output_preview_line(trace.output_preview.as_str());
                traces.push_str(&format!(
                    "    - {} [{}] {}ms batch={} progress={} err={} trunc={} diff_lines={} diff={} out={}\n",
                    trace.tool_name,
                    status,
                    trace.duration_ms,
                    trace
                        .parallel_batch
                        .map(|batch| batch.to_string())
                        .unwrap_or_else(|| "-".to_string()),
                    trace.progress_updates,
                    error,
                    truncation,
                    diff_lines,
                    diff.lines().next().unwrap_or(diff),
                    preview,
                ));
            }
        }

        Ok(CommandOutput::Message(format!(
            "Tool diagnostics:\n  Registry tools:  {}\n  Session calls:    {}\n  Current turn:     {} calls / {} bytes / {} progress\n  Budget notices:   {} (warnings {})\n  Budget active:    notice={} warning={}\n  Parallel:         {} batches / {} calls (max {})\n  Read history:     {}\n  Duplication hints: {}\n  Hook/tool line:   {}\n  Truncations:      {} (last: {})\n  Error types:      {}\n  Failure clusters: {}\n  Repeat failures:  {}\n  Last progress:    {} / {}\n  Last progress at: {}\n  Last artifact:    {}\n  Last turn done:   {}\n{}\
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
            read_history,
            duplication_hints,
            hook_tool_timeline,
            state.tool_truncation_count,
            state
                .last_tool_truncation_reason
                .as_deref()
                .unwrap_or("none"),
            error_counts,
            failure_clusters,
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

fn tool_output_preview_line(output: &str) -> String {
    let squashed = output
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("-")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if squashed.chars().count() <= 80 {
        squashed
    } else {
        format!("{}...", squashed.chars().take(80).collect::<String>())
    }
}

fn failure_cluster_summary(traces: &[yode_core::tool_runtime::ToolRuntimeCallView]) -> String {
    let mut counts = std::collections::BTreeMap::<String, u32>::new();
    for trace in traces.iter().filter(|trace| !trace.success) {
        let key = format!(
            "{}:{}",
            trace.tool_name,
            trace.error_type.as_deref().unwrap_or("unknown")
        );
        *counts.entry(key).or_insert(0) += 1;
    }
    if counts.is_empty() {
        return "none".to_string();
    }
    counts
        .into_iter()
        .map(|(key, count)| format!("{} x{}", key, count))
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::{failure_cluster_summary, tool_output_preview_line};

    #[test]
    fn tool_output_preview_line_uses_first_non_empty_line() {
        assert_eq!(
            tool_output_preview_line("\n  hello   world\nnext"),
            "hello world"
        );
    }

    #[test]
    fn failure_cluster_summary_groups_failed_tools() {
        let traces = vec![
            yode_core::tool_runtime::ToolRuntimeCallView {
                tool_name: "bash".to_string(),
                success: false,
                error_type: Some("Execution".to_string()),
                ..Default::default()
            },
            yode_core::tool_runtime::ToolRuntimeCallView {
                tool_name: "bash".to_string(),
                success: false,
                error_type: Some("Execution".to_string()),
                ..Default::default()
            },
        ];
        assert_eq!(failure_cluster_summary(&traces), "bash:Execution x2");
    }
}
