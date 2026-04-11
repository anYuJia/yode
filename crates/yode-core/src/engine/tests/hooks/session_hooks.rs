use super::*;

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
    let hook_dir = std::env::temp_dir().join(format!(
        "yode-session-hook-wake-test-{}",
        uuid::Uuid::new_v4()
    ));
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
    let hook_dir = std::env::temp_dir().join(format!(
        "yode-user-prompt-hook-test-{}",
        uuid::Uuid::new_v4()
    ));
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
