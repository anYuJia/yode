use super::*;

pub(in crate::hooks) fn parse_structured_hook_output(stdout: &str) -> Option<HookResult> {
    let trimmed = stdout.trim();
    if !trimmed.starts_with('{') {
        return None;
    }

    let value: Value = serde_json::from_str(trimmed).ok()?;
    let object = value.as_object()?;

    let continue_flag = object.get("continue").and_then(|v| v.as_bool());
    let decision = object
        .get("decision")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let blocked = continue_flag.map(|v| !v).unwrap_or(false)
        || decision
            .as_deref()
            .is_some_and(|value| value.eq_ignore_ascii_case("block"));
    let deferred = object
        .get("defer")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
        || decision
            .as_deref()
            .is_some_and(|value| value.eq_ignore_ascii_case("defer"));

    let reason = object
        .get("reason")
        .or_else(|| object.get("stopReason"))
        .or_else(|| object.get("deferReason"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let modified_input = object
        .get("modified_input")
        .cloned()
        .or_else(|| object.get("updatedInput").cloned())
        .or_else(|| {
            object
                .get("hookSpecificOutput")
                .and_then(|v| v.get("updatedInput"))
                .cloned()
        });

    let stdout = collect_hook_text_outputs(object);
    let wake_notification = object
        .get("wakeNotification")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .or_else(|| {
            object
                .get("hookSpecificOutput")
                .and_then(|v| v.get("wakeNotification"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        });
    let memory_sections = object
        .get("hookSpecificOutput")
        .and_then(|v| v.get("memorySections"))
        .and_then(render_memory_sections_markdown);
    let stdout = merge_hook_output_parts(stdout, memory_sections);

    Some(HookResult {
        blocked,
        deferred,
        reason,
        modified_input,
        stdout,
        wake_notification,
        source_hook_command: None,
    })
}

fn collect_hook_text_outputs(object: &serde_json::Map<String, Value>) -> Option<String> {
    let mut parts = Vec::new();
    push_unique_output(
        &mut parts,
        object
            .get("systemMessage")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
    );
    push_unique_output(
        &mut parts,
        object
            .get("additional_context")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
    );
    push_unique_output(
        &mut parts,
        object
            .get("hookSpecificOutput")
            .and_then(|v| v.get("additionalContext"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
    );

    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\n\n"))
    }
}

fn merge_hook_output_parts(primary: Option<String>, secondary: Option<String>) -> Option<String> {
    let mut parts = Vec::new();
    push_unique_output(&mut parts, primary);
    push_unique_output(&mut parts, secondary);
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\n\n"))
    }
}

fn push_unique_output(target: &mut Vec<String>, value: Option<String>) {
    let Some(value) = value else {
        return;
    };
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return;
    }
    if !target.iter().any(|existing| existing == trimmed) {
        target.push(trimmed.to_string());
    }
}

fn render_memory_sections_markdown(value: &Value) -> Option<String> {
    let object = value.as_object()?;
    let mut lines = Vec::new();
    let sections = [
        ("goals", "Goals"),
        ("findings", "Findings"),
        ("decisions", "Decisions"),
        ("files", "Files"),
        ("open_questions", "Open Questions"),
        ("freshness", "Freshness"),
        ("confidence", "Confidence"),
    ];

    for (key, title) in sections {
        let Some(items) = object.get(key).and_then(|v| v.as_array()) else {
            continue;
        };
        lines.push(format!("### {}", title));
        lines.push(String::new());
        if items.is_empty() {
            lines.push("- None".to_string());
        } else {
            for item in items {
                if let Some(text) = item.as_str() {
                    lines.push(format!("- {}", text));
                }
            }
        }
        lines.push(String::new());
    }

    if lines.is_empty() {
        None
    } else {
        Some(lines.join("\n").trim().to_string())
    }
}
