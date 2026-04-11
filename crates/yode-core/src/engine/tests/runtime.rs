use super::*;

use yode_llm::types::ToolCall;

#[test]
fn test_live_session_memory_refresh_writes_snapshot() {
    let mut engine = make_engine(vec![], vec![]);
    let project_root = engine.context().working_dir_compat();
    let big = "x".repeat(9_000);
    engine.messages = vec![
        Message::system("system"),
        Message::user(format!("Need to debug resume flow {}", big)),
        Message::assistant("I traced the issue to persisted message snapshots."),
    ];
    engine.tool_call_count = 3;
    engine
        .files_modified
        .push(project_root.join("src/main.rs").display().to_string());

    engine.maybe_refresh_live_session_memory(None);

    let live_path = crate::session_memory::live_session_memory_path(&project_root);
    let content = std::fs::read_to_string(live_path).unwrap();
    assert!(content.contains("Session Snapshot"));
    assert!(content.contains("persisted message snapshots"));
    assert!(engine.session_memory_initialized);
    assert!(engine.last_session_memory_tool_count >= 3);
    let runtime = engine.runtime_state();
    assert_eq!(runtime.session_memory_update_count, 1);
}

#[test]
fn test_record_response_usage_tracks_prompt_cache_telemetry() {
    let mut engine = make_engine(vec![], vec![]);
    let (tx, _rx) = mpsc::unbounded_channel();

    engine.reset_prompt_cache_turn_runtime();
    engine.record_response_usage(
        &yode_llm::types::Usage {
            prompt_tokens: 1_200,
            completion_tokens: 180,
            total_tokens: 1_380,
            cache_write_tokens: 300,
            cache_read_tokens: 200,
        },
        &tx,
    );

    let usage = engine.cost_tracker().usage().clone();
    assert_eq!(usage.input_tokens, 700);
    assert_eq!(usage.output_tokens, 180);
    assert_eq!(usage.cache_write_tokens, 300);
    assert_eq!(usage.cache_read_tokens, 200);

    let runtime = engine.runtime_state();
    assert_eq!(runtime.prompt_cache.last_turn_prompt_tokens, Some(1_200));
    assert_eq!(runtime.prompt_cache.last_turn_completion_tokens, Some(180));
    assert_eq!(runtime.prompt_cache.last_turn_cache_write_tokens, Some(300));
    assert_eq!(runtime.prompt_cache.last_turn_cache_read_tokens, Some(200));
    assert_eq!(runtime.prompt_cache.reported_turns, 1);
    assert_eq!(runtime.prompt_cache.cache_write_turns, 1);
    assert_eq!(runtime.prompt_cache.cache_read_turns, 1);
    assert_eq!(runtime.prompt_cache.cache_write_tokens_total, 300);
    assert_eq!(runtime.prompt_cache.cache_read_tokens_total, 200);
}

#[test]
fn test_system_prompt_runtime_state_tracks_segment_breakdown() {
    let engine = make_engine(vec![], vec![]);
    let runtime = engine.runtime_state();

    assert!(runtime.system_prompt_estimated_tokens > 0);
    assert!(runtime.system_prompt_segments.len() >= 2);
    assert!(runtime
        .system_prompt_segments
        .iter()
        .any(|segment| segment.label == "Base prompt"));
    assert!(runtime
        .system_prompt_segments
        .iter()
        .any(|segment| segment.label == "Environment"));
}

#[test]
fn test_compaction_cause_histogram_tracks_counts() {
    let mut engine = make_engine(vec![], vec![]);

    engine.record_compaction_cause("skipped_below_threshold");
    engine.record_compaction_cause("skipped_below_threshold");
    engine.record_compaction_cause("success_manual");

    let runtime = engine.runtime_state();
    assert_eq!(
        runtime.compaction_cause_histogram.get("skipped_below_threshold"),
        Some(&2)
    );
    assert_eq!(
        runtime.compaction_cause_histogram.get("success_manual"),
        Some(&1)
    );
}

#[tokio::test]
async fn test_tool_runtime_state_and_artifact_are_recorded() {
    let mut engine = make_engine(vec![], vec![]);
    engine.reset_tool_turn_runtime();
    engine.record_tool_progress_summary("write_file", 2, Some("writing".to_string()));
    let batch_id = engine.register_parallel_batch(2);

    let tool_call = ToolCall {
        id: "tc-tool-runtime".into(),
        name: "write_file".into(),
        arguments: r#"{"file_path":"src/lib.rs","content":"fn main() {}\n"}"#.into(),
    };
    let raw_result = ToolResult::success_with_metadata(
        "Successfully wrote 12 bytes".to_string(),
        serde_json::json!({
            "file_path": "src/lib.rs",
            "line_count": 1,
            "diff_preview": {
                "removed": [],
                "added": ["fn main() {}"],
                "more_removed": 0,
                "more_added": 0
            }
        }),
    );

    let _final = engine
        .finalize_tool_result(
            &tool_call,
            raw_result,
            Some("2026-04-09 10:00:00".to_string()),
            42,
            2,
            Some(batch_id),
        )
        .await;

    let runtime = engine.runtime_state();
    assert_eq!(runtime.current_turn_tool_calls, 1);
    assert_eq!(runtime.current_turn_tool_progress_events, 2);
    assert_eq!(runtime.current_turn_parallel_batches, 1);
    assert_eq!(runtime.tool_traces.len(), 1);
    assert_eq!(runtime.tool_traces[0].tool_name, "write_file");
    assert!(runtime.tool_traces[0].diff_preview.is_some());

    engine.complete_tool_turn_artifact();
    let runtime = engine.runtime_state();
    assert!(runtime.last_tool_turn_artifact_path.is_some());
    let path = runtime.last_tool_turn_artifact_path.unwrap();
    assert!(std::path::Path::new(&path).exists());
}

#[test]
fn test_session_end_flush_writes_snapshot_without_threshold() {
    let mut engine = make_engine(vec![], vec![]);
    let project_root = engine.context().working_dir_compat();
    engine.messages = vec![
        Message::system("system"),
        Message::user("Short session"),
        Message::assistant("But still worth persisting on exit."),
    ];

    engine.flush_live_session_memory_on_shutdown();

    let live_path = crate::session_memory::live_session_memory_path(&project_root);
    let content = std::fs::read_to_string(live_path).unwrap();
    assert!(content.contains("Session Snapshot"));
    assert!(content.contains("Short session"));
}

#[test]
fn test_restore_messages_rebuilds_artifact_runtime_state() {
    let mut engine = make_engine(vec![], vec![]);
    let project_root = engine.context().working_dir_compat();
    let transcript_dir = project_root.join(".yode").join("transcripts");
    std::fs::create_dir_all(&transcript_dir).unwrap();
    let transcript_path = transcript_dir.join("abc12345-compact-20260101-100000.md");
    std::fs::write(
        &transcript_path,
        "# Compaction Transcript\n\n- Session: abc\n- Mode: manual\n- Timestamp: 2026-01-01 10:00:00\n- Removed messages: 7\n- Tool results truncated: 2\n- Failed tool results: 1\n- Session memory path: .yode/memory/session.md\n\n## Summary Anchor\n\n```text\nRecovered summary\n```\n",
    )
    .unwrap();

    let live_path = crate::session_memory::live_session_memory_path(&project_root);
    std::fs::create_dir_all(live_path.parent().unwrap()).unwrap();
    std::fs::write(&live_path, "# Session Snapshot\n\nplaceholder").unwrap();

    engine.restore_messages(vec![Message::user("resume")]);
    let runtime = engine.runtime_state();
    assert_eq!(runtime.last_compaction_mode.as_deref(), Some("manual"));
    assert_eq!(
        runtime.last_compaction_at.as_deref(),
        Some("2026-01-01 10:00:00")
    );
    assert_eq!(
        runtime.last_compaction_summary_excerpt.as_deref(),
        Some("Recovered summary")
    );
    let transcript_path_str = transcript_path.display().to_string();
    assert_eq!(
        runtime.last_compaction_transcript_path.as_deref(),
        Some(transcript_path_str.as_str())
    );
    let live_path_str = live_path.display().to_string();
    assert_eq!(
        runtime.last_session_memory_update_path.as_deref(),
        Some(live_path_str.as_str())
    );
}
