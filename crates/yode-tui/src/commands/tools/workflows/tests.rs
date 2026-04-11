use super::definitions::{compact_json_preview, workflow_requires_write_mode, workflow_template};

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
