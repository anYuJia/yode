use std::collections::HashMap;
use std::collections::HashSet;

use tempfile::tempdir;
use yode_llm::types::{Message, ToolCall};

use super::write_compaction_transcript;
use crate::context_manager::CompressionReport;

#[test]
fn writes_compaction_transcript_file() {
    let temp = tempdir().unwrap();
    let report = CompressionReport {
        removed: 4,
        tool_results_truncated: 1,
        summary: Some("[Context summary] previous state".to_string()),
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
