use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ToolSummaryTone {
    Neutral,
    Success,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ToolSummaryLine {
    pub text: String,
    pub tone: ToolSummaryTone,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct ToolResultSummary {
    pub lines: Vec<ToolSummaryLine>,
    pub hide_body_by_default: bool,
}

pub(crate) fn summarize_tool_result(
    tool_name: &str,
    args: &Value,
    metadata: Option<&Value>,
    result_content: &str,
    is_error: bool,
) -> ToolResultSummary {
    if is_error {
        return ToolResultSummary::default();
    }

    match tool_name {
        "read_file" => summarize_read_file(args, metadata),
        "grep" => summarize_grep(args, metadata, result_content),
        "glob" => summarize_glob(args, metadata, result_content),
        "bash" => summarize_bash(metadata),
        _ => ToolResultSummary::default(),
    }
}

fn summarize_read_file(args: &Value, metadata: Option<&Value>) -> ToolResultSummary {
    let Some(metadata) = metadata else {
        return ToolResultSummary::default();
    };

    let total_lines = metadata
        .get("total_lines")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let start_line = metadata
        .get("start_line")
        .and_then(Value::as_u64)
        .unwrap_or(1);
    let end_line = metadata
        .get("end_line")
        .and_then(Value::as_u64)
        .unwrap_or(start_line);
    let was_truncated = metadata
        .get("was_truncated")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let file_path = metadata
        .get("file_path")
        .and_then(Value::as_str)
        .or_else(|| args.get("file_path").and_then(Value::as_str))
        .unwrap_or("");
    let line_count = end_line.saturating_sub(start_line).saturating_add(1);

    let primary = if total_lines == 0 {
        "read 0 lines".to_string()
    } else if was_truncated || start_line > 1 || end_line < total_lines {
        format!("read lines {}-{} of {}", start_line, end_line, total_lines)
    } else if line_count == 1 {
        "read 1 line".to_string()
    } else {
        format!("read {} lines", line_count)
    };

    let mut lines = vec![ToolSummaryLine {
        text: primary,
        tone: ToolSummaryTone::Success,
    }];

    if !file_path.is_empty() {
        lines.push(ToolSummaryLine {
            text: shorten_display_path(file_path),
            tone: ToolSummaryTone::Neutral,
        });
    }

    ToolResultSummary {
        lines,
        hide_body_by_default: true,
    }
}

fn summarize_grep(
    args: &Value,
    metadata: Option<&Value>,
    result_content: &str,
) -> ToolResultSummary {
    if result_content.trim() == "No matches found." {
        return ToolResultSummary {
            lines: vec![ToolSummaryLine {
                text: "no matches found".to_string(),
                tone: ToolSummaryTone::Neutral,
            }],
            hide_body_by_default: true,
        };
    }

    let Some(metadata) = metadata else {
        return ToolResultSummary::default();
    };

    let output_mode = metadata
        .get("output_mode")
        .and_then(Value::as_str)
        .unwrap_or("files_with_matches");
    let file_count = metadata
        .get("file_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let line_count = metadata
        .get("line_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let match_count = metadata
        .get("match_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let applied_limit = metadata.get("applied_limit").and_then(Value::as_u64);
    let applied_offset = metadata.get("applied_offset").and_then(Value::as_u64);
    let pattern = metadata
        .get("pattern")
        .and_then(Value::as_str)
        .or_else(|| args.get("pattern").and_then(Value::as_str))
        .unwrap_or("");

    let primary = match output_mode {
        "count" => {
            if file_count > 0 {
                format!("matched {} hits in {} files", match_count, file_count)
            } else {
                format!("matched {} hits", match_count)
            }
        }
        "content" => {
            if file_count > 1 {
                format!("matched {} lines in {} files", line_count, file_count)
            } else if line_count == 1 {
                "matched 1 line".to_string()
            } else {
                format!("matched {} lines", line_count)
            }
        }
        _ => {
            if file_count == 1 {
                "matched 1 file".to_string()
            } else {
                format!("matched {} files", file_count)
            }
        }
    };

    let mut lines = vec![ToolSummaryLine {
        text: primary,
        tone: ToolSummaryTone::Success,
    }];

    if let Some(window) = format_window_summary(applied_limit, applied_offset) {
        lines.push(ToolSummaryLine {
            text: window,
            tone: ToolSummaryTone::Warning,
        });
    }

    if !pattern.is_empty() {
        lines.push(ToolSummaryLine {
            text: format!("pattern: {}", truncate_for_summary(pattern, 48)),
            tone: ToolSummaryTone::Neutral,
        });
    }

    ToolResultSummary {
        lines,
        hide_body_by_default: true,
    }
}

fn summarize_glob(
    args: &Value,
    metadata: Option<&Value>,
    result_content: &str,
) -> ToolResultSummary {
    let match_count = metadata
        .and_then(|m| m.get("match_count"))
        .and_then(Value::as_u64)
        .unwrap_or_else(|| {
            if result_content.trim().is_empty() {
                0
            } else {
                result_content.lines().count() as u64
            }
        });

    let pattern = metadata
        .and_then(|m| m.get("pattern"))
        .and_then(Value::as_str)
        .or_else(|| args.get("pattern").and_then(Value::as_str))
        .unwrap_or("");

    let primary = if match_count == 1 {
        "matched 1 file".to_string()
    } else {
        format!("matched {} files", match_count)
    };

    let mut lines = vec![ToolSummaryLine {
        text: primary,
        tone: ToolSummaryTone::Success,
    }];

    if !pattern.is_empty() {
        lines.push(ToolSummaryLine {
            text: format!("pattern: {}", truncate_for_summary(pattern, 48)),
            tone: ToolSummaryTone::Neutral,
        });
    }

    ToolResultSummary {
        lines,
        hide_body_by_default: true,
    }
}

fn summarize_bash(metadata: Option<&Value>) -> ToolResultSummary {
    let Some(metadata) = metadata else {
        return ToolResultSummary::default();
    };

    let mut lines = Vec::new();
    if let Some(command_type) = metadata.get("command_type").and_then(Value::as_str) {
        let tone = if command_type == "generic" {
            ToolSummaryTone::Neutral
        } else {
            ToolSummaryTone::Warning
        };
        lines.push(ToolSummaryLine {
            text: format!("shell mode: {}", command_type),
            tone,
        });
    }
    if let Some(suggestion) = metadata.get("rewrite_suggestion").and_then(Value::as_str) {
        lines.push(ToolSummaryLine {
            text: suggestion.to_string(),
            tone: ToolSummaryTone::Warning,
        });
    }

    ToolResultSummary {
        lines,
        hide_body_by_default: false,
    }
}

fn format_window_summary(limit: Option<u64>, offset: Option<u64>) -> Option<String> {
    match (limit, offset) {
        (None, None) | (Some(0), None) | (Some(0), Some(0)) => None,
        (Some(limit), Some(offset)) if offset > 0 => {
            Some(format!("showing {} results after offset {}", limit, offset))
        }
        (Some(limit), _) => Some(format!("showing first {} results", limit)),
        (None, Some(offset)) if offset > 0 => Some(format!("skipped first {} results", offset)),
        _ => None,
    }
}

fn shorten_display_path(path: &str) -> String {
    let parts: Vec<&str> = path.rsplitn(3, '/').collect();
    if parts.len() >= 3 {
        format!(".../{}/{}", parts[1], parts[0])
    } else {
        path.to_string()
    }
}

fn truncate_for_summary(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    format!("{}...", text.chars().take(max_chars).collect::<String>())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{summarize_tool_result, ToolSummaryTone};

    #[test]
    fn summarize_read_file_hides_body() {
        let args = json!({"file_path": "/tmp/demo.txt"});
        let metadata = json!({
            "file_path": "/tmp/demo.txt",
            "total_lines": 120,
            "start_line": 21,
            "end_line": 40,
            "was_truncated": true
        });
        let summary = summarize_tool_result("read_file", &args, Some(&metadata), "body", false);
        assert!(summary.hide_body_by_default);
        assert_eq!(summary.lines[0].text, "read lines 21-40 of 120");
    }

    #[test]
    fn summarize_grep_uses_structured_counts() {
        let args = json!({"pattern": "todo"});
        let metadata = json!({
            "output_mode": "content",
            "line_count": 7,
            "file_count": 3,
            "pattern": "todo",
            "applied_limit": 5
        });
        let summary = summarize_tool_result("grep", &args, Some(&metadata), "body", false);
        assert!(summary.hide_body_by_default);
        assert_eq!(summary.lines[0].text, "matched 7 lines in 3 files");
        assert_eq!(summary.lines[1].tone, ToolSummaryTone::Warning);
    }
}
