use crate::commands::context::CommandContext;
use crate::commands::info::startup_artifacts::ProviderInventorySummary;
use crate::runtime_display::{
    fold_recovery_breadcrumbs, format_permission_decision_summary,
    format_repeated_tool_failure_summary, format_tool_progress_summary,
};
use crate::runtime_timeline::build_runtime_timeline_lines;
use yode_tools::registry::ToolInventory;
use yode_tools::RuntimeTask;

use super::super::artifact_preview::compact_tool_runtime_summary;
use super::helpers::{
    compact_breaker_hint, compaction_cause_histogram, memory_freshness_label,
    memory_update_pending, prompt_cache_last_turn_status, prompt_cache_miss_turns,
    system_prompt_segment_breakdown, ReviewSummary,
};
use super::sections::{artifact_links_section, busy_runtime_sections, reviews_section, StatusArtifactLinks};

pub(super) fn build_runtime_sections(
    runtime: Option<yode_core::engine::EngineRuntimeState>,
    tasks: &[RuntimeTask],
    latest_review: Option<&ReviewSummary>,
    always_allow: &str,
    inventory: &ToolInventory,
    artifact_links: &StatusArtifactLinks,
) -> String {
    let Some(state) = runtime else {
        return busy_runtime_sections(always_allow, inventory);
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
    let recovery_breadcrumbs = fold_recovery_breadcrumbs(&state.recovery_breadcrumbs, 3);
    let permission_summary = format_permission_decision_summary(
        state.last_permission_tool.as_deref(),
        state.last_permission_action.as_deref(),
        state.last_permission_explanation.as_deref(),
    );
    let tool_progress_summary = format_tool_progress_summary(
        state.last_tool_progress_tool.as_deref(),
        state.last_tool_progress_message.as_deref(),
        state.last_tool_progress_at.as_deref(),
    );
    let repeated_failure_summary =
        format_repeated_tool_failure_summary(state.latest_repeated_tool_failure.as_deref());
    let timeline = build_runtime_timeline_lines(&state, tasks, 6)
        .into_iter()
        .map(|line| format!("  - {}", line))
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        "\n\nCompact:\n  Query source:    {}\n  Autocompact:     {}\n  Compact fails:   {}\n  Compact count:   {} (auto {}, manual {})\n  Breaker reason:  {}\n  Breaker hint:    {}\n  Cause histogram: {}\n  Last compact:    {}\n  Compact at:      {}\n  Compact summary: {}\n  Last compact mem: {}\n  Last transcript: {}\n\nSystem Prompt:\n  Total est:       {} tokens\n{}\n\nPrompt Cache:\n  Last turn:       {}\n  Last tokens:     {} prompt / {} completion\n  Last cache:      {} write / {} read\n  Cache turns:     {} reported / {} hit / {} miss / {} fill\n  Cache tokens:    {} write / {} read\n\nTurn Runtime:\n  Last turn:       {} ms\n  Stop reason:     {}\n  Turn artifact:   {}\n  Watchdog stage:  {}\n  Retry reasons:   {}\n\nMemory:\n  Live memory:     {}{}\n  Live memory file: {}\n  Memory updates:  {}\n  Last memory update: {}\n  Freshness:       {}\n  Pending update:  {}\n\nRecovery:\n  State:           {}\n  Single-step:     {}\n  Reanchor:        {}\n  Need guidance:   {}\n  Last signature:  {}\n  Breadcrumbs:     {}\n  Artifact:        {}\n  Permission:      {}\n  Permission artifact: {}\n  Recent denials:  {}\n\nTools:\n  Inventory:       {} total / {} active / {} deferred\n  Model pool:      {} active visible / {} active hidden\n  Deferred pool:   {} visible / {} hidden\n  Pool policy:     mode={} confirm={} deny={}\n  Visible sources: {} builtin / {} mcp\n  Search mode:     {}\n  Search reason:   {}\n  Activations:     {} (last: {})\n  Duplicate regs:  {} ({})\n  Session tools:   {}\n  Current turn:    {} calls / {} bytes\n  Budget notices:  {} (warning {})\n  Budget active:   notice={} warning={}\n  Progress events: {} ({})\n  Parallel:        {} batches / {} calls (max {})\n  Truncations:     {} (last: {})\n  Error types:     {}\n  Repeat fail:     {}\n  Tool traces:     {} turn / {} calls\n  Tool artifact:   {}\n  Tool turn done:  {}\n  Failed tools:    {}\n  Always-allow:    {}{}{}{}\n\nHooks:\n  Hook runs:       {}\n  Hook timeouts:   {}\n  Hook exec errs:  {}\n  Hook exits!=0:   {}\n  Hook wakes:      {}\n  Last hook fail:  {}\n  Last hook at:    {}\n  Last hook timeout: {}\n\nTimeline:\n{}",
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
        state.last_turn_duration_ms.unwrap_or(0),
        state.last_turn_stop_reason.as_deref().unwrap_or("none"),
        state.last_turn_artifact_path.as_deref().unwrap_or("none"),
        state.last_stream_watchdog_stage.as_deref().unwrap_or("none"),
        if state.stream_retry_reason_histogram.is_empty() {
            "none".to_string()
        } else {
            state
                .stream_retry_reason_histogram
                .iter()
                .map(|(reason, count)| format!("{}={}", reason, count))
                .collect::<Vec<_>>()
                .join(", ")
        },
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
        recovery_breadcrumbs,
        state
            .last_recovery_artifact_path
            .as_deref()
            .unwrap_or("none"),
        permission_summary,
        state
            .last_permission_artifact_path
            .as_deref()
            .unwrap_or("none"),
        if state.recent_permission_denials.is_empty() {
            "none".to_string()
        } else {
            state.recent_permission_denials.join(" | ")
        },
        inventory.total_count,
        inventory.active_count,
        inventory.deferred_count,
        state.tool_pool.visible_active_count(),
        state.tool_pool.hidden_active_count(),
        state.tool_pool.visible_deferred_count(),
        state.tool_pool.hidden_deferred_count(),
        state.tool_pool.permission_mode,
        state.tool_pool.confirm_count(),
        state.tool_pool.deny_count(),
        state.tool_pool.visible_builtin_count(),
        state.tool_pool.visible_mcp_count(),
        state.tool_pool.tool_search_enabled,
        state.tool_pool.tool_search_reason.as_deref().unwrap_or("none"),
        inventory.activation_count,
        inventory.last_activated_tool.as_deref().unwrap_or("none"),
        inventory.duplicate_registration_count,
        if inventory.duplicate_tool_names.is_empty() {
            "none".to_string()
        } else {
            inventory.duplicate_tool_names.join(" | ")
        },
        state.session_tool_calls_total,
        state.current_turn_tool_calls,
        state.current_turn_tool_output_bytes,
        state.tool_budget_notice_count,
        state.tool_budget_warning_count,
        state.current_turn_budget_notice_emitted,
        state.current_turn_budget_warning_emitted,
        state.tool_progress_event_count,
        tool_progress_summary,
        state.parallel_tool_batch_count,
        state.parallel_tool_call_count,
        state.max_parallel_batch_size,
        state.tool_truncation_count,
        state
            .last_tool_truncation_reason
            .as_deref()
            .unwrap_or("none"),
        tool_error_counts,
        repeated_failure_summary,
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
        reviews_section(latest_review),
        artifact_links_section(artifact_links),
        "",
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
        timeline,
    )
}

pub(super) fn build_status_message(
    ctx: &CommandContext,
    provider_section: &str,
    runtime_sections: &str,
    cost: f64,
    resume_warmup: &str,
    startup_profile: &str,
) -> String {
    let session_short = &ctx.session.session_id[..ctx.session.session_id.len().min(8)];
    let runtime_summary = ctx
        .engine
        .try_lock()
        .ok()
        .map(|engine| compact_tool_runtime_summary(&engine.runtime_state()))
        .unwrap_or_else(|| "engine busy".to_string());
    format!(
        "Session status:\n  Session:         {}\n  Model:           {}\n  Working dir:     {}\n  Permission mode: {}\n  Startup profile: {}\n  Runtime summary: {}\n  Tokens:          {} (in: {}, out: {})\n  Tool calls:      {}\n  Resume warmup:   {}\n  Est. cost:       ${:.4}\n  Terminal:        {}{}{}",
        session_short,
        ctx.session.model,
        ctx.session.working_dir,
        ctx.session.permission_mode.label(),
        startup_profile,
        runtime_summary,
        ctx.session.total_tokens,
        ctx.session.input_tokens,
        ctx.session.output_tokens,
        ctx.session.tool_call_count,
        resume_warmup,
        cost,
        ctx.terminal_caps.summary(),
        provider_section,
        runtime_sections,
    )
}

pub(super) fn build_provider_section(
    provider_name: &str,
    model: &str,
    provider_inventory: Option<&ProviderInventorySummary>,
) -> String {
    let Some(provider_inventory) = provider_inventory else {
        return format!(
            "\n\nProvider:\n  Selected:        {} / {}\n  Registered:      unavailable\n  Source mix:      unavailable\n  Selected source: unavailable\n  Capabilities:    unavailable",
            provider_name,
            model
        );
    };

    let selected_provider = provider_inventory
        .provider_details
        .iter()
        .find(|detail| detail.name == provider_name)
        .or_else(|| {
            provider_inventory
                .provider_details
                .iter()
                .find(|detail| detail.name == provider_inventory.provider_name)
        });
    let selected_source = selected_provider
        .map(|detail| {
            format!(
                "{} / models={} / {} / {} / {}",
                detail.format,
                detail.model_count,
                detail.registration_source,
                detail.api_key_source,
                detail.base_url_source
            )
        })
        .unwrap_or_else(|| "unknown".to_string());

    format!(
        "\n\nProvider:\n  Selected:        {} / {}\n  Registered:      {} total / {} configured / {} env-detected\n  Source mix:      {}\n  Selected source: {}\n  Capabilities:    {}",
        provider_name,
        model,
        provider_inventory.total_registered,
        provider_inventory.configured_registered,
        provider_inventory.env_detected_registered,
        provider_inventory.source_breakdown.compact_label(),
        selected_source,
        if provider_inventory.capability_summary.is_empty() {
            "none"
        } else {
            provider_inventory.capability_summary.as_str()
        },
    )
}
