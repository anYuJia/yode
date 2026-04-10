use crate::commands::context::CommandContext;
use crate::commands::{Command, CommandCategory, CommandMeta, CommandOutput, CommandResult};
use chrono::{Local, NaiveDateTime};
use yode_tools::builtin::review_common::review_output_has_findings;

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
        let working_dir = std::path::PathBuf::from(&ctx.session.working_dir);
        let latest_review = latest_review_summary(&working_dir.join(".yode").join("reviews"));
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
            let memory_freshness =
                memory_freshness_label(state.last_session_memory_update_at.as_deref());
            let memory_pending = memory_update_pending(
                state.live_session_memory_updating,
                state.last_session_memory_update_at.as_deref(),
                state.last_tool_turn_completed_at.as_deref(),
            );
            let breaker_hint =
                compact_breaker_hint(state.last_compaction_breaker_reason.as_deref());
            let compaction_histogram =
                compaction_cause_histogram(&state.compaction_cause_histogram);
            let prompt_cache_last_turn = prompt_cache_last_turn_status(&state.prompt_cache);
            let prompt_cache_miss_turns = prompt_cache_miss_turns(&state.prompt_cache);
            let system_prompt_breakdown =
                system_prompt_segment_breakdown(&state.system_prompt_segments);
            format!(
                "\n\nCompact:\n  Query source:    {}\n  Autocompact:     {}\n  Compact fails:   {}\n  Compact count:   {} (auto {}, manual {})\n  Breaker reason:  {}\n  Breaker hint:    {}\n  Cause histogram: {}\n  Last compact:    {}\n  Compact at:      {}\n  Compact summary: {}\n  Last compact mem: {}\n  Last transcript: {}\n\nSystem Prompt:\n  Total est:       {} tokens\n{}\n\nPrompt Cache:\n  Last turn:       {}\n  Last tokens:     {} prompt / {} completion\n  Last cache:      {} write / {} read\n  Cache turns:     {} reported / {} hit / {} miss / {} fill\n  Cache tokens:    {} write / {} read\n\nMemory:\n  Live memory:     {}{}\n  Live memory file: {}\n  Memory updates:  {}\n  Last memory update: {}\n  Freshness:       {}\n  Pending update:  {}\n\nRecovery:\n  State:           {}\n  Single-step:     {}\n  Reanchor:        {}\n  Need guidance:   {}\n  Last signature:  {}\n  Breadcrumbs:     {}\n  Artifact:        {}\n  Last permission: {} [{}]\n  Permission why:  {}\n  Permission artifact: {}\n  Recent denials:  {}\n\nTools:\n  Session tools:   {}\n  Current turn:    {} calls / {} bytes\n  Budget notices:  {} (warning {})\n  Budget active:   notice={} warning={}\n  Progress events: {} (last: {} / {})\n  Parallel:        {} batches / {} calls (max {})\n  Truncations:     {} (last: {})\n  Error types:     {}\n  Repeat fail:     {}\n  Tool traces:     {} turn / {} calls\n  Tool artifact:   {}\n  Tool turn done:  {}\n  Failed tools:    {}\n  Always-allow:    {}\n\nReviews:\n  Latest review:   {}\n  Review status:   {}\n  Review preview:  {}\n\nHooks:\n  Hook runs:       {}\n  Hook timeouts:   {}\n  Hook exec errs:  {}\n  Hook exits!=0:   {}\n  Hook wakes:      {}\n  Last hook fail:  {}\n  Last hook at:    {}\n  Last hook timeout: {}",
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
                breaker_hint,
                compaction_histogram,
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
                state.system_prompt_estimated_tokens,
                system_prompt_breakdown,
                prompt_cache_last_turn,
                state.prompt_cache.last_turn_prompt_tokens.unwrap_or(0),
                state.prompt_cache.last_turn_completion_tokens.unwrap_or(0),
                state.prompt_cache.last_turn_cache_write_tokens.unwrap_or(0),
                state.prompt_cache.last_turn_cache_read_tokens.unwrap_or(0),
                state.prompt_cache.reported_turns,
                state.prompt_cache.cache_read_turns,
                prompt_cache_miss_turns,
                state.prompt_cache.cache_write_turns,
                state.prompt_cache.cache_write_tokens_total,
                state.prompt_cache.cache_read_tokens_total,
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
                memory_freshness,
                if memory_pending { "yes" } else { "no" },
                state.recovery_state,
                state.recovery_single_step_count,
                state.recovery_reanchor_count,
                state.recovery_need_user_guidance_count,
                state.last_failed_signature.as_deref().unwrap_or("none"),
                state
                    .recovery_breadcrumbs
                    .last()
                    .map(String::as_str)
                    .unwrap_or("none"),
                state
                    .last_recovery_artifact_path
                    .as_deref()
                    .unwrap_or("none"),
                state.last_permission_tool.as_deref().unwrap_or("none"),
                state.last_permission_action.as_deref().unwrap_or("none"),
                state
                    .last_permission_explanation
                    .as_deref()
                    .unwrap_or("none"),
                state
                    .last_permission_artifact_path
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
                latest_review
                    .as_ref()
                    .map(|summary| summary.path.display().to_string())
                    .unwrap_or_else(|| "none".to_string()),
                latest_review
                    .as_ref()
                    .map(|summary| summary.status)
                    .unwrap_or("none"),
                latest_review
                    .as_ref()
                    .map(|summary| summary.preview.as_str())
                    .unwrap_or("none"),
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

struct ReviewSummary {
    path: std::path::PathBuf,
    status: &'static str,
    preview: String,
}

fn latest_review_summary(dir: &std::path::Path) -> Option<ReviewSummary> {
    let path = latest_markdown_file(dir)?;
    let content = std::fs::read_to_string(&path).ok()?;
    let body = extract_review_result_body(&content).unwrap_or(content.as_str());
    let status = if body.trim().is_empty() {
        "unknown"
    } else if review_output_has_findings(body) {
        "findings"
    } else {
        "clean"
    };
    let preview = body
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with("```"))
        .take(2)
        .collect::<Vec<_>>()
        .join(" | ");
    let preview = if preview.is_empty() {
        "none".to_string()
    } else if preview.chars().count() > 160 {
        format!("{}...", preview.chars().take(160).collect::<String>())
    } else {
        preview
    };
    Some(ReviewSummary {
        path,
        status,
        preview,
    })
}

fn latest_markdown_file(dir: &std::path::Path) -> Option<std::path::PathBuf> {
    let mut entries = std::fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("md"))
        .collect::<Vec<_>>();
    entries.sort_by(|a, b| b.file_name().cmp(&a.file_name()));
    entries.into_iter().next()
}

fn extract_review_result_body(content: &str) -> Option<&str> {
    let start = content.find("```text\n")?;
    let body_start = start + "```text\n".len();
    let end = content[body_start..].find("\n```")?;
    Some(&content[body_start..body_start + end])
}

fn parse_runtime_timestamp(value: Option<&str>) -> Option<NaiveDateTime> {
    value.and_then(|value| NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S").ok())
}

fn memory_freshness_label(last_update_at: Option<&str>) -> &'static str {
    let Some(last_update) = parse_runtime_timestamp(last_update_at) else {
        return "unknown";
    };
    let age = Local::now().naive_local() - last_update;
    if age.num_minutes() <= 10 {
        "fresh"
    } else if age.num_minutes() <= 60 {
        "warm"
    } else {
        "stale"
    }
}

fn memory_update_pending(
    live_session_memory_updating: bool,
    last_session_memory_update_at: Option<&str>,
    last_tool_turn_completed_at: Option<&str>,
) -> bool {
    if live_session_memory_updating {
        return true;
    }
    let Some(last_tool_turn) = parse_runtime_timestamp(last_tool_turn_completed_at) else {
        return false;
    };
    let Some(last_memory_update) = parse_runtime_timestamp(last_session_memory_update_at) else {
        return true;
    };
    last_tool_turn > last_memory_update
}

fn compact_breaker_hint(reason: Option<&str>) -> &'static str {
    match reason {
        Some(reason) if reason.contains("compression made no changes") => {
            "Try /compact after a larger turn or clear older context."
        }
        Some(reason) if reason.contains("timeout") => {
            "Reduce turn scope before retrying compaction."
        }
        Some(_) => "Shorten the next turn or clear stale context before retrying.",
        None => "none",
    }
}

fn prompt_cache_last_turn_status(cache: &yode_core::engine::PromptCacheRuntimeState) -> &'static str {
    let Some(_) = cache.last_turn_prompt_tokens else {
        return "none";
    };
    let write = cache.last_turn_cache_write_tokens.unwrap_or(0);
    let read = cache.last_turn_cache_read_tokens.unwrap_or(0);
    match (write > 0, read > 0) {
        (true, true) => "hit+write",
        (true, false) => "miss+write",
        (false, true) => "hit",
        (false, false) => "miss",
    }
}

fn prompt_cache_miss_turns(cache: &yode_core::engine::PromptCacheRuntimeState) -> u32 {
    cache.reported_turns.saturating_sub(cache.cache_read_turns)
}

fn system_prompt_segment_breakdown(
    segments: &[yode_core::engine::SystemPromptSegmentRuntimeState],
) -> String {
    if segments.is_empty() {
        return "  Segments:       none".to_string();
    }

    segments
        .iter()
        .map(|segment| {
            format!(
                "  {}: {} tok / {} chars",
                segment.label, segment.estimated_tokens, segment.chars
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn compaction_cause_histogram(histogram: &std::collections::BTreeMap<String, u32>) -> String {
    if histogram.is_empty() {
        return "none".to_string();
    }

    histogram
        .iter()
        .map(|(cause, count)| format!("{}={}", cause, count))
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use chrono::Local;
    use yode_core::engine::{PromptCacheRuntimeState, SystemPromptSegmentRuntimeState};
    use super::{
        compact_breaker_hint, compaction_cause_histogram, latest_review_summary,
        memory_freshness_label, memory_update_pending, prompt_cache_last_turn_status,
        prompt_cache_miss_turns, system_prompt_segment_breakdown,
    };

    #[test]
    fn latest_review_summary_detects_clean_artifact() {
        let review_dir = std::env::temp_dir().join(format!(
            "yode-status-review-test-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&review_dir);
        std::fs::create_dir_all(&review_dir).unwrap();
        std::fs::write(
            review_dir.join("review-20260101.md"),
            "# Review Artifact\n\n## Result\n\n```text\nNo issues found.\nResidual risk: none.\n```\n",
        )
        .unwrap();

        let summary = latest_review_summary(&review_dir).unwrap();
        assert_eq!(summary.status, "clean");
        assert!(summary.preview.contains("No issues found."));
        let _ = std::fs::remove_dir_all(&review_dir);
    }

    #[test]
    fn memory_helpers_surface_freshness_and_pending() {
        let now = Local::now().naive_local();
        let fresh = now.format("%Y-%m-%d %H:%M:%S").to_string();
        let stale = (now - chrono::Duration::minutes(90))
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();
        assert_eq!(memory_freshness_label(Some(&fresh)), "fresh");
        assert_eq!(memory_freshness_label(Some(&stale)), "stale");
        assert!(memory_update_pending(false, Some(&stale), Some(&fresh)));
        assert_eq!(
            compact_breaker_hint(Some("compression made no changes")),
            "Try /compact after a larger turn or clear older context."
        );
    }

    #[test]
    fn prompt_cache_helpers_surface_hit_and_miss_labels() {
        let hit = PromptCacheRuntimeState {
            last_turn_prompt_tokens: Some(1200),
            last_turn_completion_tokens: Some(180),
            last_turn_cache_write_tokens: Some(300),
            last_turn_cache_read_tokens: Some(200),
            reported_turns: 4,
            cache_write_turns: 1,
            cache_read_turns: 3,
            cache_write_tokens_total: 300,
            cache_read_tokens_total: 900,
        };
        assert_eq!(prompt_cache_last_turn_status(&hit), "hit+write");
        assert_eq!(prompt_cache_miss_turns(&hit), 1);

        let miss = PromptCacheRuntimeState {
            last_turn_prompt_tokens: Some(800),
            last_turn_completion_tokens: Some(120),
            last_turn_cache_write_tokens: Some(0),
            last_turn_cache_read_tokens: Some(0),
            reported_turns: 2,
            cache_write_turns: 0,
            cache_read_turns: 0,
            cache_write_tokens_total: 0,
            cache_read_tokens_total: 0,
        };
        assert_eq!(prompt_cache_last_turn_status(&miss), "miss");
        assert_eq!(prompt_cache_miss_turns(&miss), 2);
    }

    #[test]
    fn system_prompt_breakdown_formats_segment_lines() {
        let rendered = system_prompt_segment_breakdown(&[
            SystemPromptSegmentRuntimeState {
                label: "Base prompt".to_string(),
                chars: 4000,
                estimated_tokens: 1000,
            },
            SystemPromptSegmentRuntimeState {
                label: "Environment".to_string(),
                chars: 120,
                estimated_tokens: 30,
            },
        ]);

        assert!(rendered.contains("Base prompt: 1000 tok / 4000 chars"));
        assert!(rendered.contains("Environment: 30 tok / 120 chars"));
    }

    #[test]
    fn compaction_histogram_renders_counts() {
        let rendered = compaction_cause_histogram(&std::collections::BTreeMap::from([
            ("failed_no_change".to_string(), 2),
            ("success_auto".to_string(), 5),
        ]));

        assert!(rendered.contains("failed_no_change=2"));
        assert!(rendered.contains("success_auto=5"));
    }
}
