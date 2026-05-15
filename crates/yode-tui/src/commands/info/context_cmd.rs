use crate::commands::context::CommandContext;
use crate::commands::info::status::helpers::compact_breaker_hint;
use crate::commands::{Command, CommandCategory, CommandMeta, CommandOutput, CommandResult};
use crate::display_text::compact_path_tail;
use crate::ui::status_summary::{context_window_summary_text, tool_runtime_summary_text};
use yode_core::engine::{EngineRuntimeState, PromptCacheRuntimeState, RestoreBudgetRuntimeState};

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
            .unwrap_or((context_window as f64 * 0.93) as usize);
        let pct = (est_tokens as f64 / context_window as f64 * 100.0).min(100.0);
        let runtime_lines = if let Some(state) = runtime {
            render_runtime_context_lines(&state, total_chars / 4, pct, threshold)
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

fn render_runtime_context_lines(
    state: &EngineRuntimeState,
    fallback_tokens: usize,
    pct: f64,
    threshold: usize,
) -> String {
    let prompt_cache = prompt_cache_summary(&state.prompt_cache);
    let compact_artifacts = compact_artifact_summary(state);
    format!(
        "\n  Summary:         {}\n  Messages:        {}\n  Compaction line: ~{} tokens\n  Pressure:        {}\n  Post-compact:    {}\n  Restore budget:  {}\n  Suggestions:     {}\n  Query source:    {}\n  Autocompact:     {}\n  Compact count:   {} (auto {}, manual {})\n  Breaker reason:  {}\n  Hint:            {}\n  Last compact:    {}\n  Media compact:   last {} / total {} removed, saved ~{} chars\n  Compact files:   {}\n  Prompt cache:    {}\n  Live memory:     {}\n  Tool runtime:    {}\n  Memory updates:  {}",
        context_window_summary_text(Some(state), fallback_tokens),
        state.message_count,
        state.compaction_threshold_tokens,
        compact_pressure_hint(state, pct, threshold),
        post_compact_pressure_summary(state),
        restore_budget_summary(state.last_restore_budget.as_ref()),
        context_suggestions_summary(state, pct, threshold),
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
        compact_breaker_hint(state.last_compaction_breaker_reason.as_deref()),
        last_compact_summary(state),
        state.last_microcompact_media_removed,
        state.microcompact_media_removed_total,
        state.microcompact_media_saved_chars_total,
        compact_artifacts,
        prompt_cache,
        state.live_session_memory_path,
        tool_runtime_summary_text(state),
        state.session_memory_update_count,
    )
}

fn restore_budget_summary(budget: Option<&RestoreBudgetRuntimeState>) -> String {
    let Some(budget) = budget else {
        return "none".to_string();
    };
    let truncated = budget
        .entries
        .iter()
        .filter(|entry| entry.truncated)
        .count();
    format!(
        "{}/{} tokens, {} blocks, {} truncated",
        budget.used_tokens,
        budget.total_tokens,
        budget.entries.len(),
        truncated
    )
}

fn context_suggestions_summary(state: &EngineRuntimeState, pct: f64, threshold: usize) -> String {
    let mut suggestions = Vec::new();

    if state.autocompact_disabled {
        suggestions
            .push("enable autocompact or run /compact before the next large turn".to_string());
    } else if state.estimated_context_tokens >= threshold {
        suggestions
            .push("run /compact now; the current context is at the auto threshold".to_string());
    } else if pct >= 85.0 {
        suggestions.push("keep the next turn tight; autocompact is close".to_string());
    }

    if state.current_turn_tool_output_bytes >= 100_000 {
        suggestions
            .push("large tool output this turn; prefer narrower reads or commands".to_string());
    } else if state.current_turn_truncated_results > 0 || state.tool_truncation_count > 0 {
        suggestions.push(
            "tool results were truncated; rerun narrower commands for exact details".to_string(),
        );
    }

    if state.read_file_history.len() >= 6 {
        suggestions
            .push("many files are in recent read history; focus reads on active files".to_string());
    }

    if let Some(segment) = state
        .system_prompt_segments
        .iter()
        .max_by_key(|segment| segment.estimated_tokens)
        .filter(|segment| segment.estimated_tokens >= threshold / 8)
    {
        suggestions.push(format!(
            "large system segment '{}' (~{} tokens); trim if it is stale",
            segment.label, segment.estimated_tokens
        ));
    }

    if suggestions.is_empty() {
        "none".to_string()
    } else {
        suggestions.truncate(3);
        suggestions.join("; ")
    }
}

fn post_compact_pressure_summary(state: &EngineRuntimeState) -> String {
    let Some(estimated) = state.last_post_compaction_estimated_tokens else {
        return "none".to_string();
    };
    let threshold = state
        .last_post_compaction_threshold_tokens
        .unwrap_or(state.compaction_threshold_tokens as u32);
    let delta = estimated as i64 - threshold as i64;
    let next_auto = match state.last_post_compaction_will_retrigger {
        Some(true) => "likely",
        Some(false) => "clear",
        None => "unknown",
    };
    format!(
        "est={} threshold={} delta={} next_auto={}",
        estimated, threshold, delta, next_auto
    )
}

fn last_compact_summary(state: &EngineRuntimeState) -> String {
    let mode = state.last_compaction_mode.as_deref().unwrap_or("none");
    let at = state.last_compaction_at.as_deref().unwrap_or("none");
    let summary = state
        .last_compaction_summary_excerpt
        .as_deref()
        .filter(|summary| !summary.trim().is_empty())
        .unwrap_or("no summary");
    format!("{} at {} · {}", mode, at, summary)
}

fn compact_artifact_summary(state: &EngineRuntimeState) -> String {
    let memory = state
        .last_compaction_session_memory_path
        .as_deref()
        .map(compact_path_display)
        .unwrap_or_else(|| "memory=none".to_string());
    let transcript = state
        .last_compaction_transcript_path
        .as_deref()
        .map(compact_path_display)
        .unwrap_or_else(|| "transcript=none".to_string());
    format!("{}; {}", memory, transcript)
}

fn compact_path_display(path: &str) -> String {
    compact_path_tail(path)
}

fn compact_pressure_hint(state: &EngineRuntimeState, pct: f64, threshold: usize) -> String {
    if state.autocompact_disabled {
        return "autocompact off · use /compact manually".to_string();
    }
    if state.estimated_context_tokens >= threshold {
        return "at threshold · run /compact now".to_string();
    }
    if pct >= 85.0 {
        return "approaching threshold · keep the next turn tight".to_string();
    }
    if pct >= 70.0 {
        return "healthy · still watch tool output and file reads".to_string();
    }
    "healthy".to_string()
}

fn prompt_cache_summary(cache: &PromptCacheRuntimeState) -> String {
    let last_turn = format!(
        "last prompt={} write={} read={} edit_del={}",
        cache
            .last_turn_prompt_tokens
            .map(|value| value.to_string())
            .unwrap_or_else(|| "-".to_string()),
        cache
            .last_turn_cache_write_tokens
            .map(|value| value.to_string())
            .unwrap_or_else(|| "-".to_string()),
        cache
            .last_turn_cache_read_tokens
            .map(|value| value.to_string())
            .unwrap_or_else(|| "-".to_string()),
        cache
            .last_turn_cache_edit_deletions
            .map(|value| value.to_string())
            .unwrap_or_else(|| "-".to_string())
    );
    let totals = format!(
        "totals write={} read={} deleted={}",
        cache.cache_write_tokens_total,
        cache.cache_read_tokens_total,
        cache.cache_deleted_tokens_total
    );
    let refs = format!(
        "refs pending={} pinned={}",
        cache.pending_cache_edit_refs, cache.pinned_cache_edit_refs
    );
    let breakage = if cache.prompt_cache_break_count > 0 {
        format!(
            "breaks={} last={}",
            cache.prompt_cache_break_count,
            cache
                .last_prompt_cache_break_reason
                .as_deref()
                .unwrap_or("unknown")
        )
    } else {
        "breaks=0".to_string()
    };
    let transition = cache
        .last_prompt_cache_transition_kind
        .as_deref()
        .unwrap_or("none");

    format!(
        "{}; {}; {}; {}; transition={}",
        last_turn, totals, refs, breakage, transition
    )
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{
        compact_artifact_summary, compact_pressure_hint, context_suggestions_summary,
        last_compact_summary, post_compact_pressure_summary, prompt_cache_summary,
        restore_budget_summary,
    };
    use yode_core::engine::{
        EngineRuntimeState, PromptCacheRuntimeState, RestoreBudgetEntryRuntimeState,
        RestoreBudgetRuntimeState, SystemPromptSegmentRuntimeState,
    };
    use yode_core::tool_runtime::ToolRuntimeCallView;
    use yode_tools::registry::ToolPoolSnapshot;

    fn state() -> EngineRuntimeState {
        EngineRuntimeState {
            query_source: "User".to_string(),
            autocompact_disabled: false,
            compaction_failures: 0,
            total_compactions: 1,
            auto_compactions: 1,
            manual_compactions: 0,
            last_compaction_breaker_reason: None,
            context_window_tokens: 128_000,
            compaction_threshold_tokens: 96_000,
            estimated_context_tokens: 64_000,
            message_count: 10,
            live_session_memory_initialized: true,
            live_session_memory_updating: false,
            live_session_memory_path: "/tmp/project/.yode/memory/live.md".to_string(),
            session_tool_calls_total: 0,
            last_compaction_mode: Some("auto".to_string()),
            last_compaction_at: Some("2026-05-12 12:00:00".to_string()),
            last_compaction_summary_excerpt: Some("kept plan and latest files".to_string()),
            last_compaction_session_memory_path: Some(
                "/tmp/project/.yode/memory/live.md".to_string(),
            ),
            last_compaction_transcript_path: Some(
                "/tmp/project/.yode/transcripts/compact.md".to_string(),
            ),
            last_compact_boundary: None,
            last_session_memory_update_at: None,
            last_session_memory_update_path: None,
            last_session_memory_generated_summary: false,
            session_memory_update_count: 1,
            tracked_failed_tool_results: 0,
            hook_total_executions: 0,
            hook_timeout_count: 0,
            hook_execution_error_count: 0,
            hook_nonzero_exit_count: 0,
            hook_wake_notification_count: 0,
            stop_hook_continue_count: 0,
            last_stop_hook_continue_reason: None,
            last_hook_failure_event: None,
            last_hook_failure_command: None,
            last_hook_failure_reason: None,
            last_hook_failure_at: None,
            last_hook_timeout_command: None,
            last_compaction_prompt_tokens: Some(96_000),
            last_post_compaction_estimated_tokens: None,
            last_post_compaction_threshold_tokens: None,
            last_post_compaction_will_retrigger: None,
            last_restore_budget: None,
            avg_compaction_prompt_tokens: Some(96_000),
            compaction_cause_histogram: BTreeMap::new(),
            last_microcompact_media_removed: 2,
            last_microcompact_media_saved_chars: 2048,
            microcompact_media_removed_total: 5,
            microcompact_media_saved_chars_total: 4096,
            system_prompt_estimated_tokens: 0,
            system_prompt_segments: Vec::new(),
            prompt_cache: PromptCacheRuntimeState::default(),
            cost: Default::default(),
            last_turn_duration_ms: None,
            last_turn_stop_reason: None,
            last_turn_artifact_path: None,
            last_stream_watchdog_stage: None,
            stream_retry_reason_histogram: BTreeMap::new(),
            recovery_state: "Normal".to_string(),
            recovery_single_step_count: 0,
            recovery_reanchor_count: 0,
            recovery_need_user_guidance_count: 0,
            last_failed_signature: None,
            recovery_breadcrumbs: Vec::new(),
            last_recovery_artifact_path: None,
            last_permission_tool: None,
            last_permission_action: None,
            last_permission_explanation: None,
            last_permission_artifact_path: None,
            recent_permission_denials: Vec::new(),
            tool_pool: ToolPoolSnapshot::default(),
            current_turn_tool_calls: 0,
            current_turn_tool_output_bytes: 0,
            current_turn_tool_progress_events: 0,
            current_turn_parallel_batches: 0,
            current_turn_parallel_calls: 0,
            current_turn_max_parallel_batch_size: 0,
            current_turn_truncated_results: 0,
            current_turn_budget_notice_emitted: false,
            current_turn_budget_warning_emitted: false,
            tool_budget_notice_count: 0,
            tool_budget_warning_count: 0,
            last_tool_budget_warning: None,
            tool_progress_event_count: 0,
            last_tool_progress_message: None,
            last_tool_progress_tool: None,
            last_tool_progress_at: None,
            parallel_tool_batch_count: 0,
            parallel_tool_call_count: 0,
            max_parallel_batch_size: 0,
            tool_truncation_count: 0,
            last_tool_truncation_reason: None,
            latest_repeated_tool_failure: None,
            read_file_history: Vec::new(),
            command_tool_duplication_hints: Vec::new(),
            last_tool_turn_completed_at: None,
            last_tool_turn_artifact_path: None,
            tool_error_type_counts: BTreeMap::new(),
            tool_trace_scope: "last".to_string(),
            tool_traces: Vec::<ToolRuntimeCallView>::new(),
        }
    }

    #[test]
    fn prompt_cache_summary_surfaces_cost_and_break_state() {
        let summary = prompt_cache_summary(&PromptCacheRuntimeState {
            last_turn_prompt_tokens: Some(1000),
            last_turn_cache_write_tokens: Some(400),
            last_turn_cache_read_tokens: Some(300),
            last_turn_cache_edit_deletions: Some(2),
            pending_cache_edit_refs: 3,
            pinned_cache_edit_refs: 1,
            prompt_cache_break_count: 1,
            last_prompt_cache_break_reason: Some("tools changed".to_string()),
            cache_write_tokens_total: 400,
            cache_read_tokens_total: 300,
            cache_deleted_tokens_total: 100,
            last_prompt_cache_transition_kind: Some("stable".to_string()),
            ..Default::default()
        });

        assert!(summary.contains("last prompt=1000 write=400 read=300 edit_del=2"));
        assert!(summary.contains("refs pending=3 pinned=1"));
        assert!(summary.contains("breaks=1 last=tools changed"));
        assert!(summary.contains("transition=stable"));
    }

    #[test]
    fn compact_summary_surfaces_last_compact_and_artifacts() {
        let state = state();
        let summary = last_compact_summary(&state);
        assert!(summary.contains("auto at 2026-05-12 12:00:00"));
        assert!(summary.contains("kept plan and latest files"));

        let artifacts = compact_artifact_summary(&state);
        assert!(artifacts.contains(".../memory/live.md"));
        assert!(artifacts.contains(".../transcripts/compact.md"));
    }

    #[test]
    fn compact_pressure_hint_reflects_threshold_and_autocompact_state() {
        let mut state = state();
        assert_eq!(compact_pressure_hint(&state, 50.0, 96_000), "healthy");
        state.autocompact_disabled = true;
        assert_eq!(
            compact_pressure_hint(&state, 50.0, 96_000),
            "autocompact off · use /compact manually"
        );
        state.autocompact_disabled = false;
        state.estimated_context_tokens = 100_000;
        assert_eq!(
            compact_pressure_hint(&state, 101.0, 96_000),
            "at threshold · run /compact now"
        );
    }

    #[test]
    fn post_compact_pressure_summary_surfaces_retrigger_risk() {
        let mut state = state();
        state.last_post_compaction_estimated_tokens = Some(100_000);
        state.last_post_compaction_threshold_tokens = Some(96_000);
        state.last_post_compaction_will_retrigger = Some(true);

        assert_eq!(
            post_compact_pressure_summary(&state),
            "est=100000 threshold=96000 delta=4000 next_auto=likely"
        );
    }

    #[test]
    fn post_compact_pressure_summary_surfaces_clear_headroom() {
        let mut state = state();
        state.last_post_compaction_estimated_tokens = Some(90_000);
        state.last_post_compaction_threshold_tokens = Some(96_000);
        state.last_post_compaction_will_retrigger = Some(false);

        assert_eq!(
            post_compact_pressure_summary(&state),
            "est=90000 threshold=96000 delta=-6000 next_auto=clear"
        );
    }

    #[test]
    fn restore_budget_summary_surfaces_usage_and_truncation() {
        let budget = RestoreBudgetRuntimeState {
            total_tokens: 4000,
            used_tokens: 2750,
            entries: vec![
                RestoreBudgetEntryRuntimeState {
                    kind: "files".to_string(),
                    used_tokens: 1400,
                    cap_tokens: 1400,
                    truncated: true,
                    reason: Some("per-block restore budget cap".to_string()),
                },
                RestoreBudgetEntryRuntimeState {
                    kind: "runtime".to_string(),
                    used_tokens: 200,
                    cap_tokens: 600,
                    truncated: false,
                    reason: None,
                },
            ],
        };

        assert_eq!(
            restore_budget_summary(Some(&budget)),
            "2750/4000 tokens, 2 blocks, 1 truncated"
        );
    }

    #[test]
    fn context_suggestions_summary_prioritizes_actionable_bloat_causes() {
        let mut state = state();
        state.estimated_context_tokens = 95_000;
        state.current_turn_tool_output_bytes = 150_000;
        state.read_file_history = vec![
            "a.rs".to_string(),
            "b.rs".to_string(),
            "c.rs".to_string(),
            "d.rs".to_string(),
            "e.rs".to_string(),
            "f.rs".to_string(),
        ];
        state.system_prompt_segments = vec![SystemPromptSegmentRuntimeState {
            label: "Tools".to_string(),
            chars: 60_000,
            estimated_tokens: 14_000,
        }];

        let summary = context_suggestions_summary(&state, 90.0, 96_000);

        assert!(summary.contains("keep the next turn tight"));
        assert!(summary.contains("large tool output"));
        assert!(summary.contains("many files"));
        assert!(!summary.contains("large system segment"));
    }
}
