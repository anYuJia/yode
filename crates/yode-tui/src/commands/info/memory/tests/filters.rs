use super::*;

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
