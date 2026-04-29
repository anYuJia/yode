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
