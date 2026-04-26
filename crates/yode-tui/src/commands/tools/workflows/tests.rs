use super::definitions::{
    compact_json_preview, latest_workflow_name, load_workflow_definition, workflow_requires_write_mode,
    workflow_template,
};

#[test]
fn workflow_mode_detection_distinguishes_safe_and_write_steps() {
    let safe = serde_json::json!([
        { "tool_name": "review_changes" },
        { "tool_name": "verification_agent" }
    ]);
    let write = serde_json::json!([{ "tool_name": "review_pipeline" }]);

    assert!(!workflow_requires_write_mode(safe.as_array().unwrap()));
    assert!(workflow_requires_write_mode(write.as_array().unwrap()));
}

#[test]
fn workflow_templates_include_ship_flows() {
    assert!(workflow_template("review-then-commit").is_some());
    assert!(workflow_template("ship-pipeline").is_some());
}

#[test]
fn compact_json_preview_truncates_long_params() {
    let preview = compact_json_preview(&serde_json::json!({
        "focus": "x".repeat(200)
    }));
    assert!(preview.ends_with("..."));
}

#[test]
fn workflow_definitions_fall_back_to_claude_workflows_dir() {
    let dir = std::env::temp_dir().join(format!("yode-workflow-compat-{}", uuid::Uuid::new_v4()));
    let workflow_dir = dir.join(".claude").join("workflows");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&workflow_dir).unwrap();
    std::fs::write(
        workflow_dir.join("demo.json"),
        r#"{
  "name": "demo",
  "description": "compat workflow",
  "steps": [{ "tool_name": "ls", "params": { "path": "." } }]
}"#,
    )
    .unwrap();

    let yode_dir = dir.join(".yode").join("workflows");
    assert_eq!(latest_workflow_name(&yode_dir).as_deref(), Some("demo"));
    let (path, _json, steps) = load_workflow_definition(&yode_dir, "demo").unwrap();
    assert!(path.ends_with(".claude/workflows/demo.json"));
    assert_eq!(steps.len(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}
