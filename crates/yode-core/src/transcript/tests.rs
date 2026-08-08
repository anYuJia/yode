use std::collections::HashMap;
use std::collections::HashSet;

use tempfile::tempdir;
use yode_llm::types::{Message, ToolCall};

use super::write_compaction_transcript;
use crate::context_manager::CompressionReport;
use crate::engine::CompactBoundaryRuntimeState;

#[test]
fn writes_compaction_transcript_file() {
    let temp = tempdir().unwrap();
    let report = CompressionReport {
        removed: 4,
        tool_results_truncated: 1,
        summary: Some("[Context summary] previous state".to_string()),
        removed_messages: vec![],
    };
    let messages = vec![
        Message::user("hello"),
        Message {
            storage_id: None,
            role: yode_llm::types::Role::Assistant,
            content: Some("working".to_string()),
            content_blocks: vec![yode_llm::types::ContentBlock::Text {
                text: "working".to_string(),
            }],
            reasoning: Some("need to inspect".to_string()),
            tool_calls: vec![ToolCall {
                id: "call_1".to_string(),
                name: "bash".to_string(),
                arguments: "{\"command\":\"pwd\"}".to_string(),
            }],
            tool_call_id: None,
            images: Vec::new(),
        },
        Message::tool_result("call_1", "permission denied"),
    ];
    let mut failed_ids = HashSet::new();
    failed_ids.insert("call_1".to_string());
    let mut files_read = HashMap::new();
    files_read.insert(temp.path().join("src/main.rs").display().to_string(), 42);
    let files_modified = vec![temp.path().join("src/lib.rs").display().to_string()];

    let transcript_path = write_compaction_transcript(
        temp.path(),
        "session-abcdef12",
        &messages,
        &report,
        "auto",
        &failed_ids,
        None,
        &files_read,
        &files_modified,
        None,
    )
    .unwrap();

    let content = std::fs::read_to_string(&transcript_path).unwrap();
    assert!(content.contains("# Compaction Transcript"));
    assert!(content.contains("- Failed tool results: 1"));
    assert!(content.contains("- Failed tools: bash"));
    assert!(content.contains("### Assistant"));
    assert!(content.contains("### Tool"));
    assert!(content.contains("Tool result status: `error`"));
}

#[test]
fn writes_compact_boundary_record_in_transcript() {
    let temp = tempdir().unwrap();
    let report = CompressionReport {
        removed: 2,
        tool_results_truncated: 0,
        summary: Some("[Context summary] compacted".to_string()),
        removed_messages: vec![],
    };
    let boundary = CompactBoundaryRuntimeState {
        mode: "manual".to_string(),
        timestamp: "2026-01-01 10:00:00".to_string(),
        removed_count: 2,
        tool_results_truncated: 0,
        preserved_tail_range: Some("3..5".to_string()),
        summary_fingerprint: Some("abcdef1234567890".to_string()),
        post_compact_estimated_tokens: 1200,
        post_compact_threshold_tokens: 96000,
        post_compact_token_delta: -94800,
        will_retrigger_next_turn: false,
        artifact_paths: vec![".yode/memory/session.md".to_string()],
    };

    let transcript_path = write_compaction_transcript(
        temp.path(),
        "session-abcdef12",
        &[Message::user("hello")],
        &report,
        "manual",
        &HashSet::new(),
        None,
        &HashMap::new(),
        &[],
        Some(&boundary),
    )
    .unwrap();

    let content = std::fs::read_to_string(&transcript_path).unwrap();
    assert!(content.contains("- Compact boundary: manual removed=2 post_tokens=1200"));
    assert!(content.contains("## Compact Boundary"));
    assert!(content.contains("\"preserved_tail_range\": \"3..5\""));
    let transcript_file_name = transcript_path
        .file_name()
        .expect("transcript filename")
        .to_string_lossy();
    assert!(content.contains(transcript_file_name.as_ref()));
}

#[test]
fn transcript_files_are_isolated_per_session() {
    let temp = tempdir().unwrap();
    let report = CompressionReport {
        removed: 1,
        tool_results_truncated: 0,
        summary: Some("summary".to_string()),
        removed_messages: vec![],
    };

    let session_a = "12345678-aaaa-bbbb";
    let session_b = "12345678-cccc-dddd";
    let path_a = write_compaction_transcript(
        temp.path(),
        session_a,
        &[Message::user("A")],
        &report,
        "auto",
        &HashSet::new(),
        None,
        &HashMap::new(),
        &[],
        None,
    )
    .unwrap();
    let path_b = write_compaction_transcript(
        temp.path(),
        session_b,
        &[Message::user("B")],
        &report,
        "auto",
        &HashSet::new(),
        None,
        &HashMap::new(),
        &[],
        None,
    )
    .unwrap();

    assert_ne!(path_a, path_b, "前 8 位相同的会话不得共用 transcript 路径");
    let content_a = std::fs::read_to_string(&path_a).unwrap();
    let content_b = std::fs::read_to_string(&path_b).unwrap();
    assert!(content_a.contains(session_a));
    assert!(content_b.contains(session_b));

    assert!(super::transcript_file_candidate(
        &path_a.file_name().unwrap().to_string_lossy(),
        session_a
    ));
    assert!(!super::transcript_file_candidate(
        &path_a.file_name().unwrap().to_string_lossy(),
        session_b
    ));
}

#[test]
fn legacy_short_id_transcript_migrates_only_for_matching_session() {
    let temp = tempdir().unwrap();
    let session = "12345678-aaaa-bbbb";
    let short = crate::session_artifact::legacy_session_short_id(session);
    let legacy_path = temp
        .path()
        .join(".yode/transcripts")
        .join(format!("{short}-compact-20260102-080000.md"));
    std::fs::create_dir_all(legacy_path.parent().unwrap()).unwrap();
    std::fs::write(
        &legacy_path,
        format!("# Compaction Transcript\n\n- Session: {session}\n- Mode: manual\n"),
    )
    .unwrap();

    let migrated_path = super::migrate_legacy_transcript_file(&legacy_path, session);
    let token = crate::session_artifact::session_artifact_token(session);
    let migrated = temp
        .path()
        .join(".yode/transcripts")
        .join(format!("{token}-compact-20260102-080000.md"));
    assert!(migrated.exists(), "验证归属后应迁移到新命名");
    assert!(!legacy_path.exists(), "迁移成功后旧文件应删除");
    assert_eq!(migrated_path, migrated);
    assert!(migrated_path.exists(), "运行时应返回迁移后的有效路径");
}

#[test]
fn legacy_transcript_candidate_requires_short_id_match() {
    let session = "12345678-aaaa-bbbb";
    assert!(super::transcript_file_candidate(
        "12345678-compact-20260101-000000.md",
        session
    ));
    assert!(!super::transcript_file_candidate(
        "87654321-compact-20260101-000000.md",
        session
    ));
}
