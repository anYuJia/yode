use std::collections::HashMap;

use super::{
    build_live_snapshot, clear_live_session_memory, persist_compaction_memory,
    persist_live_session_memory, persist_live_session_memory_summary, session_memory_path,
};
use crate::context_manager::CompressionReport;
use yode_llm::types::Message;

#[test]
fn prepends_newer_session_memory_entries() {
    let temp = tempfile::tempdir().unwrap();
    let project_root = temp.path();
    let session_id = "session-12345678-abcd";

    let first = CompressionReport {
        removed: 3,
        tool_results_truncated: 1,
        summary: Some("first summary".to_string()),
        removed_messages: vec![],
    };
    let second = CompressionReport {
        removed: 7,
        tool_results_truncated: 0,
        summary: Some("second summary".to_string()),
        removed_messages: vec![],
    };

    persist_compaction_memory(project_root, session_id, &first, &HashMap::new(), &[]).unwrap();
    let path =
        persist_compaction_memory(project_root, session_id, &second, &HashMap::new(), &[]).unwrap();

    assert_eq!(path, session_memory_path(project_root, session_id));
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
fn different_sessions_never_share_memory_files() {
    let temp = tempfile::tempdir().unwrap();
    let project_root = temp.path();

    let report = CompressionReport {
        removed: 3,
        tool_results_truncated: 1,
        summary: Some("session A summary".to_string()),
        removed_messages: vec![],
    };
    let report_b = CompressionReport {
        removed: 3,
        tool_results_truncated: 1,
        summary: Some("session B summary".to_string()),
        removed_messages: vec![],
    };

    let path_a = persist_compaction_memory(
        project_root,
        "session-aaaa-1111-aaaa-1111",
        &report,
        &HashMap::new(),
        &[],
    )
    .unwrap();
    let path_b = persist_compaction_memory(
        project_root,
        "session-bbbb-2222-bbbb-2222",
        &report_b,
        &HashMap::new(),
        &[],
    )
    .unwrap();

    assert_ne!(path_a, path_b);
    let content_a = std::fs::read_to_string(path_a).unwrap();
    let content_b = std::fs::read_to_string(path_b).unwrap();
    assert!(content_a.contains("session A summary"));
    assert!(!content_a.contains("session B summary"));
    assert!(content_b.contains("session B summary"));
    assert!(!content_b.contains("session A summary"));
}

#[test]
fn includes_relative_file_summaries() {
    let temp = tempfile::tempdir().unwrap();
    let project_root = temp.path();

    let report = CompressionReport {
        removed: 5,
        tool_results_truncated: 2,
        summary: Some("summary".to_string()),
        removed_messages: vec![],
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
        removed_messages: vec![],
    };

    let path = persist_compaction_memory(
        project_root,
        "session-artifact",
        &report,
        &HashMap::new(),
        &[],
    )
    .unwrap();
    let content = std::fs::read_to_string(path).unwrap();

    assert!(content.contains("Turn artifact: /tmp/latest-turn.json"));
}

#[test]
fn unreadable_existing_session_memory_is_not_silently_replaced() {
    let temp = tempfile::tempdir().unwrap();
    let project_root = temp.path();
    let memory_path = session_memory_path(project_root, "session-read-error");
    std::fs::create_dir_all(&memory_path).unwrap();

    let report = CompressionReport {
        removed: 1,
        tool_results_truncated: 0,
        summary: Some("new summary".to_string()),
        removed_messages: vec![],
    };

    let err = persist_compaction_memory(
        project_root,
        "session-read-error",
        &report,
        &HashMap::new(),
        &[],
    )
    .expect_err("unreadable existing memory path should fail instead of replacing history");

    assert!(err
        .to_string()
        .contains("Failed to read existing session memory file before rewrite"));
    assert!(memory_path.is_dir());
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
    assert!(content.contains("session-live"));
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
fn live_memory_files_are_isolated_per_session() {
    let temp = tempfile::tempdir().unwrap();
    let project_root = temp.path();
    let snapshot_a = build_live_snapshot(
        "session-aaaa-1111",
        &[Message::user("A goals")],
        1,
        &[],
        &[],
    );
    let snapshot_b = build_live_snapshot(
        "session-bbbb-2222",
        &[Message::user("B goals")],
        2,
        &[],
        &[],
    );

    persist_live_session_memory(project_root, &snapshot_a).unwrap();
    persist_live_session_memory(project_root, &snapshot_b).unwrap();

    let path_a = super::live_session_memory_path(project_root, "session-aaaa-1111");
    let path_b = super::live_session_memory_path(project_root, "session-bbbb-2222");
    assert_ne!(path_a, path_b);
    assert!(path_a.exists());
    assert!(path_b.exists());
    let content_a = std::fs::read_to_string(&path_a).unwrap();
    assert!(content_a.contains("A goals"));
    assert!(!content_a.contains("B goals"));
}

#[test]
fn clears_only_current_session_live_snapshot_file() {
    let temp = tempfile::tempdir().unwrap();
    let project_root = temp.path();
    let snapshot_a =
        build_live_snapshot("session-aaaa-1111", &[Message::user("hello")], 1, &[], &[]);
    let snapshot_b =
        build_live_snapshot("session-bbbb-2222", &[Message::user("hello")], 1, &[], &[]);
    persist_live_session_memory(project_root, &snapshot_a).unwrap();
    let path_b = persist_live_session_memory(project_root, &snapshot_b).unwrap();
    let path_a = super::live_session_memory_path(project_root, "session-aaaa-1111");
    assert!(path_a.exists());
    assert!(path_b.exists());

    clear_live_session_memory(project_root, "session-aaaa-1111").unwrap();
    assert!(!path_a.exists());
    assert!(path_b.exists(), "其他会话的 live 记忆不得被清除");
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
