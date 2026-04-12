use super::*;

#[test]
fn latest_transcript_prefers_newest_filename() {
    let dir =
        std::env::temp_dir().join(format!("yode-memory-command-test-{}", uuid::Uuid::new_v4()));
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
        content.push_str(&format!(
            "### Message {}\n\n```text\nline {}\n```\n\n",
            i, i
        ));
    }

    let folded = fold_transcript_preview(&content);
    assert!(folded.contains("## Summary Anchor"));
    assert!(folded.contains("## Messages Preview"));
    assert!(folded.contains("transcript preview folded"));
    assert!(folded.contains("Message 79"));
}

#[test]
fn warm_resume_transcript_caches_reports_warmed_entries() {
    let project_root =
        std::env::temp_dir().join(format!("yode-memory-warmup-{}", uuid::Uuid::new_v4()));
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
    let cache_stats = crate::commands::info::transcript_cache_stats();
    assert!(cache_stats.metadata_misses >= 2);

    std::fs::remove_dir_all(&project_root).ok();
}

#[test]
fn transcript_cache_stats_track_hits_and_invalidations() {
    let project_root = std::env::temp_dir().join(format!(
        "yode-memory-cache-stats-{}",
        uuid::Uuid::new_v4()
    ));
    let transcript_dir = project_root.join(".yode").join("transcripts");
    std::fs::create_dir_all(&transcript_dir).unwrap();
    let path = transcript_dir.join("aaa-compact-20260101.md");
    std::fs::write(
        &path,
        "# Compaction Transcript\n\n- Mode: auto\n- Timestamp: 2026-01-01 10:00:00\n",
    )
    .unwrap();

    let _ = read_transcript_metadata(&path);
    let _ = read_transcript_metadata(&path);
    let _ = latest_transcript(&transcript_dir);
    let _ = latest_transcript(&transcript_dir);
    std::thread::sleep(std::time::Duration::from_millis(10));
    std::fs::write(
        transcript_dir.join("bbb-compact-20260102.md"),
        "# Compaction Transcript\n\n- Mode: auto\n- Timestamp: 2026-01-02 10:00:00\n",
    )
    .unwrap();
    let _ = latest_transcript(&transcript_dir);

    let stats = crate::commands::info::transcript_cache_stats();
    assert!(stats.metadata_hits >= 1);
    assert!(stats.latest_hits >= 1);
    assert!(stats.invalidations >= 1);

    std::fs::remove_dir_all(&project_root).ok();
}

#[test]
fn long_session_benchmark_reports_hot_and_cold_paths() {
    let project_root =
        std::env::temp_dir().join(format!("yode-memory-bench-{}", uuid::Uuid::new_v4()));
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
    let dir = std::env::temp_dir().join(format!("yode-memory-picker-{}", uuid::Uuid::new_v4()));
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
    let dir =
        std::env::temp_dir().join(format!("yode-memory-command-meta-{}", uuid::Uuid::new_v4()));
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
    let content =
        "# Compaction Transcript\n\n## Summary Anchor\n\n```text\nFirst line\nSecond line\n```\n";
    let preview = extract_summary_preview(content).unwrap();
    assert!(preview.contains("First line"));
    assert!(preview.contains("Second line"));
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
