use chrono::{Duration, Local};

use super::{
    build_transcript_compare_output, extract_summary_preview, filtered_transcript_entries,
    fold_transcript_preview, latest_transcript, memory_entry_age, parse_compare_args,
    parse_date_range_filter, parse_latest_compare_target, parse_list_filter,
    parse_memory_document, read_transcript_metadata, render_memory_file, render_transcript_list,
    render_transcript_picker, resolve_compare_target, resolve_transcript_target,
    run_long_session_benchmark, truncate_for_display, warm_resume_transcript_caches, CompareArgs,
    CompareOptions, TranscriptListFilter, TranscriptMode, MAX_DISPLAY_CHARS,
};
use crate::commands::CommandOutput;

#[test]
fn latest_transcript_prefers_newest_filename() {
    let dir = std::env::temp_dir().join(format!("yode-memory-command-test-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("aaa-compact-20240101.md"), "old").unwrap();
    std::fs::write(dir.join("bbb-compact-20250101.md"), "new").unwrap();

    let latest = latest_transcript(&dir).unwrap();
    assert!(latest.ends_with("bbb-compact-20250101.md"));

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn truncate_for_display_appends_notice() {
    let text = "x".repeat(MAX_DISPLAY_CHARS + 100);
    let truncated = truncate_for_display(&text);
    assert!(truncated.contains("Truncated for display"));
    assert!(truncated.len() < text.len());
}

#[test]
fn fold_transcript_preview_preserves_summary_and_folds_messages() {
    let mut content = String::from(
        "# Compaction Transcript\n\n- Mode: auto\n- Timestamp: 2026-01-01 10:00:00\n\n## Summary Anchor\n\n```text\nsummary line\n```\n\n## Messages\n",
    );
    for i in 0..80 {
        content.push_str(&format!("### Message {}\n\n```text\nline {}\n```\n\n", i, i));
    }

    let folded = fold_transcript_preview(&content);
    assert!(folded.contains("## Summary Anchor"));
    assert!(folded.contains("## Messages Preview"));
    assert!(folded.contains("transcript preview folded"));
    assert!(folded.contains("Message 79"));
}

#[test]
fn warm_resume_transcript_caches_reports_warmed_entries() {
    let project_root = std::env::temp_dir().join(format!(
        "yode-memory-warmup-{}",
        uuid::Uuid::new_v4()
    ));
    let transcript_dir = project_root.join(".yode").join("transcripts");
    std::fs::create_dir_all(&transcript_dir).unwrap();
    std::fs::write(
        transcript_dir.join("aaa-compact-20260101.md"),
        "# Compaction Transcript\n\n- Mode: auto\n",
    )
    .unwrap();
    std::fs::write(
        transcript_dir.join("bbb-compact-20260102.md"),
        "# Compaction Transcript\n\n- Mode: manual\n",
    )
    .unwrap();

    let stats = warm_resume_transcript_caches(&project_root);
    assert_eq!(stats.transcript_count, 2);
    assert_eq!(stats.metadata_entries_warmed, 2);
    assert!(stats.latest_lookup_cached);

    std::fs::remove_dir_all(&project_root).ok();
}

#[test]
fn long_session_benchmark_reports_hot_and_cold_paths() {
    let project_root = std::env::temp_dir().join(format!(
        "yode-memory-bench-{}",
        uuid::Uuid::new_v4()
    ));
    let transcript_dir = project_root.join(".yode").join("transcripts");
    std::fs::create_dir_all(&transcript_dir).unwrap();
    std::fs::write(
        transcript_dir.join("aaa-compact-20260101.md"),
        "# Compaction Transcript\n\n- Mode: auto\n\n## Summary Anchor\n\n```text\nleft\n```\n\n## Messages\n\n### User\n\n```text\nhello\n```\n",
    )
    .unwrap();
    std::fs::write(
        transcript_dir.join("bbb-compact-20260102.md"),
        "# Compaction Transcript\n\n- Mode: manual\n\n## Summary Anchor\n\n```text\nright\n```\n\n## Messages\n\n### User\n\n```text\nworld\n```\n",
    )
    .unwrap();

    let report = run_long_session_benchmark(&project_root);
    assert_eq!(report.transcript_count, 2);
    assert!(report.compare_pair.is_some());
    assert!(report.compare_ms.is_some());

    std::fs::remove_dir_all(&project_root).ok();
}

#[test]
fn print_long_session_benchmark_snapshot() {
    let project_root = std::env::temp_dir().join(format!(
        "yode-memory-bench-snapshot-{}",
        uuid::Uuid::new_v4()
    ));
    let transcript_dir = project_root.join(".yode").join("transcripts");
    std::fs::create_dir_all(&transcript_dir).unwrap();
    for index in 0..120 {
        std::fs::write(
            transcript_dir.join(format!("snap-compact-202601{:02}.md", index + 1)),
            format!(
                "# Compaction Transcript\n\n- Mode: auto\n- Timestamp: 2026-01-{:02} 10:00:00\n\n## Summary Anchor\n\n```text\nsnapshot {}\n```\n\n## Messages\n\n### User\n\n```text\nhello {}\n```\n",
                (index % 28) + 1,
                index,
                index
            ),
        )
        .unwrap();
    }

    let report = run_long_session_benchmark(&project_root);
    println!("# Long Session Benchmark Snapshot");
    println!();
    println!("- Transcript count: {}", report.transcript_count);
    println!(
        "- Latest lookup: cold {} ms / hot {} ms",
        report.cold_latest_lookup_ms, report.hot_latest_lookup_ms
    );
    println!(
        "- Failed filter: cold {} ms / hot {} ms",
        report.cold_failed_filter_ms, report.hot_failed_filter_ms
    );
    println!(
        "- Resume warmup: {} ms / {} metadata",
        report.resume_warmup.duration_ms, report.resume_warmup.metadata_entries_warmed
    );
    if let Some(compare_ms) = report.compare_ms {
        println!(
            "- Compare latest pair: {} ms (summary-only={})",
            compare_ms,
            report.compare_summary_only.unwrap_or(false)
        );
    }

    std::fs::remove_dir_all(&project_root).ok();
}

#[test]
fn resolve_transcript_target_supports_index_and_filename() {
    let dir = std::env::temp_dir().join(format!(
        "yode-memory-command-resolve-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("aaa-compact-20240101.md"), "old").unwrap();
    std::fs::write(dir.join("bbb-compact-20250101.md"), "new").unwrap();

    let first = resolve_transcript_target(&dir, "1").unwrap();
    assert!(first.ends_with("bbb-compact-20250101.md"));

    let by_name = resolve_transcript_target(&dir, "aaa-compact-20240101.md").unwrap();
    assert!(by_name.ends_with("aaa-compact-20240101.md"));

    let listing = render_transcript_list(&dir, &TranscriptListFilter::default());
    assert!(listing.contains("  1. "));
    assert!(listing.contains("  2. "));

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn transcript_picker_includes_folded_summary_preview() {
    let dir = std::env::temp_dir().join(format!(
        "yode-memory-picker-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("aaa-compact-20240101.md"),
        "# Compaction Transcript\n\n- Mode: auto\n- Timestamp: 2026-01-01 10:00:00\n\n## Summary Anchor\n\n```text\nThis is a long summary preview for picker rendering.\n```\n",
    )
    .unwrap();

    let picker = render_transcript_picker(&dir);
    assert!(picker.contains("preview: This is a long summary preview"));

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn read_transcript_metadata_parses_header_fields() {
    let dir = std::env::temp_dir().join(format!(
        "yode-memory-command-meta-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("sample.md");
    std::fs::write(
        &path,
        "# Compaction Transcript\n\n- Session: abc\n- Mode: manual\n- Timestamp: 2026-01-01 10:00:00\n- Removed messages: 7\n- Tool results truncated: 2\n- Failed tool results: 1\n- Session memory path: .yode/memory/session.md\n- Files read: src/lib.rs (120 lines)\n- Files modified: src/main.rs\n\n## Summary Anchor\n",
    )
    .unwrap();

    let meta = read_transcript_metadata(&path).unwrap();
    assert_eq!(meta.mode.as_deref(), Some("manual"));
    assert_eq!(meta.timestamp.as_deref(), Some("2026-01-01 10:00:00"));
    assert_eq!(meta.removed, Some(7));
    assert_eq!(meta.truncated, Some(2));
    assert_eq!(meta.failed_tool_results, Some(1));
    assert_eq!(
        meta.session_memory_path.as_deref(),
        Some(".yode/memory/session.md")
    );
    assert_eq!(
        meta.files_read_summary.as_deref(),
        Some("src/lib.rs (120 lines)")
    );
    assert_eq!(meta.files_modified_summary.as_deref(), Some("src/main.rs"));
    assert!(meta.has_summary);

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn extract_summary_preview_reads_summary_anchor_block() {
    let content = "# Compaction Transcript\n\n## Summary Anchor\n\n```text\nFirst line\nSecond line\n```\n";
    let preview = extract_summary_preview(content).unwrap();
    assert!(preview.contains("First line"));
    assert!(preview.contains("Second line"));
}

#[test]
fn filtered_transcript_entries_supports_mode_filter() {
    let dir = std::env::temp_dir().join(format!(
        "yode-memory-command-filter-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("auto.md"),
        "# Compaction Transcript\n\n- Mode: auto\n- Timestamp: 2026-01-01 10:00:00\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("manual.md"),
        "# Compaction Transcript\n\n- Mode: manual\n- Timestamp: 2026-01-01 11:00:00\n",
    )
    .unwrap();

    let auto = filtered_transcript_entries(
        &dir,
        &TranscriptListFilter {
            mode: Some(TranscriptMode::Auto),
            ..Default::default()
        },
    );
    assert_eq!(auto.len(), 1);
    assert!(auto[0].ends_with("auto.md"));

    let manual_listing = render_transcript_list(
        &dir,
        &TranscriptListFilter {
            mode: Some(TranscriptMode::Manual),
            ..Default::default()
        },
    );
    assert!(manual_listing.contains("manual"));
    assert!(!manual_listing.contains("auto.md"));

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn filtered_transcript_entries_supports_summary_filter() {
    let dir = std::env::temp_dir().join(format!(
        "yode-memory-command-summary-filter-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("with-summary.md"),
        "# Compaction Transcript\n\n- Mode: auto\n- Timestamp: 2026-01-01 10:00:00\n\n## Summary Anchor\n\n```text\nsummary\n```\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("without-summary.md"),
        "# Compaction Transcript\n\n- Mode: manual\n- Timestamp: 2026-01-01 11:00:00\n",
    )
    .unwrap();

    let summary = filtered_transcript_entries(
        &dir,
        &TranscriptListFilter {
            require_summary: true,
            ..Default::default()
        },
    );
    assert_eq!(summary.len(), 1);
    assert!(summary[0].ends_with("with-summary.md"));

    let summary_listing = render_transcript_list(
        &dir,
        &TranscriptListFilter {
            require_summary: true,
            ..Default::default()
        },
    );
    assert!(summary_listing.contains("summary=yes"));
    assert!(!summary_listing.contains("without-summary.md"));

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn filtered_transcript_entries_supports_failed_filter() {
    let dir = std::env::temp_dir().join(format!(
        "yode-memory-command-failed-filter-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("failed.md"),
        "# Compaction Transcript\n\n- Mode: auto\n- Timestamp: 2026-01-01 10:00:00\n- Failed tool results: 2\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("clean.md"),
        "# Compaction Transcript\n\n- Mode: manual\n- Timestamp: 2026-01-01 11:00:00\n- Failed tool results: 0\n",
    )
    .unwrap();

    let failed = filtered_transcript_entries(
        &dir,
        &TranscriptListFilter {
            require_failed: true,
            ..Default::default()
        },
    );
    assert_eq!(failed.len(), 1);
    assert!(failed[0].ends_with("failed.md"));

    let failed_listing = render_transcript_list(
        &dir,
        &TranscriptListFilter {
            require_failed: true,
            ..Default::default()
        },
    );
    assert!(failed_listing.contains("failed=2"));
    assert!(!failed_listing.contains("clean.md"));

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn parse_list_filter_supports_date_ranges() {
    assert!(matches!(
        parse_list_filter("list 2026-01-01").unwrap(),
        TranscriptListFilter {
            date_range: Some(_),
            ..
        }
    ));
    assert!(matches!(
        parse_list_filter("list 2026-01-01..2026-01-03").unwrap(),
        TranscriptListFilter {
            date_range: Some(_),
            ..
        }
    ));
    assert!(matches!(
        parse_list_filter("list ..2026-01-03").unwrap(),
        TranscriptListFilter {
            date_range: Some(_),
            ..
        }
    ));
    assert!(matches!(
        parse_list_filter("list today").unwrap(),
        TranscriptListFilter {
            date_range: Some(_),
            ..
        }
    ));
    assert!(parse_list_filter("list 2026-01-03..2026-01-01").is_err());
    assert!(parse_list_filter("list nope").is_err());
}

#[test]
fn parse_date_range_filter_supports_open_ranges() {
    let range = parse_date_range_filter("2026-01-01..").unwrap();
    assert_eq!(
        format!("{:?}", range),
        "DateRangeFilter { start: Some(2026-01-01), end: None }"
    );

    let range = parse_date_range_filter("..2026-01-03").unwrap();
    assert_eq!(
        format!("{:?}", range),
        "DateRangeFilter { start: None, end: Some(2026-01-03) }"
    );
}

#[test]
fn filtered_transcript_entries_supports_date_range_filter() {
    let dir = std::env::temp_dir().join(format!(
        "yode-memory-command-date-filter-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("first.md"),
        "# Compaction Transcript\n\n- Mode: auto\n- Timestamp: 2026-01-01 10:00:00\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("second.md"),
        "# Compaction Transcript\n\n- Mode: manual\n- Timestamp: 2026-01-03 11:00:00\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("third.md"),
        "# Compaction Transcript\n\n- Mode: manual\n- Timestamp: 2026-01-05 11:00:00\n",
    )
    .unwrap();

    let filter = parse_list_filter("list 2026-01-02..2026-01-04").unwrap();
    let filtered = filtered_transcript_entries(&dir, &filter);
    assert_eq!(filtered.len(), 1);
    assert!(filtered[0].ends_with("second.md"));

    let listing = render_transcript_list(&dir, &filter);
    assert!(listing.contains("(2026-01-02..2026-01-04)"));
    assert!(listing.contains("second.md"));
    assert!(!listing.contains("first.md"));
    assert!(!listing.contains("third.md"));

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn parse_list_filter_supports_combined_flags() {
    let filter = parse_list_filter("list summary failed recent auto").unwrap();
    assert_eq!(
        filter,
        TranscriptListFilter {
            recent_limit: Some(5),
            mode: Some(TranscriptMode::Auto),
            require_summary: true,
            require_failed: true,
            date_range: None,
        }
    );
    assert!(parse_list_filter("list auto manual").is_err());
}

#[test]
fn filtered_transcript_entries_supports_combined_filters() {
    let dir = std::env::temp_dir().join(format!(
        "yode-memory-command-combo-filter-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("match.md"),
        "# Compaction Transcript\n\n- Mode: auto\n- Timestamp: 2026-01-03 10:00:00\n- Failed tool results: 2\n\n## Summary Anchor\n\n```text\nsummary\n```\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("no-failed.md"),
        "# Compaction Transcript\n\n- Mode: auto\n- Timestamp: 2026-01-03 11:00:00\n- Failed tool results: 0\n\n## Summary Anchor\n\n```text\nsummary\n```\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("no-summary.md"),
        "# Compaction Transcript\n\n- Mode: auto\n- Timestamp: 2026-01-03 12:00:00\n- Failed tool results: 1\n",
    )
    .unwrap();

    let filter = parse_list_filter("list auto summary failed").unwrap();
    let filtered = filtered_transcript_entries(&dir, &filter);
    assert_eq!(filtered.len(), 1);
    assert!(filtered[0].ends_with("match.md"));

    let listing = render_transcript_list(&dir, &filter);
    assert!(listing.contains("(auto summary failed)"));
    assert!(listing.contains("match.md"));
    assert!(!listing.contains("no-failed.md"));
    assert!(!listing.contains("no-summary.md"));

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn parse_compare_args_accepts_two_values_and_flags() {
    assert_eq!(
        parse_compare_args("compare 1 2"),
        Some(CompareArgs {
            left_target: "1".to_string(),
            right_target: "2".to_string(),
            options: CompareOptions::default(),
        })
    );
    assert_eq!(
        parse_compare_args("compare latest sample.md --hunks 2 --lines 20"),
        Some(CompareArgs {
            left_target: "latest".to_string(),
            right_target: "sample.md".to_string(),
            options: CompareOptions {
                diff_enabled: true,
                max_hunks: 2,
                max_lines: 20,
            },
        })
    );
    assert_eq!(
        parse_compare_args("compare latest latest-1 --no-diff"),
        Some(CompareArgs {
            left_target: "latest".to_string(),
            right_target: "latest-1".to_string(),
            options: CompareOptions {
                diff_enabled: false,
                ..CompareOptions::default()
            },
        })
    );
    assert_eq!(parse_compare_args("compare 1"), None);
    assert_eq!(parse_compare_args("list compare 1 2"), None);
}

#[test]
fn build_transcript_compare_output_highlights_differences() {
    let dir = std::env::temp_dir().join(format!(
        "yode-memory-command-compare-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let left_path = dir.join("left.md");
    let right_path = dir.join("right.md");
    let left = "# Compaction Transcript\n\n- Session: abc\n- Mode: auto\n- Timestamp: 2026-01-01 10:00:00\n- Removed messages: 7\n- Tool results truncated: 1\n- Failed tool results: 1\n\n## Summary Anchor\n\n```text\nLeft summary\n```\n\n## Messages\n\n### User\n\n```text\nhello\n```\n";
    let right = "# Compaction Transcript\n\n- Session: abc\n- Mode: manual\n- Timestamp: 2026-01-01 11:00:00\n- Removed messages: 3\n- Tool results truncated: 0\n- Failed tool results: 0\n\n## Summary Anchor\n\n```text\nRight summary\n```\n\n## Messages\n\n### User\n\n```text\nhello\n```\n";
    std::fs::write(&left_path, left).unwrap();
    std::fs::write(&right_path, right).unwrap();

    let output = build_transcript_compare_output(
        &left_path,
        left,
        &right_path,
        right,
        &CompareOptions::default(),
    );
    assert!(output.contains("Status: different"));
    assert!(output.contains("Mode               auto -> manual"));
    assert!(output.contains("Failed tool results 1 -> 0"));
    assert!(output.contains("A: Left summary"));
    assert!(output.contains("B: Right summary"));
    assert!(output.contains("Section summary:"));
    assert!(output.contains("User blocks:"));
    assert!(output.contains("Content diff:"));
    assert!(output.contains("Changed lines:"));
    assert!(output.contains("Hunk 1"));
    assert!(output.contains("First difference:"));

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn build_transcript_compare_output_respects_no_diff_flag() {
    let dir = std::env::temp_dir().join(format!(
        "yode-memory-command-compare-nodiff-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let left_path = dir.join("left.md");
    let right_path = dir.join("right.md");
    let left = "# Compaction Transcript\n\n## Summary Anchor\n\n```text\nLeft summary\n```\n\n## Messages\n\n### Assistant\n\n```text\nhello\n```\n";
    let right = "# Compaction Transcript\n\n## Summary Anchor\n\n```text\nRight summary\n```\n\n## Messages\n\n### Assistant\n\n```text\nworld\n```\n";
    std::fs::write(&left_path, left).unwrap();
    std::fs::write(&right_path, right).unwrap();

    let output = build_transcript_compare_output(
        &left_path,
        left,
        &right_path,
        right,
        &CompareOptions {
            diff_enabled: false,
            ..CompareOptions::default()
        },
    );
    assert!(output.contains("Content diff:\n  disabled by flag"));

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn resolve_compare_target_supports_latest_alias_offsets() {
    let dir = std::env::temp_dir().join(format!(
        "yode-memory-command-latest-alias-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("aaa-compact-20240101.md"), "old").unwrap();
    std::fs::write(dir.join("bbb-compact-20250101.md"), "mid").unwrap();
    std::fs::write(dir.join("ccc-compact-20260101.md"), "new").unwrap();

    let latest = resolve_compare_target(&dir, "latest").unwrap();
    assert!(latest.ends_with("ccc-compact-20260101.md"));

    let previous = resolve_compare_target(&dir, "latest-1").unwrap();
    assert!(previous.ends_with("bbb-compact-20250101.md"));

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn parse_memory_document_reads_structured_sections() {
    let content = "# Session Snapshot\n\nYode refreshes this file automatically.\n\n## 2026-04-09 10:00:00 session abc12345\n\n### Goals\n\n- Goal one\n\n### Findings\n\n- Finding one\n\n### Decisions\n\n- Decision one\n\n### Files\n\n- Read: src/lib.rs\n\n### Open Questions\n\n- Question one\n\n### Freshness\n\n- Generated at: 2026-04-09 10:00:00\n\n### Confidence\n\n- High\n";
    let parsed = parse_memory_document(content).unwrap();
    assert_eq!(parsed.entries.len(), 1);
    assert_eq!(parsed.entries[0].session_id.as_deref(), Some("abc12345"));
    assert_eq!(parsed.entries[0].sections[0].title, "Goals");
    assert_eq!(parsed.entries[0].sections[0].items[0], "Goal one");
}

#[test]
fn memory_entry_age_formats_recent_entries() {
    let now = Local::now().naive_local();
    let ts = (now - Duration::hours(3))
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();
    assert_eq!(memory_entry_age(Some(&ts)), "3 hours old");
}

#[test]
fn render_memory_file_prefers_structured_view() {
    let dir = std::env::temp_dir().join(format!(
        "yode-memory-command-structured-file-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("session.live.md");
    std::fs::write(
        &path,
        "# Session Snapshot\n\nYode refreshes this file during the session to preserve recent context between compactions.\n\n## 2026-04-09 10:00:00 session abc12345\n\n### Goals\n\n- Goal one\n\n### Findings\n\n- Finding one\n\n### Decisions\n\n- Decision one\n\n### Files\n\n- Read: src/lib.rs\n\n### Open Questions\n\n- Question one\n\n### Freshness\n\n- Generated at: 2026-04-09 10:00:00\n\n### Confidence\n\n- High\n",
    )
    .unwrap();

    let rendered = render_memory_file("Live session memory", &path).unwrap();
    let CommandOutput::Message(rendered) = rendered else {
        panic!("expected message output");
    };
    assert!(rendered.contains("Schema: structured-v1"));
    assert!(rendered.contains("Structured view:"));
    assert!(rendered.contains("Goals (1): Goal one"));
    assert!(rendered.contains("Freshness (1): Generated at: 2026-04-09 10:00:00"));
    assert!(rendered.contains("Raw markdown:"));

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn render_memory_file_falls_back_for_legacy_content() {
    let dir = std::env::temp_dir().join(format!(
        "yode-memory-command-legacy-file-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("session.md");
    std::fs::write(&path, "# Session Memory\n\nSummary:\nlegacy content\n").unwrap();

    let rendered = render_memory_file("Compaction memory", &path).unwrap();
    let CommandOutput::Message(rendered) = rendered else {
        panic!("expected message output");
    };
    assert!(!rendered.contains("Schema: structured-v1"));
    assert!(rendered.contains("legacy content"));

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn resolve_transcript_target_supports_unique_fuzzy_aliases() {
    let dir = std::env::temp_dir().join(format!(
        "yode-memory-command-fuzzy-target-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("abc12345-compact-20240101.md"), "old").unwrap();
    std::fs::write(dir.join("def67890-compact-20250101.md"), "new").unwrap();

    let fuzzy = resolve_transcript_target(&dir, "def67890-compact").unwrap();
    assert!(fuzzy.ends_with("def67890-compact-20250101.md"));

    let latest_alias = resolve_transcript_target(&dir, "latest-1").unwrap();
    assert!(latest_alias.ends_with("abc12345-compact-20240101.md"));

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn parse_latest_compare_target_accepts_shortcut() {
    assert_eq!(parse_latest_compare_target("latest compare 2"), Some("2"));
    assert_eq!(
        parse_latest_compare_target("latest compare latest-1"),
        Some("latest-1")
    );
    assert_eq!(parse_latest_compare_target("latest compare "), None);
}
