use super::*;

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
