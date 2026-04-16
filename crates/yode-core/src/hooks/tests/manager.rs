use super::*;

#[test]
fn test_hook_event_display() {
    assert_eq!(HookEvent::PreToolUse.to_string(), "pre_tool_use");
    assert_eq!(HookEvent::SessionStart.to_string(), "session_start");
    assert_eq!(HookEvent::PreCompact.to_string(), "pre_compact");
    assert_eq!(HookEvent::PostCompact.to_string(), "post_compact");
    assert_eq!(HookEvent::SubagentStart.to_string(), "subagent_start");
    assert_eq!(HookEvent::TaskCreated.to_string(), "task_created");
    assert_eq!(HookEvent::WorktreeCreate.to_string(), "worktree_create");
}

#[test]
fn test_hook_result_default() {
    let r = HookResult::allowed();
    assert!(!r.blocked);
    assert!(!r.deferred);
    assert!(r.reason.is_none());
}

#[test]
fn test_hook_result_blocked() {
    let r = HookResult::blocked("denied".into());
    assert!(r.blocked);
    assert_eq!(r.reason.as_deref(), Some("denied"));
}

#[test]
fn test_hook_result_deferred() {
    let r = HookResult::deferred("wait for external approval".into());
    assert!(r.deferred);
    assert_eq!(r.reason.as_deref(), Some("wait for external approval"));
}

#[tokio::test]
async fn test_hook_manager_no_hooks() {
    let mgr = HookManager::new(PathBuf::from("/tmp"));
    let ctx = HookContext {
        event: "pre_tool_use".into(),
        session_id: "test".into(),
        working_dir: "/tmp".into(),
        tool_name: Some("bash".into()),
        tool_input: None,
        tool_output: None,
        error: None,
        user_prompt: None,
        metadata: None,
    };
    let results = mgr.execute(HookEvent::PreToolUse, &ctx).await;
    assert!(results.is_empty());
}

#[tokio::test]
async fn test_hook_manager_with_echo_hook() {
    let mut mgr = HookManager::new(PathBuf::from("/tmp"));
    mgr.register(HookDefinition {
        command: "echo hello".into(),
        events: vec!["pre_tool_use".into()],
        tool_filter: None,
        timeout_secs: 5,
        can_block: false,
    });
    let ctx = HookContext {
        event: "pre_tool_use".into(),
        session_id: "test".into(),
        working_dir: "/tmp".into(),
        tool_name: Some("bash".into()),
        tool_input: None,
        tool_output: None,
        error: None,
        user_prompt: None,
        metadata: None,
    };
    let results = mgr.execute(HookEvent::PreToolUse, &ctx).await;
    assert_eq!(results.len(), 1);
    assert!(!results[0].blocked);
    assert_eq!(results[0].stdout.as_deref(), Some("hello\n"));
}

#[tokio::test]
async fn test_hook_tool_filter() {
    let mut mgr = HookManager::new(PathBuf::from("/tmp"));
    mgr.register(HookDefinition {
        command: "echo filtered".into(),
        events: vec!["pre_tool_use".into()],
        tool_filter: Some(vec!["write_file".into()]),
        timeout_secs: 5,
        can_block: false,
    });
    let ctx = HookContext {
        event: "pre_tool_use".into(),
        session_id: "test".into(),
        working_dir: "/tmp".into(),
        tool_name: Some("bash".into()),
        tool_input: None,
        tool_output: None,
        error: None,
        user_prompt: None,
        metadata: None,
    };
    let results = mgr.execute(HookEvent::PreToolUse, &ctx).await;
    assert!(results.is_empty());
}

#[tokio::test]
async fn test_hook_manager_parses_structured_json_output() {
    let mut mgr = HookManager::new(PathBuf::from("/tmp"));
    mgr.register(HookDefinition {
        command: "printf '%s' '{\"continue\":false,\"reason\":\"blocked\",\"modified_input\":{\"path\":\"src/main.rs\"},\"systemMessage\":\"hook context\"}'".into(),
        events: vec!["pre_tool_use".into()],
        tool_filter: None,
        timeout_secs: 5,
        can_block: true,
    });
    let ctx = HookContext {
        event: "pre_tool_use".into(),
        session_id: "test".into(),
        working_dir: "/tmp".into(),
        tool_name: Some("bash".into()),
        tool_input: None,
        tool_output: None,
        error: None,
        user_prompt: None,
        metadata: None,
    };
    let results = mgr.execute(HookEvent::PreToolUse, &ctx).await;
    assert_eq!(results.len(), 1);
    assert!(results[0].blocked);
    assert_eq!(results[0].reason.as_deref(), Some("blocked"));
    assert_eq!(results[0].stdout.as_deref(), Some("hook context"));
    assert_eq!(
        results[0]
            .modified_input
            .as_ref()
            .and_then(|v| v.get("path"))
            .and_then(|v| v.as_str()),
        Some("src/main.rs")
    );
}

#[tokio::test]
async fn test_hook_manager_parses_defer_output() {
    let mut mgr = HookManager::new(PathBuf::from("/tmp"));
    mgr.register(HookDefinition {
        command: "printf '%s' '{\"decision\":\"defer\",\"deferReason\":\"wait for browser auth\",\"systemMessage\":\"deferred\"}'".into(),
        events: vec!["pre_tool_use".into()],
        tool_filter: None,
        timeout_secs: 5,
        can_block: false,
    });
    let ctx = HookContext {
        event: "pre_tool_use".into(),
        session_id: "test".into(),
        working_dir: "/tmp".into(),
        tool_name: Some("bash".into()),
        tool_input: None,
        tool_output: None,
        error: None,
        user_prompt: None,
        metadata: None,
    };
    let results = mgr.execute(HookEvent::PreToolUse, &ctx).await;
    assert_eq!(results.len(), 1);
    assert!(results[0].deferred);
    assert_eq!(results[0].reason.as_deref(), Some("wait for browser auth"));
    assert_eq!(results[0].stdout.as_deref(), Some("deferred"));
    assert_eq!(results[0].source_hook_command.as_deref(), Some("printf '%s' '{\"decision\":\"defer\",\"deferReason\":\"wait for browser auth\",\"systemMessage\":\"deferred\"}'"));
    let stats = mgr.stats_snapshot();
    assert_eq!(stats.defer_count, 1);
    assert_eq!(stats.last_defer_reason.as_deref(), Some("wait for browser auth"));
}

#[tokio::test]
async fn test_hook_manager_queues_wake_notifications() {
    let mut mgr = HookManager::new(PathBuf::from("/tmp"));
    mgr.register(HookDefinition {
        command:
            "printf '%s' '{\"hookSpecificOutput\":{\"wakeNotification\":\"wake up\"}}' && exit 2"
                .into(),
        events: vec!["pre_tool_use".into()],
        tool_filter: None,
        timeout_secs: 5,
        can_block: false,
    });
    let ctx = HookContext {
        event: "pre_tool_use".into(),
        session_id: "test".into(),
        working_dir: "/tmp".into(),
        tool_name: Some("bash".into()),
        tool_input: None,
        tool_output: None,
        error: None,
        user_prompt: None,
        metadata: None,
    };
    let _ = mgr.execute(HookEvent::PreToolUse, &ctx).await;
    let wake = mgr.drain_wake_notifications();
    assert_eq!(wake.len(), 1);
    assert_eq!(wake[0].message, "wake up");
    assert_eq!(wake[0].event, "pre_tool_use");
    let stats = mgr.stats_snapshot();
    assert_eq!(stats.total_executions, 1);
    assert_eq!(stats.wake_notification_count, 1);
}

#[tokio::test]
async fn test_hook_manager_records_timeout_stats() {
    let mut mgr = HookManager::new(PathBuf::from("/tmp"));
    mgr.register(HookDefinition {
        command: "sleep 2".into(),
        events: vec!["pre_tool_use".into()],
        tool_filter: None,
        timeout_secs: 1,
        can_block: false,
    });
    let ctx = HookContext {
        event: "pre_tool_use".into(),
        session_id: "test".into(),
        working_dir: "/tmp".into(),
        tool_name: Some("bash".into()),
        tool_input: None,
        tool_output: None,
        error: None,
        user_prompt: None,
        metadata: None,
    };
    let _ = mgr.execute(HookEvent::PreToolUse, &ctx).await;
    let stats = mgr.stats_snapshot();
    assert_eq!(stats.total_executions, 1);
    assert_eq!(stats.timeout_count, 1);
    assert_eq!(stats.last_timeout_command.as_deref(), Some("sleep 2"));
}
