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
    let normalized_content = content.replace('\\', "/");
    let normalized_transcript_path = transcript_path.display().to_string().replace('\\', "/");
    assert!(normalized_content.contains(&normalized_transcript_path));
}
