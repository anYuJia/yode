use std::collections::HashMap;

use super::{
    build_live_snapshot, clear_live_session_memory, persist_compaction_memory,
    persist_live_session_memory, persist_live_session_memory_summary,
};
use crate::context_manager::CompressionReport;
use yode_llm::types::Message;

#[test]
fn prepends_newer_session_memory_entries() {
    let temp = tempfile::tempdir().unwrap();
    let project_root = temp.path();

    let first = CompressionReport {
        removed: 3,
        tool_results_truncated: 1,
        summary: Some("first summary".to_string()),
    };
    let second = CompressionReport {
        removed: 7,
        tool_results_truncated: 0,
        summary: Some("second summary".to_string()),
    };

    persist_compaction_memory(project_root, "session-one", &first, &HashMap::new(), &[]).unwrap();
    let path =
        persist_compaction_memory(project_root, "session-two", &second, &HashMap::new(), &[])
            .unwrap();

    let content = std::fs::read_to_string(path).unwrap();
    let first_idx = content.find("first summary").unwrap();
    let second_idx = content.find("second summary").unwrap();
    assert!(content.contains("### Goals"));
    assert!(content.contains("### Findings"));
    assert!(content.contains("### Decisions"));
    assert!(content.contains("### Files"));
    assert!(content.contains("### Open Questions"));
    assert!(second_idx < first_idx);
}

#[test]
fn includes_relative_file_summaries() {
    let temp = tempfile::tempdir().unwrap();
    let project_root = temp.path();

    let report = CompressionReport {
        removed: 5,
        tool_results_truncated: 2,
        summary: Some("summary".to_string()),
    };

    let mut files_read = HashMap::new();
    files_read.insert(
        project_root.join("src/lib.rs").display().to_string(),
        120usize,
    );

    let path = persist_compaction_memory(
        project_root,
        "session-three",
        &report,
        &files_read,
        &[project_root.join("src/main.rs").display().to_string()],
    )
    .unwrap();

    let content = std::fs::read_to_string(path).unwrap();
    assert!(content.contains("### Files"));
    assert!(content.contains("src/lib.rs (120 lines)"));
    assert!(content.contains("src/main.rs"));
}

#[test]
fn preserves_turn_artifact_cross_link_from_compaction_summary() {
    let temp = tempfile::tempdir().unwrap();
    let project_root = temp.path();

    let report = CompressionReport {
        removed: 5,
        tool_results_truncated: 1,
        summary: Some(
            "[Context summary] Older conversation was compacted to stay within the model window.\n- Removed messages: 5\n- Turn artifact: /tmp/latest-turn.json".to_string(),
        ),
    };

    let path =
        persist_compaction_memory(project_root, "session-artifact", &report, &HashMap::new(), &[])
            .unwrap();
    let content = std::fs::read_to_string(path).unwrap();

    assert!(content.contains("Turn artifact: /tmp/latest-turn.json"));
}

#[test]
fn persists_live_session_snapshot() {
    let temp = tempfile::tempdir().unwrap();
    let snapshot = build_live_snapshot(
        "session-live",
        &[
            Message::user("Investigate the resume bug in compact mode"),
            Message::assistant("I traced it to the persisted message snapshot."),
        ],
        4,
        &[temp.path().join("src/lib.rs").display().to_string()],
        &[temp.path().join("src/main.rs").display().to_string()],
    );

    let path = persist_live_session_memory(temp.path(), &snapshot).unwrap();
    let content = std::fs::read_to_string(path).unwrap();

    assert!(content.contains("Session Snapshot"));
    assert!(content.contains("### Goals"));
    assert!(content.contains("### Findings"));
    assert!(content.contains("### Decisions"));
    assert!(content.contains("### Files"));
    assert!(content.contains("### Open Questions"));
    assert!(content.contains("resume bug"));
    assert!(content.contains("persisted message snapshot"));
    assert!(content.contains("Total tool calls this session: 4"));
}

#[test]
fn clears_live_session_snapshot_file() {
    let temp = tempfile::tempdir().unwrap();
    let snapshot = build_live_snapshot("session-live", &[Message::user("hello")], 1, &[], &[]);
    let path = persist_live_session_memory(temp.path(), &snapshot).unwrap();
    assert!(path.exists());

    clear_live_session_memory(temp.path()).unwrap();
    assert!(!path.exists());
}

#[test]
fn normalizes_unstructured_live_summary_into_schema() {
    let temp = tempfile::tempdir().unwrap();
    let snapshot = build_live_snapshot(
        "session-live",
        &[
            Message::user("Investigate the resume bug"),
            Message::assistant("I will keep the persisted snapshot approach."),
        ],
        2,
        &[temp.path().join("src/lib.rs").display().to_string()],
        &[temp.path().join("src/main.rs").display().to_string()],
    );

    let path = persist_live_session_memory_summary(
        temp.path(),
        &snapshot,
        "Need to preserve the snapshot rewrite fix.",
    )
    .unwrap();
    let content = std::fs::read_to_string(path).unwrap();

    assert!(content.contains("### Goals"));
    assert!(content.contains("### Findings"));
    assert!(content.contains("### Decisions"));
    assert!(content.contains("### Files"));
    assert!(content.contains("### Open Questions"));
    assert!(content.contains("### Freshness"));
    assert!(content.contains("### Confidence"));
}
