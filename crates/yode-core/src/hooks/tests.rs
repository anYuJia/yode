use super::*;

mod manager;
mod parsing;

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

    let discovery = discover_plugin_hooks(dir.path());

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

    let discovery = discover_plugin_hooks(dir.path());

    assert!(discovery.hooks.is_empty());
    assert!(discovery.diagnostics.is_empty());
}
