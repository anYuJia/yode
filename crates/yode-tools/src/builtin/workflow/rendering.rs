use std::path::Path;

use serde_json::Value;

use super::WorkflowExecutionMode;

pub(super) fn render_workflow_dry_run(
    workflow_path: &Path,
    workflow_name: Option<&str>,
    description: Option<&str>,
    mode: WorkflowExecutionMode,
    variables: &serde_json::Map<String, Value>,
    plan: &[Value],
    write_steps: &[Value],
) -> String {
    let display_name = workflow_name
        .map(str::to_string)
        .unwrap_or_else(|| workflow_path.to_string_lossy().into_owned());
    let mut lines = vec![
        format!("Workflow plan: {}", display_name),
        format!("Path: {}", workflow_path.display()),
        format!("Mode: {}", workflow_mode_label(mode)),
        format!("Description: {}", description.unwrap_or("none")),
    ];

    if variables.is_empty() {
        lines.push("Variables: none".to_string());
    } else {
        let mut rendered = variables
            .iter()
            .map(|(key, value)| format!("{}={}", key, compact_json(value)))
            .collect::<Vec<_>>();
        rendered.sort();
        lines.push(format!("Variables: {}", rendered.join(", ")));
    }

    lines.push(String::new());
    lines.push("Steps:".to_string());
    for step in plan {
        let index = step.get("index").and_then(|value| value.as_u64()).unwrap_or(0);
        let tool = step.get("tool").and_then(|value| value.as_str()).unwrap_or("unknown");
        let continue_on_error = step
            .get("continue_on_error")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        let write_capable = step
            .get("write_capable")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        let params = step.get("params").cloned().unwrap_or_else(|| Value::Object(Default::default()));

        lines.push(format!(
            "  {}. {} [{}]",
            index,
            tool,
            if write_capable { "write" } else { "read" }
        ));
        lines.push(format!("     continue_on_error: {}", continue_on_error));
        lines.push(format!("     params: {}", compact_json(&params)));
    }

    lines.push(String::new());
    if write_steps.is_empty() {
        lines.push("Write checkpoints: none".to_string());
    } else {
        lines.push("Write checkpoints:".to_string());
        for checkpoint in write_steps {
            let index = checkpoint
                .get("index")
                .and_then(|value| value.as_u64())
                .unwrap_or(0);
            let tool = checkpoint
                .get("tool")
                .and_then(|value| value.as_str())
                .unwrap_or("unknown");
            let requires = checkpoint
                .get("requires")
                .and_then(|value| value.as_str())
                .unwrap_or("workflow_run_with_writes confirmation");
            lines.push(format!("  - step {} {} ({})", index, tool, requires));
        }
    }

    lines.join("\n")
}

pub(super) fn render_approval_checkpoint(index: usize, tool_name: &str) -> Value {
    let requires = "workflow_run_with_writes confirmation";
    let message = format!(
        "Step {} ({}) runs under {}.",
        index, tool_name, requires
    );
    serde_json::json!({
        "index": index,
        "tool": tool_name,
        "approval_checkpoint": true,
        "checkpoint_type": "write_capable_tool",
        "requires": requires,
        "message": message,
    })
}

pub(super) fn workflow_mode_label(mode: WorkflowExecutionMode) -> &'static str {
    match mode {
        WorkflowExecutionMode::SafeReadOnly => "safe_read_only",
        WorkflowExecutionMode::ConfirmedWrites => "confirmed_writes",
    }
}

fn compact_json(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "{}".to_string())
}
