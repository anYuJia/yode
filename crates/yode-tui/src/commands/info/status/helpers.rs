use crate::commands::dev::review_workspace::{
    compact_review_status_badge, extract_review_result_body,
};
use crate::commands::info::shared;

pub(super) struct ReviewSummary {
    pub path: std::path::PathBuf,
    pub status: &'static str,
    pub preview: String,
}

pub(super) fn latest_review_summary(dir: &std::path::Path) -> Option<ReviewSummary> {
    let path = latest_markdown_file(dir)?;
    let content = std::fs::read_to_string(&path).ok()?;
    let body = extract_review_result_body(&content).unwrap_or(content.as_str());
    let status = match compact_review_status_badge(&content) {
        "find" => "findings",
        "clean" => "clean",
        _ => "unknown",
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

pub(super) fn memory_freshness_label(last_update_at: Option<&str>) -> &'static str {
    shared::memory_freshness_label(last_update_at)
}

pub(super) fn memory_update_pending(
    live_session_memory_updating: bool,
    last_session_memory_update_at: Option<&str>,
    last_tool_turn_completed_at: Option<&str>,
) -> bool {
    if live_session_memory_updating {
        return true;
    }
    let Some(last_tool_turn) = shared::parse_runtime_timestamp(last_tool_turn_completed_at) else {
        return false;
    };
    let Some(last_memory_update) = shared::parse_runtime_timestamp(last_session_memory_update_at)
    else {
        return true;
    };
    last_tool_turn > last_memory_update
}

pub(crate) fn compact_breaker_hint(reason: Option<&str>) -> &'static str {
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

pub(super) fn prompt_cache_last_turn_status(
    cache: &yode_core::engine::PromptCacheRuntimeState,
) -> String {
    let Some(_) = cache.last_turn_prompt_tokens else {
        return "none".to_string();
    };
    let write = cache.last_turn_cache_write_tokens.unwrap_or(0);
    let read = cache.last_turn_cache_read_tokens.unwrap_or(0);
    let mut status = match (write > 0, read > 0) {
        (true, true) => "hit+write".to_string(),
        (true, false) => "miss+write".to_string(),
        (false, true) => "hit".to_string(),
        (false, false) => "miss".to_string(),
    };
    if cache.last_turn_cache_edit_deletions.unwrap_or(0) > 0
        || cache.last_turn_cache_deleted_tokens.unwrap_or(0) > 0
    {
        status.push_str("+edit");
    }
    status
}

pub(super) fn prompt_cache_miss_turns(cache: &yode_core::engine::PromptCacheRuntimeState) -> u32 {
    cache.reported_turns.saturating_sub(cache.cache_read_turns)
}

pub(super) fn system_prompt_segment_breakdown(
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

pub(super) fn compaction_cause_histogram(
    histogram: &std::collections::BTreeMap<String, u32>,
) -> String {
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
        let review_dir =
            std::env::temp_dir().join(format!("yode-status-review-test-{}", std::process::id()));
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
            last_turn_cache_edit_deletions: Some(2),
            last_turn_cache_deleted_tokens: Some(150),
            pending_cache_edit_refs: 0,
            pinned_cache_edit_refs: 0,
            pending_cache_edit_ref_values: Vec::new(),
            pinned_cache_edit_ref_values: Vec::new(),
            prompt_cache_break_count: 0,
            last_prompt_cache_break_reason: None,
            last_prompt_cache_break_at: None,
            last_prompt_cache_expected_drop_reason: None,
            last_prompt_cache_change_summary: None,
            last_prompt_cache_transition_kind: None,
            last_prompt_cache_transition_reason: None,
            last_prompt_cache_prefix_hash: None,
            last_prompt_cache_system_hash: None,
            last_prompt_cache_restore_hash: None,
            last_prompt_cache_tool_hash: None,
            last_prompt_cache_message_hash: None,
            last_prompt_cache_diff_artifact_path: None,
            last_prompt_cache_diff_summary: None,
            reported_turns: 4,
            cache_write_turns: 1,
            cache_read_turns: 3,
            cache_edit_turns: 0,
            cache_write_tokens_total: 300,
            cache_read_tokens_total: 900,
            cache_edit_deletions_total: 0,
            cache_deleted_tokens_total: 0,
        };
        assert_eq!(prompt_cache_last_turn_status(&hit), "hit+write+edit");
        assert_eq!(prompt_cache_miss_turns(&hit), 1);

        let miss = PromptCacheRuntimeState {
            last_turn_prompt_tokens: Some(800),
            last_turn_completion_tokens: Some(120),
            last_turn_cache_write_tokens: Some(0),
            last_turn_cache_read_tokens: Some(0),
            last_turn_cache_edit_deletions: Some(0),
            last_turn_cache_deleted_tokens: Some(0),
            pending_cache_edit_refs: 0,
            pinned_cache_edit_refs: 0,
            pending_cache_edit_ref_values: Vec::new(),
            pinned_cache_edit_ref_values: Vec::new(),
            prompt_cache_break_count: 0,
            last_prompt_cache_break_reason: None,
            last_prompt_cache_break_at: None,
            last_prompt_cache_expected_drop_reason: None,
            last_prompt_cache_change_summary: None,
            last_prompt_cache_transition_kind: None,
            last_prompt_cache_transition_reason: None,
            last_prompt_cache_prefix_hash: None,
            last_prompt_cache_system_hash: None,
            last_prompt_cache_restore_hash: None,
            last_prompt_cache_tool_hash: None,
            last_prompt_cache_message_hash: None,
            last_prompt_cache_diff_artifact_path: None,
            last_prompt_cache_diff_summary: None,
            reported_turns: 2,
            cache_write_turns: 0,
            cache_read_turns: 0,
            cache_edit_turns: 0,
            cache_write_tokens_total: 0,
            cache_read_tokens_total: 0,
            cache_edit_deletions_total: 0,
            cache_deleted_tokens_total: 0,
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
