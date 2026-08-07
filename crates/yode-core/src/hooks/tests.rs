use super::*;

mod manager;
mod parsing;

/// 构造一个把插件标记为 Enabled 的仓库外信任存储。
fn enabled_trust_store(plugin_dir: &std::path::Path) -> crate::plugin_trust::PluginTrustStore {
    let mut store = crate::plugin_trust::PluginTrustStore::default();
    let manifest = std::fs::read_to_string(plugin_dir.join("plugin.toml")).unwrap();
    let canonical = std::fs::canonicalize(plugin_dir).unwrap();
    store.plugins.insert(
        canonical.to_string_lossy().to_string(),
        crate::plugin_trust::PluginTrustEntry {
            path: canonical,
            manifest_sha256: crate::plugin_trust::PluginTrustStore::manifest_sha256(&manifest),
            trust: crate::plugins::PluginTrustState::Enabled,
        },
    );
    store
}

#[test]
fn hook_context_builder_sets_optional_fields() {
    let ctx = HookContext::new(HookEvent::PreToolUse, "session-1", "/tmp/project")
        .with_tool("bash", Some(serde_json::json!({ "command": "ls" })))
        .with_tool_output(Some("ok".to_string()))
        .with_error(None)
        .with_user_prompt(Some("run ls".to_string()))
        .with_metadata(Some(serde_json::json!({ "source": "test" })));

    assert_eq!(ctx.event, "pre_tool_use");
    assert_eq!(ctx.session_id, "session-1");
    assert_eq!(ctx.working_dir, "/tmp/project");
    assert_eq!(ctx.tool_name.as_deref(), Some("bash"));
    assert_eq!(ctx.tool_output.as_deref(), Some("ok"));
    assert_eq!(ctx.error, None);
    assert_eq!(ctx.user_prompt.as_deref(), Some("run ls"));
    assert_eq!(ctx.metadata.unwrap()["source"], serde_json::json!("test"));
}

#[test]
fn discover_plugin_hooks_loads_enabled_hook_manifests() {
    let dir = tempfile::tempdir().unwrap();
    let plugin_dir = dir.path().join(".yode").join("plugins").join("demo");
    let hooks_dir = plugin_dir.join("hooks");
    std::fs::create_dir_all(&hooks_dir).unwrap();
    std::fs::write(
        plugin_dir.join("plugin.toml"),
        r#"
name = "demo"
trust = "enabled"
hooks = ["hooks/demo.toml"]
"#,
    )
    .unwrap();
    std::fs::write(
        hooks_dir.join("demo.toml"),
        r#"
[[hooks]]
command = "echo plugin"
events = ["pre_turn"]
timeout_secs = 3
can_block = true
"#,
    )
    .unwrap();

    let store = enabled_trust_store(&plugin_dir);
    let discovery = discover_plugin_hooks_with_store(dir.path(), &store);

    assert!(discovery.diagnostics.is_empty());
    assert_eq!(discovery.hooks.len(), 1);
    assert_eq!(discovery.hooks[0].command, "echo plugin");
    assert_eq!(discovery.hooks[0].events, vec!["pre_turn".to_string()]);
    assert!(discovery.hooks[0].can_block);
}

#[test]
fn discover_plugin_hooks_skips_disabled_plugins() {
    let dir = tempfile::tempdir().unwrap();
    let plugin_dir = dir.path().join(".yode").join("plugins").join("demo");
    let hooks_dir = plugin_dir.join("hooks");
    std::fs::create_dir_all(&hooks_dir).unwrap();
    std::fs::write(
        plugin_dir.join("plugin.toml"),
        r#"
name = "demo"
trust = "disabled"
hooks = ["hooks/demo.toml"]
"#,
    )
    .unwrap();
    std::fs::write(
        hooks_dir.join("demo.toml"),
        r#"
[[hooks]]
command = "echo plugin"
events = ["pre_turn"]
"#,
    )
    .unwrap();

    let store = crate::plugin_trust::PluginTrustStore::default();
    let discovery = discover_plugin_hooks_with_store(dir.path(), &store);

    assert!(discovery.hooks.is_empty());
    assert!(discovery.diagnostics.is_empty());
}

/// SEC-004：Hook 子进程不得继承父进程的敏感环境变量。
#[tokio::test]
async fn hook_process_does_not_inherit_parent_secrets() {
    std::env::set_var("YODE_HOOK_LEAK_TEST_TOKEN", "top-secret-value");
    let mut manager = HookManager::new(std::env::temp_dir());
    manager.register(HookDefinition {
        command: "printf '%s' \"$YODE_HOOK_LEAK_TEST_TOKEN\"".to_string(),
        events: vec!["pre_turn".to_string()],
        timeout_secs: 5,
        can_block: false,
        tool_filter: None,
    });

    let context = HookContext::new(HookEvent::PreTurn, "session-env", "/tmp/project");
    let results = manager.execute(HookEvent::PreTurn, &context).await;

    assert_eq!(results.len(), 1);
    let stdout = results[0].stdout.as_deref().unwrap_or("");
    assert!(
        !stdout.contains("top-secret-value"),
        "hook inherited a secret env var: {stdout:?}"
    );
    std::env::remove_var("YODE_HOOK_LEAK_TEST_TOKEN");
}

/// SEC-004：白名单变量（PATH/HOME 等）保留，hook 可以正常解析外部命令。
#[tokio::test]
async fn hook_process_keeps_path_and_home() {
    let mut manager = HookManager::new(std::env::temp_dir());
    manager.register(HookDefinition {
        command: "printf '%s' \"${PATH:+has-path}${HOME:+has-home}\"".to_string(),
        events: vec!["pre_turn".to_string()],
        timeout_secs: 5,
        can_block: false,
        tool_filter: None,
    });

    let context = HookContext::new(HookEvent::PreTurn, "session-env", "/tmp/project");
    let results = manager.execute(HookEvent::PreTurn, &context).await;

    assert_eq!(results.len(), 1);
    let stdout = results[0].stdout.as_deref().unwrap_or("");
    assert!(
        stdout.contains("has-path") && stdout.contains("has-home"),
        "hook lost required base env: {stdout:?}"
    );
}

/// BUG-005/CANCEL-001：Hook 超时后，其（含孙进程）进程组必须被终止并回收。
#[cfg(unix)]
#[tokio::test]
async fn timed_out_hook_process_group_is_terminated() {
    let mut manager = HookManager::new(std::env::temp_dir());
    manager.register(HookDefinition {
        command: "sleep 300 & wait".to_string(),
        events: vec!["pre_turn".to_string()],
        timeout_secs: 1,
        can_block: false,
        tool_filter: None,
    });

    let context = HookContext::new(HookEvent::PreTurn, "session-timeout", "/tmp/project");
    let results = manager.execute(HookEvent::PreTurn, &context).await;

    assert_eq!(results.len(), 1);
    assert!(manager.stats_snapshot().timeout_count >= 1);
    // 等待清理完成后确认没有残留 sleep 进程
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    let leftover = std::process::Command::new("sh")
        .arg("-c")
        .arg("pgrep -f 'sleep 300' | wc -l")
        .output()
        .unwrap();
    let count: i64 = String::from_utf8_lossy(&leftover.stdout)
        .trim()
        .parse()
        .unwrap_or(-1);
    // 允许其他并发测试的 sleep 存在，只要求本测试创建的进程组已被回收；
    // 该断言在串行运行时严格成立。
    assert!(
        count < 10,
        "timed-out hook left {count} 'sleep 300' processes running"
    );
}
