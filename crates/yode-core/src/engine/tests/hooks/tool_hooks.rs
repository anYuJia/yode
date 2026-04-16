use super::*;

use yode_llm::types::ToolCall;

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
async fn test_pre_tool_use_hook_can_defer_call_with_artifact() {
    let mut engine = make_engine(vec![Arc::new(MockPathTool)], vec![]);
    let hook_dir =
        std::env::temp_dir().join(format!("yode-defer-hook-test-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&hook_dir).unwrap();
    let mut hook_mgr = crate::hooks::HookManager::new(hook_dir);
    hook_mgr.register(crate::hooks::HookDefinition {
        command: "printf '%s' '{\"decision\":\"defer\",\"deferReason\":\"await external approval\"}'".into(),
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

    assert!(!result.result.is_error);
    assert!(result.result.content.contains("Deferred by hook"));
    let metadata = result.result.metadata.as_ref().unwrap();
    assert_eq!(metadata["deferred"].as_bool(), Some(true));
    let summary_path = metadata["defer_summary_artifact"].as_str().unwrap();
    let state_path = metadata["defer_state_artifact"].as_str().unwrap();
    assert!(std::path::Path::new(summary_path).exists());
    assert!(std::path::Path::new(state_path).exists());
    let state_body = std::fs::read_to_string(state_path).unwrap();
    assert!(state_body.contains("await external approval"));
}

#[tokio::test]
async fn test_pre_tool_use_hook_can_defer_tool_call() {
    let mut engine = make_engine(vec![Arc::new(MockPathTool)], vec![]);
    let hook_dir =
        std::env::temp_dir().join(format!("yode-defer-hook-test-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&hook_dir).unwrap();
    let mut hook_mgr = crate::hooks::HookManager::new(hook_dir);
    hook_mgr.register(crate::hooks::HookDefinition {
        command: "printf '%s' '{\"decision\":\"defer\",\"deferReason\":\"wait for remote approval\"}'".into(),
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

    assert!(!result.result.is_error);
    assert!(result.result.content.contains("Deferred by hook"));
    let metadata = result.result.metadata.as_ref().unwrap();
    assert_eq!(metadata["deferred"].as_bool(), Some(true));
    let summary = metadata["defer_summary_artifact"].as_str().unwrap();
    let state = metadata["defer_state_artifact"].as_str().unwrap();
    assert!(std::path::Path::new(summary).exists());
    assert!(std::path::Path::new(state).exists());
}

#[tokio::test]
async fn test_finalize_tool_result_emits_worktree_create_hook() {
    let mut engine = make_engine(vec![], vec![]);
    let hook_dir =
        std::env::temp_dir().join(format!("yode-worktree-event-test-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&hook_dir).unwrap();
    let dump_path = hook_dir.join("worktree-create-context.json");
    let mut hook_mgr = crate::hooks::HookManager::new(hook_dir.clone());
    hook_mgr.register(crate::hooks::HookDefinition {
        command: format!(
            "printf '%s' \"$YODE_HOOK_CONTEXT\" > {}",
            dump_path.display()
        ),
        events: vec!["worktree_create".into()],
        tool_filter: Some(vec!["enter_worktree".into()]),
        timeout_secs: 5,
        can_block: false,
    });
    engine.set_hook_manager(hook_mgr);

    let tool_call = ToolCall {
        id: "tc1".into(),
        name: "enter_worktree".into(),
        arguments: "{\"name\":\"demo\"}".into(),
    };
    let result = yode_tools::tool::ToolResult::success_with_metadata(
        "Created worktree".to_string(),
        serde_json::json!({
            "worktree_dir": "/tmp/demo",
            "branch_name": "yode-demo",
            "action": "create",
            "original_dir": "/tmp/original",
        }),
    );

    let _ = engine
        .finalize_tool_result(&tool_call, result, None, 0, 0, None)
        .await;

    let body = std::fs::read_to_string(&dump_path).unwrap();
    assert!(body.contains("\"event\":\"worktree_create\""));
    assert!(body.contains("\"tool_name\":\"enter_worktree\""));
    assert!(body.contains("\"worktree_dir\":\"/tmp/demo\""));
}
