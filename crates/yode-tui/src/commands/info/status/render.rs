use crate::commands::context::CommandContext;

use super::helpers::{
    compact_breaker_hint, compaction_cause_histogram, memory_freshness_label,
    memory_update_pending, prompt_cache_last_turn_status, prompt_cache_miss_turns,
    system_prompt_segment_breakdown, ReviewSummary,
};

pub(super) fn build_runtime_sections(
    runtime: Option<yode_core::engine::EngineRuntimeState>,
    latest_review: Option<&ReviewSummary>,
    always_allow: &str,
) -> String {
    let Some(state) = runtime else {
        return format!(
            "\n\nCompact:\n  Runtime state:   engine busy\n\nMemory:\n  Runtime state:   engine busy\n\nTools:\n  Always-allow:    {}",
            always_allow,
        );
    };

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
    let memory_freshness = memory_freshness_label(state.last_session_memory_update_at.as_deref());
    let memory_pending = memory_update_pending(
        state.live_session_memory_updating,
        state.last_session_memory_update_at.as_deref(),
        state.last_tool_turn_completed_at.as_deref(),
    );
    let breaker_hint = compact_breaker_hint(state.last_compaction_breaker_reason.as_deref());
    let compaction_histogram = compaction_cause_histogram(&state.compaction_cause_histogram);
    let prompt_cache_last_turn = prompt_cache_last_turn_status(&state.prompt_cache);
    let prompt_cache_miss_turns = prompt_cache_miss_turns(&state.prompt_cache);
    let system_prompt_breakdown = system_prompt_segment_breakdown(&state.system_prompt_segments);

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
        state.last_compaction_mode.as_deref().unwrap_or("none"),
        state.last_compaction_at.as_deref().unwrap_or("none"),
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
        state.last_tool_progress_tool.as_deref().unwrap_or("none"),
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
            .map(|summary| summary.path.display().to_string())
            .unwrap_or_else(|| "none".to_string()),
        latest_review.map(|summary| summary.status).unwrap_or("none"),
        latest_review
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
        state.last_hook_failure_at.as_deref().unwrap_or("none"),
        state
            .last_hook_timeout_command
            .as_deref()
            .unwrap_or("none"),
    )
}

pub(super) fn build_status_message(
    ctx: &CommandContext,
    runtime_sections: &str,
    cost: f64,
    resume_warmup: &str,
) -> String {
    let session_short = &ctx.session.session_id[..ctx.session.session_id.len().min(8)];
    format!(
        "Session status:\n  Session:         {}\n  Model:           {}\n  Working dir:     {}\n  Permission mode: {}\n  Tokens:          {} (in: {}, out: {})\n  Tool calls:      {}\n  Resume warmup:   {}\n  Est. cost:       ${:.4}\n  Terminal:        {}{}",
        session_short,
        ctx.session.model,
        ctx.session.working_dir,
        ctx.session.permission_mode.label(),
        ctx.session.total_tokens,
        ctx.session.input_tokens,
        ctx.session.output_tokens,
        ctx.session.tool_call_count,
        resume_warmup,
        cost,
        ctx.terminal_caps.summary(),
        runtime_sections,
    )
}
