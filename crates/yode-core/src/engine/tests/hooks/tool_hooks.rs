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
