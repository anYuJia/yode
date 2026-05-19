use super::*;
use std::sync::Arc;

use yode_llm::types::{ChatResponse, ImageData, ToolCall, Usage};
use yode_llm::MockProvider;
use yode_tools::registry::ToolRegistry;

fn make_engine_with_provider(provider: Arc<dyn yode_llm::provider::LlmProvider>) -> AgentEngine {
    let registry = ToolRegistry::new();
    let permissions = PermissionManager::from_confirmation_list(vec![]);
    let workdir = std::env::temp_dir().join(format!("yode-engine-test-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&workdir).unwrap();
    let context = AgentContext::new(workdir, "stub".to_string(), "claude-sonnet-4".to_string());
    AgentEngine::new(provider, Arc::new(registry), permissions, context)
}

fn summary_response(content: &str) -> ChatResponse {
    ChatResponse {
        message: Message::assistant(content),
        usage: Usage::default(),
        model: "mock-summary".to_string(),
        stop_reason: None,
    }
}

#[tokio::test]
async fn test_autocompact_circuit_breaker_trips_after_repeated_failures() {
    let mut engine = make_engine(vec![], vec![]);
    engine.messages = vec![
        Message::system("system"),
        Message::user("u1"),
        Message::assistant("a1"),
        Message::user("u2"),
        Message::assistant("a2"),
        Message::user("u3"),
        Message::assistant("a3"),
    ];
    engine.current_query_source = QuerySource::User;

    let (tx, mut rx) = mpsc::unbounded_channel();
    for _ in 0..MAX_CONSECUTIVE_COMPACTION_FAILURES {
        engine.maybe_compact_context(190_000, &tx).await;
    }

    assert!(engine.autocompact_disabled);
    assert_eq!(
        engine.compaction_failures,
        MAX_CONSECUTIVE_COMPACTION_FAILURES
    );
    assert!(engine.messages.iter().any(|msg| {
        msg.content
            .as_deref()
            .unwrap_or_default()
            .contains("Auto-compact disabled")
    }));
    let runtime = engine.runtime_state();
    assert_eq!(
        runtime.last_compaction_breaker_reason.as_deref(),
        Some("compression made no changes")
    );
    let mut saw_start = false;
    let mut saw_error = false;
    while let Ok(event) = rx.try_recv() {
        match event {
            EngineEvent::ContextCompactionStarted { mode } => {
                saw_start |= mode == "auto";
            }
            EngineEvent::Error(_) => saw_error = true,
            _ => {}
        }
    }
    assert!(saw_start);
    assert!(saw_error);
}

#[tokio::test]
async fn test_autocompact_prefers_session_memory_summary_when_available() {
    let mut engine = make_engine(vec![], vec![]);
    let project_root = engine.context().working_dir_compat();
    let live_path = crate::session_memory::live_session_memory_path(&project_root);
    std::fs::create_dir_all(live_path.parent().unwrap()).unwrap();
    std::fs::write(
        &live_path,
        "# Session Snapshot\n\n## 2026-01-01 session abc123\n\n- Goals: stabilize compact flow\n- Findings: older tool results dominate token usage\n- Decisions: use session memory first\n",
    )
    .unwrap();

    let big = "x".repeat(18_000);
    engine.messages = vec![
        Message::system("system"),
        Message::user(&big),
        Message::assistant(&big),
        Message::tool_result("tc1", &big),
        Message::user(&big),
        Message::assistant(&big),
        Message::user(&big),
        Message::assistant(&big),
        Message::user("recent1"),
        Message::assistant("recent2"),
        Message::user("recent3"),
        Message::assistant("recent4"),
        Message::user("recent5"),
        Message::assistant("recent6"),
        Message::user("recent7"),
        Message::assistant("recent8"),
    ];
    engine.current_query_source = QuerySource::User;

    let (tx, _rx) = mpsc::unbounded_channel();
    engine.maybe_compact_context(190_000, &tx).await;

    assert!(engine.messages.iter().any(|message| {
        message
            .content
            .as_deref()
            .unwrap_or_default()
            .contains("persisted session memory")
    }));
    let runtime = engine.runtime_state();
    assert_eq!(runtime.last_compaction_mode.as_deref(), Some("auto"));
    assert_eq!(
        runtime
            .compaction_cause_histogram
            .get("success_auto_session_memory"),
        Some(&1)
    );
}

#[test]
fn test_recovery_artifact_written_on_state_transition() {
    let mut engine = make_engine(vec![], vec![]);
    engine.last_failed_signature = Some("bash:{\"command\":\"cargo test\"}".to_string());
    engine.error_buckets.insert(ToolErrorType::Validation, 2);
    engine.update_recovery_state();

    assert_eq!(engine.recovery_state, RecoveryState::SingleStepMode);
    let artifact = engine
        .last_recovery_artifact_path
        .as_ref()
        .expect("recovery artifact should exist");
    let content = std::fs::read_to_string(artifact).unwrap();
    assert!(content.contains("SingleStepMode"));
    assert!(content.contains("Breadcrumbs"));
}

#[tokio::test]
async fn test_compact_query_source_skips_autocompact() {
    let mut engine = make_engine(vec![], vec![]);
    let big = "x".repeat(18_000);
    engine.messages = vec![
        Message::system("system"),
        Message::user(&big),
        Message::assistant(&big),
        Message::tool_result("tc1", &big),
        Message::user(&big),
        Message::assistant(&big),
        Message::user("recent1"),
        Message::assistant("recent2"),
        Message::user("recent3"),
        Message::assistant("recent4"),
        Message::user("recent5"),
        Message::assistant("recent6"),
    ];
    engine.current_query_source = QuerySource::Compact;

    let before_len = engine.messages.len();
    let (tx, _rx) = mpsc::unbounded_channel();
    engine.maybe_compact_context(190_000, &tx).await;

    assert_eq!(engine.messages.len(), before_len);
    assert_eq!(engine.compaction_failures, 0);
    assert!(!engine.messages.iter().any(|msg| {
        msg.content
            .as_deref()
            .unwrap_or_default()
            .starts_with("[Context summary]")
    }));
}

#[tokio::test]
async fn test_force_compact_ignores_auto_compact_guard() {
    let mut engine = make_engine(vec![], vec![]);
    let big = "x".repeat(18_000);
    engine.messages = vec![
        Message::system("system"),
        Message::user(&big),
        Message::assistant(&big),
        Message::tool_result("tc1", &big),
        Message::user(&big),
        Message::assistant(&big),
        Message::user("recent1"),
        Message::assistant("recent2"),
        Message::user("recent3"),
        Message::assistant("recent4"),
        Message::user("recent5"),
        Message::assistant("recent6"),
    ];
    engine.current_query_source = QuerySource::Compact;
    engine.autocompact_disabled = true;

    let (tx, _rx) = mpsc::unbounded_channel();
    let changed = engine.force_compact_keep_last(2, tx).await;

    assert!(changed);
    let runtime = engine.runtime_state();
    assert_eq!(runtime.total_compactions, 1);
    assert_eq!(runtime.manual_compactions, 1);
    assert!(
        engine.messages.len() < 12
            || engine.messages.iter().any(|msg| {
                msg.content
                    .as_deref()
                    .unwrap_or_default()
                    .contains("[compressed]")
            })
    );
}

#[tokio::test]
async fn test_force_compact_uses_full_post_compact_finalize_path() {
    let mut engine = make_engine(vec![], vec![]);
    let project_root = engine.context().working_dir_compat();
    let src_dir = project_root.join("src");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::write(
        src_dir.join("lib.rs"),
        "pub fn restored_context_marker() -> &'static str {\n    \"compact restore excerpt\"\n}\n",
    )
    .unwrap();
    let skill_dir = project_root.join(".yode/skills/rust");
    std::fs::create_dir_all(&skill_dir).unwrap();
    std::fs::write(
        skill_dir.join("SKILL.md"),
        "---\nname: rust\ndescription: Rust path guidance\npaths:\n  - src/**\n---\nPrefer cargo test.\n",
    )
    .unwrap();
    engine
        .skill_invocation_store
        .lock()
        .await
        .push(yode_tools::builtin::skill::SkillInvocation {
            sequence: 1,
            name: "rust".to_string(),
            description: "Rust path guidance".to_string(),
            action: "get".to_string(),
            content_excerpt: "Prefer cargo test. Preserve this skill hint after compact."
                .to_string(),
            content_truncated: false,
            session_id: Some(engine.context().session_id.clone()),
            subagent_description: None,
            subagent_type: None,
            team_id: None,
            member_id: None,
        });
    engine.files_read.insert("src/lib.rs".to_string(), 3);
    engine.pending_cache_edit_refs = vec!["tc1".to_string()];
    engine.pinned_cache_edit_refs = vec!["tc0".to_string()];
    engine.last_prompt_cache_prefix_hash = Some("prefix-before-compact".to_string());
    let plan_dir = project_root.join(".yode/plans");
    std::fs::create_dir_all(&plan_dir).unwrap();
    let short_session = engine
        .context()
        .session_id
        .chars()
        .take(8)
        .collect::<String>();
    let plan_path = plan_dir.join(format!("{}-plan.md", short_session));
    std::fs::write(&plan_path, "# Active Plan\n\n- Keep compact state.").unwrap();
    assert!(engine.set_runtime_plan_mode(true));
    let task_output = project_root.join(".yode/tasks/task-1.log");
    std::fs::create_dir_all(task_output.parent().unwrap()).unwrap();
    std::fs::write(&task_output, "task output").unwrap();
    let task = engine
        .create_runtime_task(
            "agent",
            "spawn_agent",
            "continue compact validation",
            &task_output.display().to_string(),
            Some(
                project_root
                    .join(".yode/tasks/task-1.md")
                    .display()
                    .to_string(),
            ),
        )
        .expect("task created");
    engine.mark_runtime_task_running(&task.id);
    engine.update_runtime_task_progress(&task.id, "exploring restore state");

    let big = "x".repeat(18_000);
    engine.messages = vec![
        Message::system("system"),
        Message::user(&big),
        Message::assistant(&big),
        Message::tool_result("tc1", &big),
        Message::user(&big),
        Message::assistant(&big),
        Message::user("recent1"),
        Message::assistant("recent2"),
        Message::user("recent3"),
        Message::assistant("recent4"),
        Message::user("recent5"),
        Message::assistant("recent6"),
    ];

    let (tx, _rx) = mpsc::unbounded_channel();
    let changed = engine.force_compact_keep_last(2, tx).await;

    assert!(changed);
    assert_eq!(
        engine.forced_prompt_cache_expected_drop_reason.as_deref(),
        Some("compaction_manual")
    );

    let runtime = engine.runtime_state();
    assert_eq!(
        runtime
            .prompt_cache
            .last_prompt_cache_expected_drop_reason
            .as_deref(),
        Some("compaction_manual")
    );
    let request = engine.build_chat_request();
    assert!(!request.provider_hints.restore_system_blocks.is_empty());
    assert!(request
        .provider_hints
        .restore_system_blocks
        .iter()
        .any(|block| block.kind == "files"
            && block.content.contains("restored_context_marker")
            && block.content.contains("compact restore excerpt")));
    assert!(request
        .provider_hints
        .restore_system_blocks
        .iter()
        .any(|block| block.kind == "skills"
            && block.content.contains("Path-gated active skills")
            && block.content.contains("Recently invoked skills")
            && block.content.contains("Preserve this skill hint")
            && block.content.contains("rust")));
    assert!(request
        .provider_hints
        .restore_system_blocks
        .iter()
        .any(|block| block.kind == "prompt-cache"
            && block
                .content
                .contains("Expected next drop: compaction_manual")
            && block
                .content
                .contains("Active cache edits: pending=1 pinned=1")
            && block.content.contains("prefix=prefix-before-compact")));
    assert!(request
        .provider_hints
        .restore_system_blocks
        .iter()
        .any(|block| block.kind == "plan"
            && block.content.contains("- Plan mode: enabled")
            && block.content.contains("- Active plan file:")
            && block.content.contains("Restore contract")));
    assert!(request
        .provider_hints
        .restore_system_blocks
        .iter()
        .any(|block| block.kind == "tasks"
            && block.content.contains(&task.id)
            && block.content.contains("exploring restore state")
            && block.content.contains("do not respawn")));
    let runtime_after_request = engine.runtime_state();
    assert!(runtime_after_request
        .last_post_compaction_estimated_tokens
        .is_some());
    assert!(runtime_after_request
        .last_post_compaction_threshold_tokens
        .is_some());
    assert!(runtime_after_request
        .last_post_compaction_will_retrigger
        .is_some());
    assert_eq!(
        runtime_after_request.prompt_cache.pending_cache_edit_refs,
        0
    );
    assert_eq!(runtime_after_request.prompt_cache.pinned_cache_edit_refs, 0);
    let restore_budget = runtime_after_request
        .last_restore_budget
        .as_ref()
        .expect("restore budget");
    assert!(restore_budget.used_tokens <= restore_budget.total_tokens);
    assert_eq!(restore_budget.total_tokens, 5_000);
    assert!(restore_budget
        .entries
        .iter()
        .any(|entry| entry.kind == "files"));
    let boundary = runtime_after_request
        .last_compact_boundary
        .as_ref()
        .expect("compact boundary record");
    assert_eq!(boundary.mode, "manual");
    assert!(boundary.removed_count > 0);
    assert_eq!(
        boundary.post_compact_estimated_tokens,
        runtime_after_request
            .last_post_compaction_estimated_tokens
            .unwrap()
    );
    assert_eq!(
        boundary.post_compact_token_delta,
        boundary.post_compact_estimated_tokens as i64
            - boundary.post_compact_threshold_tokens as i64
    );
    assert!(boundary.summary_fingerprint.is_some());
    assert!(boundary.preserved_tail_range.is_some());

    let restore_artifact_path = project_root
        .join(".yode/status")
        .join(format!("{}-post-compact-restore.md", short_session));
    assert!(restore_artifact_path.exists());
    let restore_artifact = std::fs::read_to_string(restore_artifact_path).unwrap();
    assert!(restore_artifact.contains("Post-compact pressure:"));
    assert!(restore_artifact.contains("next_auto="));
    assert!(restore_artifact.contains("## Compact Boundary"));
    assert!(restore_artifact.contains("## Restore Budget"));
    assert!(restore_artifact.contains("[Post-compact restore: tasks]"));
    assert!(restore_artifact.contains("| Block | Used | Cap | Truncated | Reason |"));
    assert!(restore_artifact.contains("\"removed_count\""));
    let restore_state_path = project_root
        .join(".yode/status")
        .join(format!("{}-post-compact-restore-state.json", short_session));
    assert!(restore_state_path.exists());
    let restore_state = std::fs::read_to_string(restore_state_path).unwrap();
    assert!(restore_state.contains("\"restore_budget\""));
    assert!(boundary
        .artifact_paths
        .iter()
        .any(|path| path.ends_with("-post-compact-restore.md")));
}

#[test]
fn test_prompt_too_long_errors_trigger_reactive_compact_detection() {
    let engine = make_engine(vec![], vec![]);
    let err = anyhow::anyhow!(
        "OpenAI API error (400): This model's maximum context length is 128000 tokens."
    );
    assert!(engine.should_reactive_compact_error(&err));
}

#[test]
fn test_media_errors_trigger_reactive_strip_detection() {
    let engine = make_engine(vec![], vec![]);
    let err = anyhow::anyhow!("Anthropic API error (400): image exceeds maximum size");
    assert!(engine.should_reactive_strip_media_error(&err));
}

#[test]
fn test_reactive_strip_old_media_clears_images_from_older_messages() {
    let mut engine = make_engine(vec![], vec![]);
    engine.messages = vec![
        Message::system("system"),
        Message::user_with_images(
            "older image",
            vec![ImageData {
                base64: "abcd".to_string(),
                media_type: "image/png".to_string(),
            }],
        ),
        Message::assistant("a1"),
        Message::user("recent1"),
        Message::assistant("recent2"),
        Message::user("recent3"),
        Message::assistant("recent4"),
        Message::user("recent5"),
        Message::assistant("recent6"),
        Message::user("recent7"),
        Message::assistant("recent8"),
    ];

    let changed = engine.reactive_strip_old_media();

    assert!(changed);
    assert!(engine.messages[1].images.is_empty());
    assert!(engine.messages[1]
        .content
        .as_deref()
        .unwrap_or_default()
        .contains("older media removed"));
}

#[test]
fn test_apply_microcompact_proactively_clears_old_media() {
    let mut engine = make_engine(vec![], vec![]);
    engine.messages = vec![
        Message::system("system"),
        Message::user_with_images(
            "older image",
            vec![ImageData {
                base64: "abcd".repeat(1_024),
                media_type: "image/png".to_string(),
            }],
        ),
        Message::assistant("a1"),
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

    assert!(engine.messages[1].images.is_empty());
    assert!(engine.messages[1]
        .content
        .as_deref()
        .unwrap_or_default()
        .contains("Older media microcompacted"));
    assert_eq!(
        engine.compaction_cause_histogram.get("microcompact_media"),
        Some(&1)
    );
    let runtime = engine.runtime_state();
    assert_eq!(runtime.last_microcompact_media_removed, 1);
    assert_eq!(runtime.microcompact_media_removed_total, 1);
    assert!(runtime.microcompact_media_saved_chars_total > 0);
}

#[test]
fn test_reactive_compact_prefers_prefix_range_from_token_gap() {
    let mut engine = make_engine(vec![], vec![]);
    let big = "x".repeat(8_000);
    engine.messages = vec![
        Message::system("system"),
        Message::user(&big),
        Message::assistant(&big),
        Message::user(&big),
        Message::assistant(&big),
        Message::user("recent1"),
        Message::assistant("recent2"),
        Message::user("recent3"),
        Message::assistant("recent4"),
        Message::user("recent5"),
        Message::assistant("recent6"),
        Message::user("recent7"),
        Message::assistant("recent8"),
    ];

    let end = engine
        .reactive_prefix_end_for_token_gap("prompt is too long: 16000 tokens > 12000 maximum")
        .expect("should derive a reactive prefix");
    assert!(end > 1);
    assert!(end < engine.messages.len().saturating_sub(6));
}

#[tokio::test]
async fn test_post_compact_file_restore_prefers_recent_and_skips_preserved_tail_reads() {
    let mut engine = make_engine(vec![], vec![]);
    let project_root = engine.context().working_dir_compat();
    let src_dir = project_root.join("src");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::write(
        src_dir.join("older.rs"),
        "pub const OLDER: &str = \"older\";\n",
    )
    .unwrap();
    std::fs::write(
        src_dir.join("middle.rs"),
        "pub const MIDDLE: &str = \"middle\";\n",
    )
    .unwrap();
    std::fs::write(
        src_dir.join("recent.rs"),
        "pub const RECENT: &str = \"recent\";\n",
    )
    .unwrap();

    engine.files_read.insert("src/older.rs".to_string(), 1);
    engine.files_read.insert("src/middle.rs".to_string(), 1);
    engine.files_read.insert("src/recent.rs".to_string(), 1);
    engine.recent_file_reads = vec![
        "src/older.rs".to_string(),
        "src/middle.rs".to_string(),
        "src/recent.rs".to_string(),
    ];

    let mut read_recent = Message::assistant("reading recent");
    read_recent.tool_calls = vec![ToolCall {
        id: "read-recent".to_string(),
        name: "read_file".to_string(),
        arguments: r#"{"file_path":"src/recent.rs"}"#.to_string(),
    }];
    let big = "x".repeat(18_000);
    engine.messages = vec![
        Message::system("system"),
        Message::user(&big),
        Message::assistant(&big),
        Message::user("older turn"),
        Message::assistant("older response"),
        read_recent,
        Message::tool_result("read-recent", "pub const RECENT: &str = \"recent\";"),
        Message::user("continue"),
        Message::assistant("ok"),
    ];

    let (tx, _rx) = mpsc::unbounded_channel();
    assert!(engine.force_compact_keep_last(4, tx).await);

    let request = engine.build_chat_request();
    let files_block = request
        .provider_hints
        .restore_system_blocks
        .iter()
        .find(|block| block.kind == "files")
        .expect("files restore block");
    assert!(files_block.content.contains("src/middle.rs"));
    assert!(files_block.content.contains("MIDDLE"));
    assert!(files_block.content.contains("src/older.rs"));
    assert!(!files_block.content.contains("Excerpt from src/recent.rs"));

    let short_session = engine
        .context()
        .session_id
        .chars()
        .take(8)
        .collect::<String>();
    let restore_artifact = std::fs::read_to_string(
        project_root
            .join(".yode/status")
            .join(format!("{}-post-compact-restore.md", short_session)),
    )
    .unwrap();
    assert!(restore_artifact.contains("Skipped excerpts already preserved in tail: src/recent.rs"));
}

#[tokio::test]
async fn test_partial_compact_up_to_keeps_newer_tail() {
    let mut engine = make_engine(vec![], vec![]);
    engine.messages = vec![
        Message::system("system"),
        Message::user("u1"),
        Message::assistant("a1"),
        Message::user("u2"),
        Message::assistant("a2"),
        Message::user("u3"),
        Message::assistant("a3"),
        Message::user("u4"),
        Message::assistant("a4"),
    ];

    let (tx, _rx) = mpsc::unbounded_channel();
    let changed = engine.force_partial_compact_up_to(4, tx).await;

    assert!(changed);
    assert!(engine.messages.iter().any(|message| {
        message
            .content
            .as_deref()
            .unwrap_or_default()
            .starts_with("[Context summary]")
    }));
    assert!(engine
        .messages
        .iter()
        .any(|message| message.content.as_deref() == Some("u3")));
    assert!(engine
        .messages
        .iter()
        .any(|message| message.content.as_deref() == Some("a4")));
    let boundary = engine
        .runtime_state()
        .last_compact_boundary
        .expect("partial up_to boundary");
    assert_eq!(boundary.mode, "manual");
    assert_eq!(boundary.removed_count, 4);
    assert!(boundary.post_compact_estimated_tokens > 0);
    assert!(boundary
        .artifact_paths
        .iter()
        .any(|path| path.replace('\\', "/").contains(".yode/transcripts/")));
}

#[tokio::test]
async fn test_partial_compact_from_keeps_older_prefix() {
    let mut engine = make_engine(vec![], vec![]);
    engine.messages = vec![
        Message::system("system"),
        Message::user("u1"),
        Message::assistant("a1"),
        Message::user("u2"),
        Message::assistant("a2"),
        Message::user("u3"),
        Message::assistant("a3"),
        Message::user("u4"),
        Message::assistant("a4"),
    ];

    let (tx, _rx) = mpsc::unbounded_channel();
    let changed = engine.force_partial_compact_from(5, tx).await;

    assert!(changed);
    assert_eq!(engine.messages[1].content.as_deref(), Some("u1"));
    assert_eq!(engine.messages[2].content.as_deref(), Some("a1"));
    assert!(engine.messages.iter().any(|message| {
        message
            .content
            .as_deref()
            .unwrap_or_default()
            .starts_with("[Context summary]")
    }));
    assert!(!engine.messages.iter().any(|message| {
        message
            .content
            .as_deref()
            .unwrap_or_default()
            .starts_with("[Post-compact restore:")
    }));

    let request = engine.build_chat_request();
    assert!(!request.provider_hints.restore_system_blocks.is_empty());
    let boundary = engine
        .runtime_state()
        .last_compact_boundary
        .expect("partial from boundary");
    assert_eq!(boundary.mode, "manual");
    assert_eq!(boundary.removed_count, 4);
    assert!(boundary.post_compact_estimated_tokens > 0);
}

#[tokio::test]
async fn test_reactive_compact_records_boundary() {
    let mut engine = make_engine(vec![], vec![]);
    let big = "x".repeat(200_000);
    engine.messages = vec![
        Message::system("system"),
        Message::user(&big),
        Message::assistant(&big),
        Message::tool_result("tc1", &big),
        Message::user(&big),
        Message::assistant(&big),
        Message::user("recent1"),
        Message::assistant("recent2"),
        Message::user("recent3"),
        Message::assistant("recent4"),
        Message::user("recent5"),
        Message::assistant("recent6"),
    ];

    let (tx, _rx) = mpsc::unbounded_channel();
    assert!(
        engine
            .reactive_compact_context_for_text("context window exceeded", &tx)
            .await
    );
    let boundary = engine
        .runtime_state()
        .last_compact_boundary
        .expect("reactive boundary");
    assert_eq!(boundary.mode, "reactive");
    assert!(boundary.removed_count > 0);
    assert!(boundary.post_compact_token_delta < boundary.post_compact_estimated_tokens as i64);
    assert!(boundary
        .artifact_paths
        .iter()
        .any(|path| path.ends_with("-post-compact-restore-state.json")));
}

#[tokio::test]
async fn test_partial_compact_summary_prompt_is_direction_aware() {
    let mock_provider = MockProvider::new("stub-summary")
        .with_chat_response(summary_response(
            "## Goals\n- preserve older prefix\n## Current State\n- up_to compacted\n## Findings\n- None\n## Decisions\n- None\n## Files\n- None\n## Tools\n- None\n## Constraints\n- None\n## Open Questions\n- None\n## Next Steps\n- continue",
        ))
        .with_chat_response(summary_response(
            "## Goals\n- preserve earlier messages\n## Current State\n- from compacted\n## Findings\n- None\n## Decisions\n- None\n## Files\n- None\n## Tools\n- None\n## Constraints\n- None\n## Open Questions\n- None\n## Next Steps\n- continue",
        ));
    let provider: Arc<dyn yode_llm::provider::LlmProvider> = Arc::new(mock_provider.clone());
    let mut engine = make_engine_with_provider(provider);
    engine.set_model("gpt-3.5".to_string());
    engine.messages = vec![
        Message::system("system"),
        Message::user("u1"),
        Message::assistant("a1"),
        Message::user("u2"),
        Message::assistant("a2"),
        Message::user("u3"),
        Message::assistant("a3"),
        Message::user("u4"),
        Message::assistant("a4"),
    ];

    let (tx, _rx) = mpsc::unbounded_channel();
    assert!(engine.force_partial_compact_up_to(4, tx.clone()).await);
    assert!(engine.force_partial_compact_from(5, tx).await);

    let requests = mock_provider.requests();
    assert!(requests.len() >= 2);
    let prompts = requests
        .iter()
        .filter_map(|request| request.messages.get(1)?.content.as_deref())
        .collect::<Vec<_>>();
    let up_to_prompt = prompts
        .iter()
        .find(|prompt| prompt.contains("Scope: partial compact up_to"))
        .copied()
        .unwrap_or_default();
    let from_prompt = prompts
        .iter()
        .find(|prompt| prompt.contains("Scope: partial compact from"))
        .copied()
        .unwrap_or_default();
    assert!(up_to_prompt.contains("Scope: partial compact up_to"));
    assert!(up_to_prompt.contains("older prefix"));
    assert!(from_prompt.contains("Scope: partial compact from"));
    assert!(from_prompt.contains("later tail"));
}

#[tokio::test]
async fn test_repeated_compaction_does_not_duplicate_restore_blocks() {
    let mut engine = make_engine(vec![], vec![]);
    let big = "x".repeat(18_000);
    engine.messages = vec![
        Message::system("system"),
        Message::user(&big),
        Message::assistant(&big),
        Message::tool_result("tc1", &big),
        Message::user(&big),
        Message::assistant(&big),
        Message::user("recent1"),
        Message::assistant("recent2"),
        Message::user("recent3"),
        Message::assistant("recent4"),
        Message::user("recent5"),
        Message::assistant("recent6"),
    ];

    let (tx, _rx) = mpsc::unbounded_channel();
    assert!(engine.force_compact_keep_last(4, tx.clone()).await);
    assert!(engine.force_compact_keep_last(4, tx).await);

    let restore_count = engine
        .messages
        .iter()
        .filter(|message| {
            message
                .content
                .as_deref()
                .unwrap_or_default()
                .starts_with("[Post-compact restore:")
        })
        .count();
    assert_eq!(restore_count, 0);

    let request = engine.build_chat_request();
    let request_restore_count = request.provider_hints.restore_system_blocks.len();
    assert!(request_restore_count <= 9);
}

#[tokio::test]
async fn test_manual_compaction_uses_llm_structured_summary_when_available() {
    let provider: Arc<dyn yode_llm::provider::LlmProvider> =
        Arc::new(MockProvider::new("stub-summary").with_chat_response(summary_response(
            "## Goals\n- finish compaction parity\n## Current State\n- context compacted\n## Findings\n- older tool results were dominating\n## Decisions\n- use structured summary\n## Files\n- src/engine.rs\n## Tools\n- read_file\n## Constraints\n- keep it concise\n## Open Questions\n- None\n## Next Steps\n- continue with recent tail",
        )));
    let mut engine = make_engine_with_provider(provider);
    engine.set_model("gpt-3.5".to_string());
    let big = "x".repeat(18_000);
    engine.messages = vec![
        Message::system("system"),
        Message::user(&big),
        Message::assistant(&big),
        Message::tool_result("tc1", &big),
        Message::user(&big),
        Message::assistant(&big),
        Message::user("recent1"),
        Message::assistant("recent2"),
        Message::user("recent3"),
        Message::assistant("recent4"),
        Message::user("recent5"),
        Message::assistant("recent6"),
    ];

    let (tx, _rx) = mpsc::unbounded_channel();
    let changed = engine.force_compact(tx).await;

    assert!(changed);
    assert!(engine.messages.iter().any(|message| {
        message
            .content
            .as_deref()
            .unwrap_or_default()
            .contains("LLM-generated structured summary")
    }));
    assert!(engine.messages.iter().any(|message| {
        message
            .content
            .as_deref()
            .unwrap_or_default()
            .contains("## Goals")
    }));
}

#[tokio::test]
async fn test_manual_compaction_retries_llm_summary_after_prompt_too_long() {
    let mock_provider = MockProvider::new("stub-summary")
        .with_chat_error("prompt too long")
        .with_chat_response(summary_response(
            "## Goals\n- recover compaction summary\n## Current State\n- retry succeeded\n## Findings\n- head truncation worked\n## Decisions\n- retry once\n## Files\n- None\n## Tools\n- None\n## Constraints\n- concise\n## Open Questions\n- None\n## Next Steps\n- continue",
        ));
    let provider: Arc<dyn yode_llm::provider::LlmProvider> = Arc::new(mock_provider.clone());
    let mut engine = make_engine_with_provider(provider);
    engine.set_model("gpt-3.5".to_string());
    let big = "x".repeat(18_000);
    engine.messages = vec![
        Message::system("system"),
        Message::user(&big),
        Message::assistant(&big),
        Message::tool_result("tc1", &big),
        Message::user(&big),
        Message::assistant(&big),
        Message::user("recent1"),
        Message::assistant("recent2"),
        Message::user("recent3"),
        Message::assistant("recent4"),
        Message::user("recent5"),
        Message::assistant("recent6"),
    ];

    let (tx, _rx) = mpsc::unbounded_channel();
    let changed = engine.force_compact_keep_last(2, tx).await;

    assert!(changed);
    assert_eq!(mock_provider.requests().len(), 2);
    assert!(engine.messages.iter().any(|message| {
        message
            .content
            .as_deref()
            .unwrap_or_default()
            .contains("retry succeeded")
    }));
}