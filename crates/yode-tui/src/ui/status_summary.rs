use std::path::Path;

use ratatui::style::{Color, Style};
use ratatui::text::Span;
use yode_core::engine::{EngineRuntimeState, PromptCacheRuntimeState};

use crate::app::App;
use crate::runtime_display::format_tool_progress_summary;

use super::palette::{ERROR_COLOR, INFO_COLOR, LIGHT, MUTED, SEP, SUCCESS_COLOR, WARNING_COLOR};
use super::responsive::Density;

#[derive(Debug, Clone)]
pub(crate) struct StatusBadge {
    pub text: String,
    pub color: Color,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct RuntimeStatusSnapshot {
    pub state: Option<EngineRuntimeState>,
    pub running_tasks: usize,
    pub has_team_artifact: bool,
    pub has_live_artifact: bool,
    pub has_defer_artifact: bool,
}

#[derive(Debug, Clone, Copy)]
struct ContextMetrics {
    estimated_tokens: usize,
    context_window_tokens: usize,
    compaction_threshold_tokens: usize,
}

#[derive(Debug, Clone, Copy)]
struct CompactionMetrics {
    autocompact_disabled: bool,
    failures: u32,
    total: u32,
    auto: u32,
    manual: u32,
}

#[derive(Debug, Clone, Copy)]
struct MemoryMetrics {
    initialized: bool,
    updating: bool,
    updates: u32,
}

pub(crate) fn runtime_status_snapshot(app: &App) -> RuntimeStatusSnapshot {
    let (state, running_tasks) = app
        .engine
        .as_ref()
        .and_then(|engine| engine.try_lock().ok())
        .map(|engine| {
            let state = engine.runtime_state();
            let running_tasks = engine
                .runtime_tasks_snapshot()
                .into_iter()
                .filter(|task| matches!(task.status, yode_tools::RuntimeTaskStatus::Running))
                .count();
            (Some(state), running_tasks)
        })
        .unwrap_or((None, 0));
    runtime_status_snapshot_from_parts(Path::new(&app.session.working_dir), state, running_tasks)
}

pub(crate) fn runtime_status_snapshot_from_parts(
    working_dir: &Path,
    state: Option<EngineRuntimeState>,
    running_tasks: usize,
) -> RuntimeStatusSnapshot {
    RuntimeStatusSnapshot {
        state,
        running_tasks,
        has_team_artifact: crate::commands::artifact_nav::latest_agent_team_monitor_artifact(
            working_dir,
        )
        .is_some(),
        has_live_artifact: crate::commands::artifact_nav::latest_remote_live_session_artifact(
            working_dir,
        )
        .is_some(),
        has_defer_artifact: crate::commands::artifact_nav::latest_hook_deferred_artifact(
            working_dir,
        )
        .is_some(),
    }
}

pub(crate) fn push_badge(parts: &mut Vec<Span<'static>>, badge: StatusBadge) {
    parts.push(Span::styled(badge.text, Style::default().fg(badge.color)));
    parts.push(Span::styled("· ", Style::default().fg(SEP)));
}

pub(crate) fn context_badge(
    state: Option<&EngineRuntimeState>,
    fallback_context_tokens: usize,
    density: Density,
) -> StatusBadge {
    let metrics = state
        .and_then(context_metrics_from_state)
        .unwrap_or(ContextMetrics {
            estimated_tokens: fallback_context_tokens,
            context_window_tokens: 128_000,
            compaction_threshold_tokens: 96_000,
        });
    let pct = percentage_label(metrics.estimated_tokens, metrics.context_window_tokens);
    let threshold_ratio = if metrics.compaction_threshold_tokens > 0 {
        metrics.estimated_tokens as f64 / metrics.compaction_threshold_tokens as f64
    } else {
        0.0
    };
    let color =
        if threshold_ratio >= 1.0 || metrics.estimated_tokens >= metrics.context_window_tokens {
            ERROR_COLOR
        } else if threshold_ratio >= 0.85
            || metrics.estimated_tokens * 100 >= metrics.context_window_tokens * 75
        {
            WARNING_COLOR
        } else {
            LIGHT
        };
    let text = match density {
        Density::Wide => format!("ctx {} ", pct),
        Density::Medium => format!("ctx{} ", pct),
        Density::Narrow => format!("c{} ", pct),
    };
    StatusBadge { text, color }
}

pub(crate) fn compaction_badge(
    state: Option<&EngineRuntimeState>,
    density: Density,
) -> Option<StatusBadge> {
    let metrics = compaction_metrics_from_state(state?)?;
    let (text, color) = if metrics.autocompact_disabled {
        (
            match density {
                Density::Wide => "compact off ".to_string(),
                Density::Medium => "cmp off ".to_string(),
                Density::Narrow => "cmp! ".to_string(),
            },
            WARNING_COLOR,
        )
    } else if metrics.failures > 0 {
        (
            match density {
                Density::Wide => format!("compact !{} ", metrics.failures),
                Density::Medium => format!("cmp!{} ", metrics.failures),
                Density::Narrow => format!("!{} ", metrics.failures),
            },
            ERROR_COLOR,
        )
    } else {
        (
            match density {
                Density::Wide if metrics.total > 0 => {
                    format!("compact {}a/{}m ", metrics.auto, metrics.manual)
                }
                Density::Wide => "compact 0 ".to_string(),
                Density::Medium => format!("cmp{} ", metrics.total),
                Density::Narrow => format!("c{} ", metrics.total),
            },
            if metrics.total > 0 {
                SUCCESS_COLOR
            } else {
                MUTED
            },
        )
    };
    Some(StatusBadge { text, color })
}

pub(crate) fn memory_badge(
    state: Option<&EngineRuntimeState>,
    density: Density,
) -> Option<StatusBadge> {
    let metrics = memory_metrics_from_state(state?)?;
    if matches!(density, Density::Narrow) && !metrics.updating && !metrics.initialized {
        return None;
    }

    let (text, color) = if metrics.updating {
        (format!("mem {}* ", metrics.updates), INFO_COLOR)
    } else if metrics.initialized {
        (format!("mem {} ", metrics.updates), SUCCESS_COLOR)
    } else {
        (
            match density {
                Density::Wide | Density::Medium => "mem cold ".to_string(),
                Density::Narrow => "mem? ".to_string(),
            },
            MUTED,
        )
    };
    Some(StatusBadge { text, color })
}

pub(crate) fn prompt_cache_badge(
    state: Option<&EngineRuntimeState>,
    density: Density,
) -> Option<StatusBadge> {
    let cache = &state?.prompt_cache;
    prompt_cache_badge_from_state(cache, density)
}

pub(crate) fn turn_tool_badge(
    state: Option<&EngineRuntimeState>,
    fallback_turn_tool_count: u32,
    density: Density,
) -> Option<StatusBadge> {
    let tools = state
        .map(|state| state.current_turn_tool_calls)
        .unwrap_or(fallback_turn_tool_count);
    if tools == 0 {
        return None;
    }

    Some(StatusBadge {
        text: match density {
            Density::Wide => format!("{} tools ", tools),
            Density::Medium | Density::Narrow => format!("t{} ", tools),
        },
        color: if tools >= 25 {
            ERROR_COLOR
        } else if tools >= 15 {
            WARNING_COLOR
        } else {
            LIGHT
        },
    })
}

pub(crate) fn tool_progress_badge(
    state: Option<&EngineRuntimeState>,
    density: Density,
) -> Option<StatusBadge> {
    if matches!(density, Density::Narrow) {
        return None;
    }

    let summary = format_tool_progress_summary(
        state?.last_tool_progress_tool.as_deref(),
        state?.last_tool_progress_message.as_deref(),
        state?.last_tool_progress_at.as_deref(),
    );
    if summary == "none" {
        return None;
    }

    let max_chars = match density {
        Density::Wide => 40,
        Density::Medium => 24,
        Density::Narrow => 0,
    };
    Some(StatusBadge {
        text: format!("{} ", truncate_text(&summary, max_chars)),
        color: INFO_COLOR,
    })
}

pub(crate) fn runtime_family_badges(
    snapshot: &RuntimeStatusSnapshot,
    density: Density,
) -> Vec<StatusBadge> {
    let mut badges = Vec::new();
    if snapshot.has_team_artifact {
        badges.push(StatusBadge {
            text: runtime_flag_text("team", density),
            color: Color::LightCyan,
        });
    }
    if snapshot.has_live_artifact {
        badges.push(StatusBadge {
            text: runtime_flag_text("live", density),
            color: Color::LightGreen,
        });
    }
    if snapshot.has_defer_artifact {
        badges.push(StatusBadge {
            text: runtime_flag_text("defer", density),
            color: Color::Yellow,
        });
    }
    badges
}

pub(crate) fn session_runtime_summary_text(
    snapshot: &RuntimeStatusSnapshot,
    fallback_context_tokens: usize,
) -> String {
    let mut parts = Vec::new();
    if let Some(badge) = tool_progress_badge(snapshot.state.as_ref(), Density::Wide) {
        parts.push(compact_badge_text(&badge.text));
    }
    if let Some(badge) = turn_tool_badge(snapshot.state.as_ref(), 0, Density::Wide) {
        parts.push(compact_badge_text(&badge.text));
    }
    parts.push(compact_badge_text(
        &context_badge(snapshot.state.as_ref(), fallback_context_tokens, Density::Wide).text,
    ));
    if let Some(badge) = compaction_badge(snapshot.state.as_ref(), Density::Wide) {
        parts.push(compact_badge_text(&badge.text));
    }
    if let Some(badge) = memory_badge(snapshot.state.as_ref(), Density::Wide) {
        parts.push(compact_badge_text(&badge.text));
    }
    if let Some(badge) = prompt_cache_badge(snapshot.state.as_ref(), Density::Wide) {
        parts.push(compact_badge_text(&badge.text));
    }
    if snapshot.running_tasks > 0 {
        parts.push(compact_badge_text(&super::badges::task_badge_label(
            snapshot.running_tasks,
            Density::Wide,
        )));
    }
    for badge in runtime_family_badges(snapshot, Density::Wide) {
        parts.push(compact_badge_text(&badge.text));
    }
    if parts.is_empty() {
        "none".to_string()
    } else {
        parts.join(" · ")
    }
}

pub(crate) fn tool_runtime_summary_text(state: &EngineRuntimeState) -> String {
    format!(
        "session {} · turn {} · progress {} · trunc {} · parallel {}b/{}c",
        state.session_tool_calls_total,
        state.current_turn_tool_calls,
        state.tool_progress_event_count,
        state.tool_truncation_count,
        state.parallel_tool_batch_count,
        state.parallel_tool_call_count,
    )
}

pub(crate) fn context_window_summary_text(
    state: Option<&EngineRuntimeState>,
    fallback_context_tokens: usize,
) -> String {
    let metrics = state
        .and_then(context_metrics_from_state)
        .unwrap_or(ContextMetrics {
            estimated_tokens: fallback_context_tokens,
            context_window_tokens: 128_000,
            compaction_threshold_tokens: 96_000,
        });
    let mut parts = vec![
        format!(
            "ctx {}",
            percentage_label(metrics.estimated_tokens, metrics.context_window_tokens)
        ),
        format!(
            "{}/{}",
            short_tokens(metrics.estimated_tokens as u64),
            short_tokens(metrics.context_window_tokens as u64)
        ),
        format!(
            "compact {}",
            short_tokens(metrics.compaction_threshold_tokens as u64)
        ),
    ];
    if let Some(state) = state {
        parts.push(format!("messages {}", state.message_count));
        parts.push(format!("query {}", state.query_source));
    }
    parts.join(" · ")
}

fn context_metrics_from_state(state: &EngineRuntimeState) -> Option<ContextMetrics> {
    (state.context_window_tokens > 0).then_some(ContextMetrics {
        estimated_tokens: state.estimated_context_tokens,
        context_window_tokens: state.context_window_tokens,
        compaction_threshold_tokens: state.compaction_threshold_tokens,
    })
}

fn compaction_metrics_from_state(state: &EngineRuntimeState) -> Option<CompactionMetrics> {
    Some(CompactionMetrics {
        autocompact_disabled: state.autocompact_disabled,
        failures: state.compaction_failures,
        total: state.total_compactions,
        auto: state.auto_compactions,
        manual: state.manual_compactions,
    })
}

fn memory_metrics_from_state(state: &EngineRuntimeState) -> Option<MemoryMetrics> {
    Some(MemoryMetrics {
        initialized: state.live_session_memory_initialized,
        updating: state.live_session_memory_updating,
        updates: state.session_memory_update_count,
    })
}

fn prompt_cache_badge_from_state(
    cache: &PromptCacheRuntimeState,
    density: Density,
) -> Option<StatusBadge> {
    if matches!(density, Density::Narrow) {
        return None;
    }
    if cache.reported_turns == 0 {
        return None;
    }

    let read = cache.cache_read_tokens_total;
    let write = cache.cache_write_tokens_total;
    let (text, color) = if read == 0 && write == 0 {
        (
            match density {
                Density::Wide => "cache warm ".to_string(),
                Density::Medium => "warm ".to_string(),
                Density::Narrow => String::new(),
            },
            MUTED,
        )
    } else {
        let body = match (read > 0, write > 0) {
            (true, true) => format!("r{}/w{}", short_tokens(read), short_tokens(write)),
            (true, false) => format!("r{}", short_tokens(read)),
            (false, true) => format!("w{}", short_tokens(write)),
            (false, false) => "warm".to_string(),
        };
        (
            match density {
                Density::Wide => format!("cache {} ", body),
                Density::Medium => format!("{} ", body),
                Density::Narrow => String::new(),
            },
            INFO_COLOR,
        )
    };

    (!text.is_empty()).then_some(StatusBadge { text, color })
}

fn percentage_label(numerator: usize, denominator: usize) -> String {
    if denominator == 0 || numerator == 0 {
        return "0%".to_string();
    }
    let ratio = numerator as f64 / denominator as f64;
    if ratio > 0.0 && ratio < 0.01 {
        "<1%".to_string()
    } else {
        format!("{:.0}%", ratio * 100.0)
    }
}

fn short_tokens(tokens: u64) -> String {
    if tokens >= 1_000_000 {
        format!("{:.1}m", tokens as f64 / 1_000_000.0)
    } else if tokens >= 1_000 {
        format!("{:.1}k", tokens as f64 / 1_000.0)
    } else {
        tokens.to_string()
    }
}

fn truncate_text(text: &str, max_chars: usize) -> String {
    let count = text.chars().count();
    if count <= max_chars {
        return text.to_string();
    }
    let mut truncated = text
        .chars()
        .take(max_chars.saturating_sub(1))
        .collect::<String>();
    truncated.push('…');
    truncated
}

fn runtime_flag_text(label: &str, density: Density) -> String {
    match density {
        Density::Wide => format!("{} ", label),
        Density::Medium | Density::Narrow => {
            format!("{} ", label.chars().take(4).collect::<String>())
        }
    }
}

fn compact_badge_text(text: &str) -> String {
    text.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::{
        compaction_badge, context_window_summary_text, percentage_label,
        prompt_cache_badge_from_state, runtime_status_snapshot_from_parts,
        session_runtime_summary_text, short_tokens, tool_runtime_summary_text, truncate_text,
    };
    use crate::ui::responsive::Density;
    use yode_core::engine::{EngineRuntimeState, PromptCacheRuntimeState};
    use yode_tools::registry::ToolPoolSnapshot;

    fn test_runtime_state() -> EngineRuntimeState {
        EngineRuntimeState {
            query_source: "User".to_string(),
            autocompact_disabled: false,
            compaction_failures: 0,
            total_compactions: 0,
            auto_compactions: 0,
            manual_compactions: 0,
            last_compaction_breaker_reason: None,
            context_window_tokens: 128_000,
            compaction_threshold_tokens: 96_000,
            estimated_context_tokens: 64_000,
            message_count: 0,
            live_session_memory_initialized: false,
            live_session_memory_updating: false,
            live_session_memory_path: String::new(),
            session_tool_calls_total: 0,
            last_compaction_mode: None,
            last_compaction_at: None,
            last_compaction_summary_excerpt: None,
            last_compaction_session_memory_path: None,
            last_compaction_transcript_path: None,
            last_session_memory_update_at: None,
            last_session_memory_update_path: None,
            last_session_memory_generated_summary: false,
            session_memory_update_count: 0,
            tracked_failed_tool_results: 0,
            hook_total_executions: 0,
            hook_timeout_count: 0,
            hook_execution_error_count: 0,
            hook_nonzero_exit_count: 0,
            hook_wake_notification_count: 0,
            last_hook_failure_event: None,
            last_hook_failure_command: None,
            last_hook_failure_reason: None,
            last_hook_failure_at: None,
            last_hook_timeout_command: None,
            last_compaction_prompt_tokens: None,
            avg_compaction_prompt_tokens: None,
            compaction_cause_histogram: std::collections::BTreeMap::new(),
            system_prompt_estimated_tokens: 0,
            system_prompt_segments: Vec::new(),
            prompt_cache: PromptCacheRuntimeState::default(),
            last_turn_duration_ms: None,
            last_turn_stop_reason: None,
            last_turn_artifact_path: None,
            last_stream_watchdog_stage: None,
            stream_retry_reason_histogram: std::collections::BTreeMap::new(),
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
            tool_error_type_counts: std::collections::BTreeMap::new(),
            tool_trace_scope: "last".to_string(),
            tool_traces: Vec::new(),
        }
    }

    #[test]
    fn percentage_label_handles_sub_one_percent() {
        assert_eq!(percentage_label(100, 128_000), "<1%");
    }

    #[test]
    fn prompt_cache_badge_shows_read_write_totals() {
        let badge = prompt_cache_badge_from_state(
            &PromptCacheRuntimeState {
                reported_turns: 3,
                cache_read_tokens_total: 12_500,
                cache_write_tokens_total: 1_200,
                ..PromptCacheRuntimeState::default()
            },
            Density::Wide,
        )
        .unwrap();
        assert_eq!(badge.text, "cache r12.5k/w1.2k ");
        assert_eq!(badge.color, super::INFO_COLOR);
    }

    #[test]
    fn compaction_badge_highlights_disabled_state() {
        let mut state = test_runtime_state();
        state.autocompact_disabled = true;
        let badge = compaction_badge(Some(&state), Density::Wide).unwrap();
        assert_eq!(badge.text, "compact off ");
        assert_eq!(badge.color, super::WARNING_COLOR);
    }

    #[test]
    fn short_tokens_compacts_large_values() {
        assert_eq!(short_tokens(950), "950");
        assert_eq!(short_tokens(1_250), "1.2k");
        assert_eq!(short_tokens(2_500_000), "2.5m");
    }

    #[test]
    fn truncate_text_adds_ellipsis() {
        assert_eq!(truncate_text("abcdef", 4), "abc…");
    }

    #[test]
    fn session_runtime_summary_text_includes_core_badges() {
        let mut state = test_runtime_state();
        state.estimated_context_tokens = 72_000;
        state.total_compactions = 3;
        state.auto_compactions = 2;
        state.manual_compactions = 1;
        state.live_session_memory_initialized = true;
        state.session_memory_update_count = 4;
        state.current_turn_tool_calls = 2;

        let snapshot =
            runtime_status_snapshot_from_parts(std::path::Path::new("/tmp"), Some(state), 1);
        let summary = session_runtime_summary_text(&snapshot, 0);
        assert!(summary.contains("ctx 56%"));
        assert!(summary.contains("compact 2a/1m"));
        assert!(summary.contains("mem 4"));
        assert!(summary.contains("2 tools"));
        assert!(summary.contains("1 jobs"));
    }

    #[test]
    fn tool_runtime_summary_text_is_compact() {
        let mut state = test_runtime_state();
        state.session_tool_calls_total = 12;
        state.current_turn_tool_calls = 3;
        state.tool_progress_event_count = 5;
        state.tool_truncation_count = 1;
        state.parallel_tool_batch_count = 2;
        state.parallel_tool_call_count = 7;
        assert_eq!(
            tool_runtime_summary_text(&state),
            "session 12 · turn 3 · progress 5 · trunc 1 · parallel 2b/7c"
        );
    }

    #[test]
    fn context_window_summary_text_formats_counts() {
        let state = test_runtime_state();
        let summary = context_window_summary_text(Some(&state), 0);
        assert!(summary.contains("ctx 50%"));
        assert!(summary.contains("64.0k/128.0k"));
        assert!(summary.contains("compact 96.0k"));
        assert!(summary.contains("messages 0"));
        assert!(summary.contains("query User"));
    }
}
