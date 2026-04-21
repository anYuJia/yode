use std::sync::Arc;

use crate::registry::ToolRegistry;
use crate::tool::{Tool, ToolContext};

use super::variables::apply_variables;
use super::{WorkflowRunTool, WorkflowRunWithWritesTool};

#[tokio::test]
async fn workflow_runs_read_only_steps() {
    let dir = tempfile::tempdir().unwrap();
    let workflow_dir = dir.path().join(".yode").join("workflows");
    tokio::fs::create_dir_all(&workflow_dir).await.unwrap();
    tokio::fs::write(
        workflow_dir.join("inspect.json"),
        r#"{
            "name": "inspect",
            "steps": [
                { "tool_name": "ls", "params": { "path": "." } }
            ]
        }"#,
    )
    .await
    .unwrap();

    let mut registry = ToolRegistry::new();
    crate::builtin::register_builtin_tools(&mut registry);

    let mut ctx = ToolContext::empty();
    ctx.registry = Some(Arc::new(registry));
    ctx.working_dir = Some(dir.path().to_path_buf());

    let tool = WorkflowRunTool;
    let result = tool
        .execute(serde_json::json!({ "name": "inspect" }), &ctx)
        .await
        .unwrap();
    assert!(!result.is_error);
    assert!(result.content.contains("\"content\""));
    assert!(result.content.contains("\"tool\": \"ls\""));
}

#[tokio::test]
async fn workflow_dry_run_returns_plan_without_execution() {
    let dir = tempfile::tempdir().unwrap();
    let workflow_dir = dir.path().join(".yode").join("workflows");
    tokio::fs::create_dir_all(&workflow_dir).await.unwrap();
    tokio::fs::write(
        workflow_dir.join("plan.json"),
        r#"{
            "name": "plan",
            "steps": [
                { "tool_name": "review_changes", "params": { "focus": "${focus}" } }
            ]
        }"#,
    )
    .await
    .unwrap();

    let mut registry = ToolRegistry::new();
    crate::builtin::register_builtin_tools(&mut registry);

    let mut ctx = ToolContext::empty();
    ctx.registry = Some(Arc::new(registry));
    ctx.working_dir = Some(dir.path().to_path_buf());

    let tool = WorkflowRunTool;
    let result = tool
        .execute(
            serde_json::json!({
                "name": "plan",
                "dry_run": true,
                "variables": { "focus": "regressions" }
            }),
            &ctx,
    )
        .await
        .unwrap();
    assert!(!result.is_error);
    assert!(result.content.contains("Workflow plan: plan"));
    assert!(result.content.contains("1. review_changes [read]"));
    assert!(result.content.contains("regressions"));
    assert!(result.metadata.unwrap()["write_steps"].is_array());
}

#[test]
fn workflow_applies_variable_substitution() {
    let params = serde_json::json!({
        "command": "echo ${name}",
        "nested": ["${kind}"]
    });
    let variables = serde_json::json!({
        "name": "world",
        "kind": "read-only"
    })
    .as_object()
    .unwrap()
    .clone();
    let applied = apply_variables(params, &variables);
    assert_eq!(applied["command"].as_str(), Some("echo world"));
    assert_eq!(applied["nested"][0].as_str(), Some("read-only"));
}

#[tokio::test]
async fn safe_workflow_blocks_mutating_tools() {
    let dir = tempfile::tempdir().unwrap();
    let output_path = dir.path().join("created.txt");
    let workflow_dir = dir.path().join(".yode").join("workflows");
    tokio::fs::create_dir_all(&workflow_dir).await.unwrap();
    tokio::fs::write(
        workflow_dir.join("write.json"),
        format!(
            r#"{{
                "name": "write",
                "steps": [
                    {{
                        "tool_name": "write_file",
                        "params": {{
                            "file_path": "{}",
                            "content": "hello"
                        }}
                    }}
                ]
            }}"#,
            output_path.display()
        ),
    )
    .await
    .unwrap();

    let mut registry = ToolRegistry::new();
    crate::builtin::register_builtin_tools(&mut registry);

    let mut ctx = ToolContext::empty();
    ctx.registry = Some(Arc::new(registry));
    ctx.working_dir = Some(dir.path().to_path_buf());

    let tool = WorkflowRunTool;
    let result = tool
        .execute(serde_json::json!({ "name": "write" }), &ctx)
        .await
        .unwrap();

    assert!(result.is_error);
    assert!(result.content.contains("blocked in safe workflow mode"));
    assert!(!output_path.exists());
}

#[tokio::test]
async fn write_enabled_workflow_can_run_mutating_steps() {
    let dir = tempfile::tempdir().unwrap();
    let output_path = dir.path().join("created.txt");
    let workflow_dir = dir.path().join(".yode").join("workflows");
    tokio::fs::create_dir_all(&workflow_dir).await.unwrap();
    tokio::fs::write(
        workflow_dir.join("write.json"),
        format!(
            r#"{{
                "name": "write",
                "steps": [
                    {{
                        "tool_name": "write_file",
                        "params": {{
                            "file_path": "{}",
                            "content": "hello"
                        }}
                    }}
                ]
            }}"#,
            output_path.display()
        ),
    )
    .await
    .unwrap();

    let mut registry = ToolRegistry::new();
    crate::builtin::register_builtin_tools(&mut registry);

    let mut ctx = ToolContext::empty();
    ctx.registry = Some(Arc::new(registry));
    ctx.working_dir = Some(dir.path().to_path_buf());

    let tool = WorkflowRunWithWritesTool;
    let result = tool
        .execute(serde_json::json!({ "name": "write" }), &ctx)
        .await
        .unwrap();

    assert!(!result.is_error);
    assert_eq!(
        tokio::fs::read_to_string(&output_path).await.unwrap(),
        "hello"
    );
    let metadata = result.metadata.unwrap();
    assert_eq!(metadata["mode"], "confirmed_writes");
    assert!(metadata["approval_checkpoints"].is_array());
    assert_eq!(
        metadata["approval_checkpoints"][0]["checkpoint_type"],
        "write_capable_tool"
    );
}

#[tokio::test]
async fn workflow_blocks_recursive_execution() {
    let dir = tempfile::tempdir().unwrap();
    let workflow_dir = dir.path().join(".yode").join("workflows");
    tokio::fs::create_dir_all(&workflow_dir).await.unwrap();
    tokio::fs::write(
        workflow_dir.join("nested.json"),
        r#"{
            "name": "nested",
            "steps": [
                { "tool_name": "workflow_run", "params": { "name": "other" } }
            ]
        }"#,
    )
    .await
    .unwrap();

    let mut registry = ToolRegistry::new();
    crate::builtin::register_builtin_tools(&mut registry);

    let mut ctx = ToolContext::empty();
    ctx.registry = Some(Arc::new(registry));
    ctx.working_dir = Some(dir.path().to_path_buf());

    let tool = WorkflowRunWithWritesTool;
    let result = tool
        .execute(serde_json::json!({ "name": "nested" }), &ctx)
        .await
        .unwrap();

    assert!(result.is_error);
    assert!(result
        .content
        .contains("blocked to avoid nested workflow execution"));
    assert!(result.content.contains("/workflows preview"));
}

#[tokio::test]
async fn workflow_resolves_explicit_path() {
    let dir = tempfile::tempdir().unwrap();
    let workflow_path = dir.path().join("custom-workflow.json");
    tokio::fs::write(
        &workflow_path,
        r#"{
            "name": "custom",
            "steps": [
                { "tool_name": "ls", "params": { "path": "." } }
            ]
        }"#,
    )
    .await
    .unwrap();

    let registry = ToolRegistry::new();
    crate::builtin::register_builtin_tools(&registry);

    let mut ctx = ToolContext::empty();
    ctx.registry = Some(Arc::new(registry));
    ctx.working_dir = Some(dir.path().to_path_buf());

    let tool = WorkflowRunTool;
    let result = tool
        .execute(
            serde_json::json!({ "workflow_path": workflow_path.display().to_string(), "dry_run": true }),
            &ctx,
        )
        .await
        .unwrap();
    assert!(!result.is_error);
    assert!(result.content.contains("Workflow plan: custom"));
    assert!(result.content.contains("1. ls [read]"));
}

#[tokio::test]
async fn workflow_errors_for_unknown_tools() {
    let dir = tempfile::tempdir().unwrap();
    let workflow_dir = dir.path().join(".yode").join("workflows");
    tokio::fs::create_dir_all(&workflow_dir).await.unwrap();
    tokio::fs::write(
        workflow_dir.join("unknown.json"),
        r#"{
            "name": "unknown",
            "steps": [
                { "tool_name": "definitely_missing_tool", "params": {} }
            ]
        }"#,
    )
    .await
    .unwrap();

    let registry = ToolRegistry::new();
    crate::builtin::register_builtin_tools(&registry);

    let mut ctx = ToolContext::empty();
    ctx.registry = Some(Arc::new(registry));
    ctx.working_dir = Some(dir.path().to_path_buf());

    let tool = WorkflowRunTool;
    let result = tool
        .execute(serde_json::json!({ "name": "unknown" }), &ctx)
        .await
        .unwrap();

    assert!(result.is_error);
    assert!(result.content.contains("references unknown tool"));
}

#[tokio::test]
async fn workflow_continue_on_error_runs_later_steps() {
    let dir = tempfile::tempdir().unwrap();
    let workflow_dir = dir.path().join(".yode").join("workflows");
    tokio::fs::create_dir_all(&workflow_dir).await.unwrap();
    tokio::fs::write(
        workflow_dir.join("continue.json"),
        r#"{
            "name": "continue",
            "steps": [
                { "tool_name": "ls", "params": { "path": "./missing" }, "continue_on_error": true },
                { "tool_name": "ls", "params": { "path": "." } }
            ]
        }"#,
    )
    .await
    .unwrap();

    let registry = ToolRegistry::new();
    crate::builtin::register_builtin_tools(&registry);

    let mut ctx = ToolContext::empty();
    ctx.registry = Some(Arc::new(registry));
    ctx.working_dir = Some(dir.path().to_path_buf());

    let tool = WorkflowRunTool;
    let result = tool
        .execute(serde_json::json!({ "name": "continue" }), &ctx)
        .await
        .unwrap();

    assert!(!result.is_error);
    assert!(result.content.contains("\"is_error\": true"));
    assert!(result.content.contains("\"tool\": \"ls\""));
    assert!(result.content.matches("\"tool\": \"ls\"").count() >= 2);
}
