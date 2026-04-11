use super::*;
use std::sync::Arc;
use yode_llm::types::ToolCall;
use yode_tools::registry::ToolRegistry;
use yode_tools::tool::{Tool, ToolCapabilities, ToolContext, ToolResult};

/// Minimal mock LLM provider (never actually called in these tests).
struct MockProvider;

#[async_trait::async_trait]
impl yode_llm::provider::LlmProvider for MockProvider {
    fn name(&self) -> &str {
        "mock"
    }
    async fn chat(
        &self,
        _req: yode_llm::types::ChatRequest,
    ) -> anyhow::Result<yode_llm::types::ChatResponse> {
        unimplemented!("Mock provider should not be called in unit tests")
    }
    async fn chat_stream(
        &self,
        _req: yode_llm::types::ChatRequest,
        _tx: tokio::sync::mpsc::Sender<yode_llm::types::StreamEvent>,
    ) -> anyhow::Result<()> {
        unimplemented!()
    }
    async fn list_models(&self) -> anyhow::Result<Vec<yode_llm::ModelInfo>> {
        Ok(vec![])
    }
}

/// A mock read-only tool for testing parallel execution.
struct MockReadTool {
    name: String,
}

#[async_trait::async_trait]
impl Tool for MockReadTool {
    fn name(&self) -> &str {
        &self.name
    }
    fn description(&self) -> &str {
        "mock read tool"
    }
    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({"type": "object", "properties": {}})
    }
    fn capabilities(&self) -> ToolCapabilities {
        ToolCapabilities {
            requires_confirmation: false,
            supports_auto_execution: true,
            read_only: true,
        }
    }
    async fn execute(
        &self,
        _params: serde_json::Value,
        _ctx: &ToolContext,
    ) -> anyhow::Result<ToolResult> {
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        Ok(ToolResult::success(format!("result from {}", self.name)))
    }
}

/// A mock write tool that requires confirmation.
struct MockWriteTool;

#[async_trait::async_trait]
impl Tool for MockWriteTool {
    fn name(&self) -> &str {
        "mock_write"
    }
    fn description(&self) -> &str {
        "mock write tool"
    }
    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({"type": "object", "properties": {}})
    }
    fn capabilities(&self) -> ToolCapabilities {
        ToolCapabilities {
            requires_confirmation: true,
            supports_auto_execution: false,
            read_only: false,
        }
    }
    async fn execute(
        &self,
        _params: serde_json::Value,
        _ctx: &ToolContext,
    ) -> anyhow::Result<ToolResult> {
        Ok(ToolResult::success("write done".to_string()))
    }
}

struct MockPathTool;

#[async_trait::async_trait]
impl Tool for MockPathTool {
    fn name(&self) -> &str {
        "mock_path"
    }
    fn description(&self) -> &str {
        "mock path tool"
    }
    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" }
            },
            "required": ["path"]
        })
    }
    fn capabilities(&self) -> ToolCapabilities {
        ToolCapabilities {
            requires_confirmation: false,
            supports_auto_execution: true,
            read_only: false,
        }
    }
    async fn execute(
        &self,
        params: serde_json::Value,
        _ctx: &ToolContext,
    ) -> anyhow::Result<ToolResult> {
        let path = params
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or("missing");
        Ok(ToolResult::success(format!("path={}", path)))
    }
}

fn make_engine(tools: Vec<Arc<dyn Tool>>, confirm_tools: Vec<String>) -> AgentEngine {
    let mut registry = ToolRegistry::new();
    for t in tools {
        registry.register(t);
    }
    let provider: Arc<dyn yode_llm::provider::LlmProvider> = Arc::new(MockProvider);
    let permissions = PermissionManager::from_confirmation_list(confirm_tools);
    let workdir = std::env::temp_dir().join(format!("yode-engine-test-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&workdir).unwrap();
    let context = AgentContext::new(workdir, "mock".to_string(), "claude-sonnet-4".to_string());
    AgentEngine::new(provider, Arc::new(registry), permissions, context)
}

#[test]
fn test_partition_all_read_only() {
    let engine = make_engine(
        vec![
            Arc::new(MockReadTool { name: "r1".into() }),
            Arc::new(MockReadTool { name: "r2".into() }),
            Arc::new(MockReadTool { name: "r3".into() }),
        ],
        vec![],
    );
    let tcs = vec![
        ToolCall {
            id: "1".into(),
            name: "r1".into(),
            arguments: "{}".into(),
        },
        ToolCall {
            id: "2".into(),
            name: "r2".into(),
            arguments: "{}".into(),
        },
        ToolCall {
            id: "3".into(),
            name: "r3".into(),
            arguments: "{}".into(),
        },
    ];
    let (par, seq) = engine.partition_tool_calls(&tcs);
    assert_eq!(par.len(), 3);
    assert_eq!(seq.len(), 0);
}

#[test]
fn test_partition_mixed() {
    let engine = make_engine(
        vec![
            Arc::new(MockReadTool {
                name: "reader".into(),
            }),
            Arc::new(MockWriteTool),
        ],
        vec!["mock_write".into()],
    );
    let tcs = vec![
        ToolCall {
            id: "1".into(),
            name: "reader".into(),
            arguments: "{}".into(),
        },
        ToolCall {
            id: "2".into(),
            name: "mock_write".into(),
            arguments: "{}".into(),
        },
        ToolCall {
            id: "3".into(),
            name: "reader".into(),
            arguments: "{}".into(),
        },
    ];
    let (par, seq) = engine.partition_tool_calls(&tcs);
    assert_eq!(par.len(), 2);
    assert_eq!(seq.len(), 1);
    assert_eq!(seq[0].name, "mock_write");
}

#[test]
fn test_partition_unknown_tool() {
    let engine = make_engine(vec![], vec![]);
    let tcs = vec![ToolCall {
        id: "1".into(),
        name: "nonexistent".into(),
        arguments: "{}".into(),
    }];
    let (par, seq) = engine.partition_tool_calls(&tcs);
    assert_eq!(par.len(), 0);
    assert_eq!(seq.len(), 1);
}

#[test]
fn test_partition_read_only_needing_confirm() {
    let engine = make_engine(
        vec![Arc::new(MockReadTool {
            name: "sensitive".into(),
        })],
        vec!["sensitive".into()],
    );
    let tcs = vec![ToolCall {
        id: "1".into(),
        name: "sensitive".into(),
        arguments: "{}".into(),
    }];
    let (par, seq) = engine.partition_tool_calls(&tcs);
    assert_eq!(par.len(), 0, "Confirm-required tools must not be parallelized");
    assert_eq!(seq.len(), 1);
}

#[tokio::test]
async fn test_parallel_returns_all_results_in_order() {
    let mut engine = make_engine(
        vec![
            Arc::new(MockReadTool { name: "a".into() }),
            Arc::new(MockReadTool { name: "b".into() }),
            Arc::new(MockReadTool { name: "c".into() }),
        ],
        vec![],
    );
    let tcs = vec![
        ToolCall {
            id: "x1".into(),
            name: "a".into(),
            arguments: "{}".into(),
        },
        ToolCall {
            id: "x2".into(),
            name: "b".into(),
            arguments: "{}".into(),
        },
        ToolCall {
            id: "x3".into(),
            name: "c".into(),
            arguments: "{}".into(),
        },
    ];
    let (tx, mut rx) = mpsc::unbounded_channel();
    let results = engine.execute_tools_parallel(&tcs, &tx).await;

    assert_eq!(results.len(), 3);
    assert_eq!(results[0].tool_call.id, "x1");
    assert_eq!(results[1].tool_call.id, "x2");
    assert_eq!(results[2].tool_call.id, "x3");
    for outcome in &results {
        assert!(!outcome.result.is_error);
    }

    let mut starts = 0;
    while let Ok(ev) = rx.try_recv() {
        if matches!(ev, EngineEvent::ToolCallStart { .. }) {
            starts += 1;
        }
    }
    assert_eq!(starts, 3);
}

#[tokio::test]
async fn test_parallel_empty() {
    let mut engine = make_engine(vec![], vec![]);
    let (tx, _rx) = mpsc::unbounded_channel();
    let results = engine.execute_tools_parallel(&[], &tx).await;
    assert!(results.is_empty());
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
        engine.maybe_compact_context(160_000, &tx).await;
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
    assert!(matches!(rx.try_recv(), Ok(EngineEvent::Error(_))));
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
    engine.maybe_compact_context(160_000, &tx).await;

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
    let changed = engine.force_compact(tx).await;

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
async fn test_initialize_session_hooks_injects_system_context() {
    let mut engine = make_engine(vec![], vec![]);
    let hook_dir =
        std::env::temp_dir().join(format!("yode-session-hook-test-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&hook_dir).unwrap();
    let mut hook_mgr = crate::hooks::HookManager::new(hook_dir);
    hook_mgr.register(crate::hooks::HookDefinition {
        command: "echo session context".into(),
        events: vec!["session_start".into()],
        tool_filter: None,
        timeout_secs: 5,
        can_block: false,
    });
    engine.set_hook_manager(hook_mgr);

    engine.initialize_session_hooks("startup").await;

    assert!(engine.messages.iter().any(|msg| {
        msg.content
            .as_deref()
            .unwrap_or_default()
            .contains("session_start hooks")
    }));
    assert!(engine.messages.iter().any(|msg| {
        msg.content
            .as_deref()
            .unwrap_or_default()
            .contains("session context")
    }));
}

#[tokio::test]
async fn test_session_start_hook_wake_notification_is_injected() {
    let mut engine = make_engine(vec![], vec![]);
    let hook_dir =
        std::env::temp_dir().join(format!("yode-session-hook-wake-test-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&hook_dir).unwrap();
    let mut hook_mgr = crate::hooks::HookManager::new(hook_dir);
    hook_mgr.register(crate::hooks::HookDefinition {
        command:
            "printf '%s' '{\"hookSpecificOutput\":{\"wakeNotification\":\"background hook finished\"}}' && exit 2"
                .into(),
        events: vec!["session_start".into()],
        tool_filter: None,
        timeout_secs: 5,
        can_block: false,
    });
    engine.set_hook_manager(hook_mgr);

    engine.initialize_session_hooks("startup").await;

    assert!(engine.messages.iter().any(|msg| {
        msg.content
            .as_deref()
            .unwrap_or_default()
            .contains("[Hook Wake via session_start")
    }));
    assert!(engine.messages.iter().any(|msg| {
        msg.content
            .as_deref()
            .unwrap_or_default()
            .contains("background hook finished")
    }));
}

#[tokio::test]
async fn test_pre_compact_hook_context_includes_runtime_metadata() {
    let mut engine = make_engine(vec![], vec![]);
    let hook_dir =
        std::env::temp_dir().join(format!("yode-compact-hook-test-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&hook_dir).unwrap();
    let dump_path = hook_dir.join("pre-compact-context.json");
    let mut hook_mgr = crate::hooks::HookManager::new(hook_dir.clone());
    hook_mgr.register(crate::hooks::HookDefinition {
        command: format!(
            "printf '%s' \"$YODE_HOOK_CONTEXT\" > {}",
            dump_path.display()
        ),
        events: vec!["pre_compact".into()],
        tool_filter: None,
        timeout_secs: 5,
        can_block: false,
    });
    engine.set_hook_manager(hook_mgr);
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
    engine.recovery_state = RecoveryState::SingleStepMode;
    engine.recovery_single_step_count = 2;
    engine.last_failed_signature = Some("bash:{\"command\":\"cargo test\"}".to_string());

    let (tx, _rx) = mpsc::unbounded_channel();
    let _ = engine.force_compact(tx).await;

    let value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dump_path).unwrap()).unwrap();
    let runtime = value
        .get("metadata")
        .and_then(|v| v.get("runtime"))
        .and_then(|v| v.as_object())
        .unwrap();
    assert!(runtime.contains_key("total_compactions"));
    assert!(runtime.contains_key("live_session_memory_initialized"));
    assert!(runtime.contains_key("session_memory_update_count"));
    assert_eq!(
        runtime.get("recovery_state").and_then(|v| v.as_str()),
        Some("SingleStepMode")
    );
    assert_eq!(
        runtime
            .get("last_failed_signature")
            .and_then(|v| v.as_str()),
        Some("bash:{\"command\":\"cargo test\"}")
    );
}

#[tokio::test]
async fn test_append_hook_outputs_as_system_message_injects_context() {
    let mut engine = make_engine(vec![], vec![]);
    let hook_dir =
        std::env::temp_dir().join(format!("yode-user-prompt-hook-test-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&hook_dir).unwrap();
    let mut hook_mgr = crate::hooks::HookManager::new(hook_dir);
    hook_mgr.register(crate::hooks::HookDefinition {
        command: "echo prompt context".into(),
        events: vec!["user_prompt_submit".into()],
        tool_filter: None,
        timeout_secs: 5,
        can_block: false,
    });
    engine.set_hook_manager(hook_mgr);

    let ctx = HookContext {
        event: HookEvent::UserPromptSubmit.to_string(),
        session_id: engine.context().session_id.clone(),
        working_dir: engine.context().working_dir_compat().display().to_string(),
        tool_name: None,
        tool_input: None,
        tool_output: None,
        error: None,
        user_prompt: Some("hello".to_string()),
        metadata: None,
    };
    engine
        .append_hook_outputs_as_system_message(
            HookEvent::UserPromptSubmit,
            ctx,
            "System Auto-Context via user_prompt_submit hooks",
        )
        .await;

    assert!(engine.messages.iter().any(|msg| {
        msg.content
            .as_deref()
            .unwrap_or_default()
            .contains("prompt context")
    }));
}

#[tokio::test]
async fn test_pre_tool_use_hook_can_modify_input() {
    let mut engine = make_engine(vec![Arc::new(MockPathTool)], vec![]);
    let hook_dir =
        std::env::temp_dir().join(format!("yode-modify-hook-test-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&hook_dir).unwrap();
    let mut hook_mgr = crate::hooks::HookManager::new(hook_dir);
    hook_mgr.register(crate::hooks::HookDefinition {
        command: "printf '%s' '{\"updatedInput\":{\"path\":\"new.txt\"}}'".into(),
        events: vec!["pre_tool_use".into()],
        tool_filter: Some(vec!["mock_path".into()]),
        timeout_secs: 5,
        can_block: false,
    });
    engine.set_hook_manager(hook_mgr);

    let tool_call = ToolCall {
        id: "tc1".into(),
        name: "mock_path".into(),
        arguments: "{\"path\":\"old.txt\"}".into(),
    };
    let (event_tx, _event_rx) = mpsc::unbounded_channel();
    let (_confirm_tx, mut confirm_rx) = mpsc::unbounded_channel();

    let result = engine
        .handle_tool_call(&tool_call, &event_tx, &mut confirm_rx, None)
        .await
        .unwrap();

    assert_eq!(result.result.content, "path=new.txt");
}

#[tokio::test]
async fn test_permission_hook_metadata_uses_effective_input_snapshot() {
    let mut engine = make_engine(vec![Arc::new(MockPathTool)], vec!["mock_path".into()]);
    let hook_dir = std::env::temp_dir().join(format!(
        "yode-permission-hook-metadata-test-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&hook_dir).unwrap();
    let dump_path = hook_dir.join("permission-context.json");
    let mut hook_mgr = crate::hooks::HookManager::new(hook_dir.clone());
    hook_mgr.register(crate::hooks::HookDefinition {
        command: format!(
            "printf '%s' \"$YODE_HOOK_CONTEXT\" > {}",
            dump_path.display()
        ),
        events: vec!["permission_request".into()],
        tool_filter: Some(vec!["mock_path".into()]),
        timeout_secs: 5,
        can_block: false,
    });
    engine.set_hook_manager(hook_mgr);

    let tool_call = ToolCall {
        id: "tc1".into(),
        name: "mock_path".into(),
        arguments: "{\"path\":\"old.txt\"}".into(),
    };
    let (event_tx, _event_rx) = mpsc::unbounded_channel();
    let (confirm_tx, mut confirm_rx) = mpsc::unbounded_channel();
    confirm_tx.send(ConfirmResponse::Allow).unwrap();

    let _ = engine
        .handle_tool_call(&tool_call, &event_tx, &mut confirm_rx, None)
        .await
        .unwrap();

    let value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dump_path).unwrap()).unwrap();
    let metadata = value.get("metadata").and_then(|v| v.as_object()).unwrap();
    assert_eq!(
        metadata
            .get("effective_input_snapshot")
            .and_then(|v| v.get("path"))
            .and_then(|v| v.as_str()),
        Some("old.txt")
    );
    assert_eq!(
        metadata
            .get("original_input_snapshot")
            .and_then(|v| v.get("path"))
            .and_then(|v| v.as_str()),
        Some("old.txt")
    );
    assert_eq!(
        metadata
            .get("input_changed_by_hook")
            .and_then(|v| v.as_bool()),
        Some(false)
    );
}

#[tokio::test]
async fn test_session_end_hook_context_includes_runtime_metadata() {
    let mut engine = make_engine(vec![], vec![]);
    let hook_dir = std::env::temp_dir().join(format!(
        "yode-session-end-hook-test-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&hook_dir).unwrap();
    let dump_path = hook_dir.join("session-end-context.json");
    let mut hook_mgr = crate::hooks::HookManager::new(hook_dir.clone());
    hook_mgr.register(crate::hooks::HookDefinition {
        command: format!(
            "printf '%s' \"$YODE_HOOK_CONTEXT\" > {}",
            dump_path.display()
        ),
        events: vec!["session_end".into()],
        tool_filter: None,
        timeout_secs: 5,
        can_block: false,
    });
    engine.set_hook_manager(hook_mgr);
    engine.messages = vec![
        Message::system("system"),
        Message::user("hello"),
        Message::assistant("world"),
    ];

    engine.finalize_session_hooks("shutdown").await;

    let value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dump_path).unwrap()).unwrap();
    let metadata = value.get("metadata").unwrap();
    let runtime = metadata.get("runtime").and_then(|v| v.as_object()).unwrap();
    assert_eq!(
        metadata.get("reason").and_then(|v| v.as_str()),
        Some("shutdown")
    );
    assert!(runtime.contains_key("live_session_memory_path"));
    assert!(runtime.contains_key("tracked_failed_tool_results"));
    let memory_flush = metadata
        .get("memory_flush")
        .and_then(|v| v.as_object())
        .unwrap();
    assert!(memory_flush.contains_key("path"));
    assert!(memory_flush.contains_key("update_count"));
}

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
