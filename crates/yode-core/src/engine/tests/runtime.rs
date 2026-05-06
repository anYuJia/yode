use super::*;

use std::sync::Arc;

use yode_llm::types::RestoreSystemBlockHint;
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
            cache_deleted_tokens: 0,
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
    assert_eq!(runtime.prompt_cache.last_turn_cache_edit_deletions, None);
    assert_eq!(runtime.prompt_cache.last_turn_cache_deleted_tokens, Some(0));
    assert_eq!(runtime.prompt_cache.reported_turns, 1);
    assert_eq!(runtime.prompt_cache.cache_write_turns, 1);
    assert_eq!(runtime.prompt_cache.cache_read_turns, 1);
    assert_eq!(runtime.prompt_cache.cache_edit_turns, 0);
    assert_eq!(runtime.prompt_cache.cache_write_tokens_total, 300);
    assert_eq!(runtime.prompt_cache.cache_read_tokens_total, 200);
    assert_eq!(runtime.prompt_cache.cache_edit_deletions_total, 0);
    assert_eq!(runtime.prompt_cache.cache_deleted_tokens_total, 0);
}

#[test]
fn test_cached_microcompact_updates_prompt_cache_runtime() {
    let mut engine = make_engine(vec![], vec![]);
    let big = "x".repeat(1_000);
    engine.messages = vec![
        Message::system("system"),
        Message::user("u1"),
        Message::assistant("a1"),
        Message::tool_result("tc1", &big),
        Message::user("u2"),
        Message::assistant("a2"),
        Message::tool_result("tc2", &big),
        Message::user("recent1"),
        Message::assistant("recent2"),
        Message::user("recent3"),
        Message::assistant("recent4"),
        Message::user("recent5"),
        Message::assistant("recent6"),
        Message::user("recent7"),
        Message::assistant("recent8"),
    ];

    engine.apply_microcompact();

    let runtime = engine.runtime_state();
    assert_eq!(runtime.prompt_cache.last_turn_cache_edit_deletions, Some(2));
    assert_eq!(runtime.prompt_cache.cache_edit_turns, 1);
    assert_eq!(runtime.prompt_cache.cache_edit_deletions_total, 2);
    assert_eq!(
        runtime.prompt_cache.pending_cache_edit_ref_values,
        vec!["tc1".to_string(), "tc2".to_string()]
    );
}

#[test]
fn test_prompt_cache_pending_refs_promote_to_pinned_after_usage() {
    let mut engine = make_engine(vec![], vec![]);
    engine.messages = vec![
        Message::system("system"),
        Message::assistant("a1"),
        Message::tool_result("tc1", "ok1"),
        Message::assistant("a2"),
        Message::tool_result("tc2", "ok2"),
        Message::user("tail"),
    ];
    engine.pending_cache_edit_refs = vec!["tc1".to_string(), "tc2".to_string()];

    let request = engine.build_chat_request();
    engine.record_prompt_cache_request_state(&request);

    let (tx, _rx) = mpsc::unbounded_channel();
    engine.record_response_usage(
        &yode_llm::types::Usage {
            prompt_tokens: 1_000,
            completion_tokens: 100,
            total_tokens: 1_100,
            cache_write_tokens: 0,
            cache_read_tokens: 500,
            cache_deleted_tokens: 0,
        },
        &tx,
    );

    let runtime = engine.runtime_state();
    assert_eq!(runtime.prompt_cache.pending_cache_edit_refs, 0);
    assert_eq!(runtime.prompt_cache.pinned_cache_edit_refs, 2);
    assert_eq!(
        runtime.prompt_cache.pending_cache_edit_ref_values,
        Vec::<String>::new()
    );
    assert_eq!(
        runtime.prompt_cache.pinned_cache_edit_ref_values,
        vec!["tc1".to_string(), "tc2".to_string()]
    );
}

#[test]
fn test_build_chat_request_prunes_stale_cache_edit_refs() {
    let mut engine = make_engine(vec![], vec![]);
    engine.messages = vec![
        Message::system("system"),
        Message::assistant("a1"),
        Message::tool_result("tc1", "ok"),
        Message::user("tail"),
    ];
    engine.pending_cache_edit_refs = vec!["tc1".to_string(), "stale".to_string()];
    engine.pinned_cache_edit_refs = vec!["tc2".to_string(), "stale2".to_string()];

    let request = engine.build_chat_request();
    let hints = request.provider_hints.anthropic.expect("anthropic hints");
    assert_eq!(
        hints.pending_deleted_cache_references,
        vec!["tc1".to_string()]
    );
    assert_eq!(hints.pinned_deleted_cache_references, Vec::<String>::new());

    let runtime = engine.runtime_state();
    assert_eq!(runtime.prompt_cache.pending_cache_edit_refs, 1);
    assert_eq!(runtime.prompt_cache.pinned_cache_edit_refs, 0);
    assert_eq!(
        runtime.prompt_cache.pending_cache_edit_ref_values,
        vec!["tc1".to_string()]
    );
    assert_eq!(
        runtime.prompt_cache.pinned_cache_edit_ref_values,
        Vec::<String>::new()
    );
}

#[test]
fn test_prompt_cache_break_detection_flags_unexpected_drop() {
    let mut engine = make_engine(vec![], vec![]);
    let request = engine.build_chat_request();
    let (tx, _rx) = mpsc::unbounded_channel();

    engine.record_prompt_cache_request_state(&request);
    engine.record_response_usage(
        &yode_llm::types::Usage {
            prompt_tokens: 1_000,
            completion_tokens: 100,
            total_tokens: 1_100,
            cache_write_tokens: 0,
            cache_read_tokens: 5_000,
            cache_deleted_tokens: 0,
        },
        &tx,
    );

    engine.record_prompt_cache_request_state(&request);
    engine.record_response_usage(
        &yode_llm::types::Usage {
            prompt_tokens: 1_050,
            completion_tokens: 110,
            total_tokens: 1_160,
            cache_write_tokens: 0,
            cache_read_tokens: 1_000,
            cache_deleted_tokens: 0,
        },
        &tx,
    );

    let runtime = engine.runtime_state();
    assert_eq!(runtime.prompt_cache.prompt_cache_break_count, 1);
    assert!(runtime
        .prompt_cache
        .last_prompt_cache_break_reason
        .as_deref()
        .unwrap_or_default()
        .contains("cache read dropped"));
    let diff_path = runtime
        .prompt_cache
        .last_prompt_cache_diff_artifact_path
        .as_deref()
        .expect("break diff artifact path");
    assert!(std::path::Path::new(diff_path).exists());
}

#[test]
fn test_system_prefix_changes_are_classified_separately() {
    let mut engine = make_engine(vec![], vec![]);
    let request = engine.build_chat_request();
    let (tx, _rx) = mpsc::unbounded_channel();

    engine.record_prompt_cache_request_state(&request);
    engine.record_response_usage(
        &yode_llm::types::Usage {
            prompt_tokens: 1_000,
            completion_tokens: 100,
            total_tokens: 1_100,
            cache_write_tokens: 0,
            cache_read_tokens: 1_500,
            cache_deleted_tokens: 0,
        },
        &tx,
    );

    engine
        .messages
        .push(Message::system("[Context summary] compacted"));
    let request = engine.build_chat_request();
    engine.record_prompt_cache_request_state(&request);
    engine.record_response_usage(
        &yode_llm::types::Usage {
            prompt_tokens: 1_050,
            completion_tokens: 110,
            total_tokens: 1_160,
            cache_write_tokens: 0,
            cache_read_tokens: 1_000,
            cache_deleted_tokens: 0,
        },
        &tx,
    );

    let runtime = engine.runtime_state();
    assert_eq!(
        runtime
            .prompt_cache
            .last_prompt_cache_transition_kind
            .as_deref(),
        Some("system_prefix_changed")
    );
    assert_eq!(
        runtime
            .prompt_cache
            .last_prompt_cache_change_summary
            .as_deref(),
        Some("system")
    );
}

#[test]
fn test_restore_prefix_changes_are_classified_separately() {
    let mut engine = make_engine(vec![], vec![]);
    engine.post_compact_restore_blocks =
        vec!["[Post-compact restore: files]\n- Recent files read: src/main.rs".to_string()];
    let request = engine.build_chat_request();
    let (tx, _rx) = mpsc::unbounded_channel();

    engine.record_prompt_cache_request_state(&request);
    engine.record_response_usage(
        &yode_llm::types::Usage {
            prompt_tokens: 1_000,
            completion_tokens: 100,
            total_tokens: 1_100,
            cache_write_tokens: 0,
            cache_read_tokens: 1_500,
            cache_deleted_tokens: 0,
        },
        &tx,
    );

    engine.post_compact_restore_blocks =
        vec!["[Post-compact restore: files]\n- Recent files read: src/lib.rs".to_string()];
    let request = engine.build_chat_request();
    engine.record_prompt_cache_request_state(&request);
    engine.record_response_usage(
        &yode_llm::types::Usage {
            prompt_tokens: 1_050,
            completion_tokens: 110,
            total_tokens: 1_160,
            cache_write_tokens: 0,
            cache_read_tokens: 1_000,
            cache_deleted_tokens: 0,
        },
        &tx,
    );

    let runtime = engine.runtime_state();
    assert_eq!(
        runtime
            .prompt_cache
            .last_prompt_cache_transition_kind
            .as_deref(),
        Some("restore_prefix_changed")
    );
    assert_eq!(
        runtime
            .prompt_cache
            .last_prompt_cache_change_summary
            .as_deref(),
        Some("restore")
    );
}

#[test]
fn test_model_change_sets_expected_prompt_cache_drop_reason() {
    let mut engine = make_engine(vec![], vec![]);
    engine.set_model("gpt-4o".to_string());

    let request = engine.build_chat_request();
    engine.record_prompt_cache_request_state(&request);

    let runtime = engine.runtime_state();
    assert_eq!(
        runtime
            .prompt_cache
            .last_prompt_cache_expected_drop_reason
            .as_deref(),
        Some("model_change")
    );
}

#[test]
fn test_expected_prompt_cache_drop_writes_diff_artifact() {
    let mut engine = make_engine(vec![], vec![]);
    engine.set_model("gpt-4o".to_string());

    let request = engine.build_chat_request();
    engine.record_prompt_cache_request_state(&request);

    let (tx, _rx) = mpsc::unbounded_channel();
    engine.record_response_usage(
        &yode_llm::types::Usage {
            prompt_tokens: 1_000,
            completion_tokens: 100,
            total_tokens: 1_100,
            cache_write_tokens: 0,
            cache_read_tokens: 0,
            cache_deleted_tokens: 0,
        },
        &tx,
    );

    let runtime = engine.runtime_state();
    assert_eq!(
        runtime
            .prompt_cache
            .last_prompt_cache_transition_kind
            .as_deref(),
        Some("expected_drop")
    );
    let diff_path = runtime
        .prompt_cache
        .last_prompt_cache_diff_artifact_path
        .as_deref()
        .expect("expected diff artifact path");
    assert!(std::path::Path::new(diff_path).exists());
}

#[test]
fn test_first_prompt_cache_snapshot_is_marked_cold_start() {
    let mut engine = make_engine(vec![], vec![]);
    let request = engine.build_chat_request();
    engine.record_prompt_cache_request_state(&request);

    let (tx, _rx) = mpsc::unbounded_channel();
    engine.record_response_usage(
        &yode_llm::types::Usage {
            prompt_tokens: 1_000,
            completion_tokens: 100,
            total_tokens: 1_100,
            cache_write_tokens: 0,
            cache_read_tokens: 0,
            cache_deleted_tokens: 0,
        },
        &tx,
    );

    let runtime = engine.runtime_state();
    assert_eq!(
        runtime
            .prompt_cache
            .last_prompt_cache_transition_kind
            .as_deref(),
        Some("cold_start")
    );
}

#[test]
fn test_cache_edits_transition_is_classified_separately() {
    let mut engine = make_engine(vec![], vec![]);
    let big = "x".repeat(1_000);
    engine.messages = vec![
        Message::system("system"),
        Message::user("u1"),
        Message::assistant("a1"),
        Message::tool_result("tc1", &big),
        Message::user("u2"),
        Message::assistant("a2"),
        Message::tool_result("tc2", &big),
        Message::user("recent1"),
        Message::assistant("recent2"),
        Message::user("recent3"),
        Message::assistant("recent4"),
        Message::user("recent5"),
        Message::assistant("recent6"),
        Message::user("recent7"),
        Message::assistant("recent8"),
    ];
    engine.apply_microcompact();

    let request = engine.build_chat_request();
    engine.record_prompt_cache_request_state(&request);
    let (tx, _rx) = mpsc::unbounded_channel();
    engine.record_response_usage(
        &yode_llm::types::Usage {
            prompt_tokens: 1_000,
            completion_tokens: 100,
            total_tokens: 1_100,
            cache_write_tokens: 0,
            cache_read_tokens: 0,
            cache_deleted_tokens: 200,
        },
        &tx,
    );

    let runtime = engine.runtime_state();
    assert_eq!(
        runtime
            .prompt_cache
            .last_prompt_cache_transition_kind
            .as_deref(),
        Some("cache_edit_applied")
    );
}

#[test]
fn test_clear_conversation_resets_cache_edit_tracking() {
    let mut engine = make_engine(vec![], vec![]);
    engine.messages.push(Message::user("hello"));
    engine.pending_cache_edit_refs = vec!["tc1".to_string()];
    engine.pinned_cache_edit_refs = vec!["tc2".to_string()];
    engine.cached_microcompact_deleted_refs = vec!["tc1".to_string(), "tc2".to_string()];

    engine.clear_conversation();

    let runtime = engine.runtime_state();
    assert_eq!(runtime.prompt_cache.pending_cache_edit_refs, 0);
    assert_eq!(runtime.prompt_cache.pinned_cache_edit_refs, 0);
    assert_eq!(
        engine.forced_prompt_cache_expected_drop_reason.as_deref(),
        Some("clear_conversation")
    );
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
    assert!(runtime
        .system_prompt_segments
        .iter()
        .any(|segment| segment.label == "Multi-agent coordination"));
    assert!(engine.system_prompt.contains("send_message"));
    assert!(engine.system_prompt.contains("<task-notification>"));
}

#[test]
fn test_runtime_state_counts_hidden_restore_in_context_and_segments() {
    let mut engine = make_engine(vec![], vec![]);
    let before = engine.runtime_state();
    engine.post_compact_restore_blocks = vec![
        "[Post-compact restore: runtime]\n- Runtime cwd: /tmp/project".to_string(),
        "[Post-compact restore: files]\n- Recent files read: src/main.rs".to_string(),
    ];

    let after = engine.runtime_state();

    assert!(after.estimated_context_tokens > before.estimated_context_tokens);
    assert!(after.system_prompt_estimated_tokens > before.system_prompt_estimated_tokens);
    assert!(after
        .system_prompt_segments
        .iter()
        .any(|segment| segment.label == "Post-compact restore"));
}

#[test]
fn test_compaction_cause_histogram_tracks_counts() {
    let mut engine = make_engine(vec![], vec![]);

    engine.record_compaction_cause("skipped_below_threshold");
    engine.record_compaction_cause("skipped_below_threshold");
    engine.record_compaction_cause("success_manual");

    let runtime = engine.runtime_state();
    assert_eq!(
        runtime
            .compaction_cause_histogram
            .get("skipped_below_threshold"),
        Some(&2)
    );
    assert_eq!(
        runtime.compaction_cause_histogram.get("success_manual"),
        Some(&1)
    );
}

#[test]
fn test_build_chat_request_hides_denied_tools_from_model() {
    let mut engine = make_engine(
        vec![
            Arc::new(MockReadTool {
                name: "read_file".into(),
            }),
            Arc::new(MockWriteTool {
                name: "write_file".into(),
            }),
        ],
        vec![],
    );

    engine
        .permissions_mut()
        .set_mode(crate::PermissionMode::Plan);
    let request = engine.build_chat_request();
    let tool_names = request
        .tools
        .into_iter()
        .map(|tool| tool.name)
        .collect::<Vec<_>>();

    assert!(tool_names.iter().any(|name| name == "read_file"));
    assert!(!tool_names.iter().any(|name| name == "write_file"));
}

#[test]
fn test_build_chat_request_injects_restore_blocks_as_virtual_system_messages() {
    let mut engine = make_engine(vec![], vec![]);
    engine
        .messages
        .push(Message::system("[Context summary] compacted"));
    engine.post_compact_restore_blocks = vec![
        "[Post-compact restore: runtime]\n- Runtime cwd: /tmp/project".to_string(),
        "[Post-compact restore: files]\n- Recent files read: src/main.rs".to_string(),
    ];
    engine.messages.push(Message::user("resume"));

    let request = engine.build_chat_request();
    assert_eq!(request.provider_hints.restore_system_blocks.len(), 2);
    assert_eq!(
        request
            .provider_hints
            .restore_system_blocks
            .first()
            .map(|block| (block.kind.as_str(), block.content.as_str())),
        Some(("runtime", "- Runtime cwd: /tmp/project"))
    );

    assert!(!engine.messages.iter().any(|message| {
        message
            .content
            .as_deref()
            .unwrap_or_default()
            .starts_with("[Post-compact restore:")
    }));
    assert!(request.messages.iter().all(|message| {
        !message
            .content
            .as_deref()
            .unwrap_or_default()
            .starts_with("[Post-compact restore:")
    }));
}

#[test]
fn test_build_chat_request_sanitizes_volatile_restore_blocks_for_cache_stability() {
    let mut engine = make_engine(vec![], vec![]);
    engine.post_compact_restore_blocks = vec![
        "[Post-compact restore: tools]\n- Tool pool: 4 active visible, 1 active hidden, 2 deferred visible, search=enabled (reason: mcp)\n- Tool inventory: total=9 active=7 deferred=2 activations=3 last=mcp__demo__search".to_string(),
        "[Post-compact restore: mcp]\n- MCP: visible_tools=2 deferred_tools=4 cache(list 10 hit/2 miss, read 8 hit/5 miss)".to_string(),
        "[Post-compact restore: artifacts]\n- Session memory artifact: .yode/status/session.md\n- Latest turn artifact: /tmp/turn.md".to_string(),
    ];

    let request = engine.build_chat_request();
    let system_texts = request
        .provider_hints
        .restore_system_blocks
        .iter()
        .map(|block| (block.kind.as_str(), block.content.as_str()))
        .collect::<Vec<_>>();

    assert!(system_texts.iter().any(|text| {
        *text
            == (
                "tools",
                "- Tool availability follows the current runtime tool pool and permission state.",
            )
    }));
    assert!(system_texts.iter().any(|text| {
        *text
            == (
                "mcp",
                "- MCP availability follows the current runtime inventory.",
            )
    }));
    assert!(system_texts.iter().any(|text| {
        *text
            == (
                "artifacts",
                "- Compaction and runtime artifacts remain available via status inspectors.",
            )
    }));
    assert!(!system_texts
        .iter()
        .any(|(_, content)| content.contains("/tmp/turn.md")));
    assert!(!system_texts
        .iter()
        .any(|(_, content)| content.contains("cache(list")));
    assert!(!system_texts
        .iter()
        .any(|(_, content)| content.contains("Tool inventory: total=")));
}

#[test]
fn test_runtime_state_exposes_tool_pool_gating() {
    let mut engine = make_engine(
        vec![
            Arc::new(MockReadTool {
                name: "read_file".into(),
            }),
            Arc::new(MockWriteTool {
                name: "write_file".into(),
            }),
            Arc::new(MockReadTool {
                name: "mcp__demo__search".into(),
            }),
        ],
        vec![],
    );

    engine.permissions_mut().deny("mcp__demo__search");
    let runtime = engine.runtime_state();

    assert_eq!(runtime.tool_pool.permission_mode, "default");
    assert_eq!(runtime.tool_pool.visible_active_count(), 2);
    assert_eq!(runtime.tool_pool.hidden_active_count(), 1);
    assert_eq!(runtime.tool_pool.confirm_count(), 1);
    assert_eq!(runtime.tool_pool.deny_count(), 1);
    assert_eq!(runtime.tool_pool.visible_builtin_count(), 2);
    assert_eq!(runtime.tool_pool.visible_mcp_count(), 0);
    assert!(runtime
        .tool_pool
        .hidden_tool_names()
        .contains(&"mcp__demo__search"));
}

#[test]
fn test_deferred_activation_updates_next_chat_request_tools() {
    let engine = make_engine(
        vec![Arc::new(MockReadTool {
            name: "read_file".into(),
        })],
        vec![],
    );

    engine.tools.register_deferred(Arc::new(MockReadTool {
        name: "mcp__demo__search".into(),
    }));
    engine.tools.set_tool_search_enabled(true);

    let before = engine.build_chat_request();
    assert!(!before
        .tools
        .iter()
        .any(|tool| tool.name == "mcp__demo__search"));
    assert_eq!(engine.tools.inventory().deferred_count, 1);

    assert!(engine.tools.activate_tool("mcp__demo__search"));

    let after = engine.build_chat_request();
    assert!(after
        .tools
        .iter()
        .any(|tool| tool.name == "mcp__demo__search"));
    assert_eq!(engine.tools.inventory().activation_count, 1);
    assert_eq!(
        engine.tools.inventory().last_activated_tool.as_deref(),
        Some("mcp__demo__search")
    );
}

#[test]
fn test_tool_search_tool_hidden_when_search_mode_disabled() {
    let engine = make_engine(
        vec![Arc::new(MockReadTool {
            name: "tool_search".into(),
        })],
        vec![],
    );

    let request = engine.build_chat_request();
    assert!(!request.tools.iter().any(|tool| tool.name == "tool_search"));
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
    let content = std::fs::read_to_string(path).unwrap();
    assert!(content.contains("## Tool Pool"));
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

    engine.restore_messages(vec![
        Message::assistant("a1"),
        Message::tool_result("tc1", "ok1"),
        Message::assistant("a2"),
        Message::tool_result("tc2", "ok2"),
        Message::user("resume"),
    ]);
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
    assert_eq!(
        engine.forced_prompt_cache_expected_drop_reason.as_deref(),
        Some("restore_messages")
    );
    assert_eq!(
        runtime
            .prompt_cache
            .last_prompt_cache_expected_drop_reason
            .as_deref(),
        Some("restore_messages")
    );
}

#[test]
fn test_restore_messages_rehydrates_post_compact_restore_blocks_from_artifact() {
    let mut engine = make_engine(vec![], vec![]);
    let project_root = engine.context().working_dir_compat();
    let status_dir = project_root.join(".yode").join("status");
    std::fs::create_dir_all(&status_dir).unwrap();
    let short_session = engine
        .context()
        .session_id
        .chars()
        .take(8)
        .collect::<String>();
    let state_path = status_dir.join(format!("{}-post-compact-restore-state.json", short_session));
    std::fs::write(
        &state_path,
        r#"{
          "blocks": [
            { "kind": "runtime", "content": "[Post-compact restore: runtime]\n- Runtime cwd: /tmp", "fingerprint": "a" },
            { "kind": "files", "content": "[Post-compact restore: files]\n- Recent files read: src/main.rs", "fingerprint": "b" }
          ]
        }"#,
    )
    .unwrap();

    engine.restore_messages(vec![
        Message::system("[Context summary] compacted"),
        Message::user("resume"),
    ]);

    assert!(!engine.messages.iter().any(|message| {
        message
            .content
            .as_deref()
            .unwrap_or_default()
            .starts_with("[Post-compact restore:")
    }));

    let request = engine.build_chat_request();
    assert!(request
        .provider_hints
        .restore_system_blocks
        .iter()
        .any(|block| {
            block
                == &RestoreSystemBlockHint {
                    kind: "runtime".to_string(),
                    content: "- Runtime cwd: /tmp".to_string(),
                }
        }));
    assert!(request
        .provider_hints
        .restore_system_blocks
        .iter()
        .any(|block| {
            block
                == &RestoreSystemBlockHint {
                    kind: "files".to_string(),
                    content: "- Recent files read: src/main.rs".to_string(),
                }
        }));
}

#[test]
fn test_restore_messages_rehydrates_prompt_cache_state_from_artifact() {
    let mut engine = make_engine(vec![], vec![]);
    engine.set_model("claude-3-5-sonnet".to_string());
    let project_root = engine.context().working_dir_compat();
    let status_dir = project_root.join(".yode").join("status");
    std::fs::create_dir_all(&status_dir).unwrap();
    let short_session = engine
        .context()
        .session_id
        .chars()
        .take(8)
        .collect::<String>();
    let state_path = status_dir.join(format!("{}-prompt-cache-state.json", short_session));
    std::fs::write(
        &state_path,
        r#"{
          "reported_turns": 4,
          "cache_write_turns": 2,
          "cache_read_turns": 3,
          "cache_edit_turns": 1,
          "cache_write_tokens_total": 400,
          "cache_read_tokens_total": 1200,
          "cache_deleted_tokens_total": 150,
          "cache_edit_deletions_total": 2,
          "last_turn_cache_read_tokens": 500,
          "pending_cache_edit_refs": 1,
          "pinned_cache_edit_refs": 1,
          "pending_cache_edit_ref_values": ["tc2"],
          "pinned_cache_edit_ref_values": ["tc1"],
          "last_prompt_cache_prefix_hash": "prefix-hash",
          "last_prompt_cache_system_hash": "system-hash",
          "last_prompt_cache_tool_hash": "tool-hash",
          "last_prompt_cache_message_hash": "message-hash",
          "last_prompt_cache_change_summary": "stable",
          "last_prompt_cache_transition_kind": "cache_edit_applied",
          "last_prompt_cache_transition_reason": "cache_edits",
          "last_prompt_cache_diff_summary": "cache_edit_applied / old->new / cache_edits",
          "last_prompt_cache_break_reason": "none",
          "last_prompt_cache_break_at": "2026-01-02 03:04:05"
        }"#,
    )
    .unwrap();

    let diff_path = status_dir.join(format!("{}-prompt-cache-diff.md", short_session));
    std::fs::write(&diff_path, "# Prompt Cache Diff").unwrap();

    engine.restore_messages(vec![
        Message::assistant("a1"),
        Message::tool_result("tc1", "ok1"),
        Message::assistant("a2"),
        Message::tool_result("tc2", "ok2"),
        Message::user("resume"),
    ]);

    let runtime = engine.runtime_state();
    assert_eq!(runtime.prompt_cache.reported_turns, 4);
    assert_eq!(runtime.prompt_cache.cache_deleted_tokens_total, 150);
    assert_eq!(runtime.prompt_cache.pending_cache_edit_refs, 1);
    assert_eq!(runtime.prompt_cache.pinned_cache_edit_refs, 1);
    assert_eq!(
        runtime.prompt_cache.pending_cache_edit_ref_values,
        vec!["tc2".to_string()]
    );
    assert_eq!(
        runtime.prompt_cache.pinned_cache_edit_ref_values,
        vec!["tc1".to_string()]
    );
    assert_eq!(
        runtime
            .prompt_cache
            .last_prompt_cache_prefix_hash
            .as_deref(),
        Some("prefix-hash")
    );
    assert_eq!(
        runtime
            .prompt_cache
            .last_prompt_cache_transition_kind
            .as_deref(),
        Some("cache_edit_applied")
    );
    assert_eq!(
        runtime
            .prompt_cache
            .last_prompt_cache_diff_artifact_path
            .as_deref(),
        Some(diff_path.display().to_string().as_str())
    );

    let request = engine.build_chat_request();
    let hints = request
        .provider_hints
        .anthropic
        .expect("anthropic prompt cache hints");
    assert_eq!(
        hints.pending_deleted_cache_references,
        vec!["tc2".to_string()]
    );
    assert_eq!(
        hints.pinned_deleted_cache_references,
        vec!["tc1".to_string()]
    );
}

#[test]
fn test_reset_turn_runtime_clears_stream_watchdog_stage() {
    let mut engine = make_engine(vec![], vec![]);
    engine.last_stream_watchdog_stage = Some("receive_loop:stall_timeout".to_string());

    engine.reset_turn_runtime_state();

    assert_eq!(
        engine.runtime_state().last_stream_watchdog_stage.as_deref(),
        None
    );
}
