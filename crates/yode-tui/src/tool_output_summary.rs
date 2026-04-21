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

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct ShellOutputSections {
    pub stdout_lines: Vec<String>,
    pub stderr_lines: Vec<String>,
    pub exit_code: Option<i32>,
}

pub(crate) fn summarize_tool_result(
    tool_name: &str,
    args: &Value,
    metadata: Option<&Value>,
    result_content: &str,
    is_error: bool,
) -> ToolResultSummary {
    if is_error {
        return summarize_failed_tool_result(tool_name, metadata, result_content);
    }

    match tool_name {
        "read_file" => summarize_read_file(args, metadata),
        "grep" => summarize_grep(args, metadata, result_content),
        "glob" => summarize_glob(args, metadata, result_content),
        "ls" => summarize_ls(args, metadata, result_content),
        "memory" => summarize_memory(args, metadata, result_content),
        "skill" => summarize_skill(args, metadata, result_content),
        "discover_skills" => summarize_discover_skills(result_content),
        "lsp" => summarize_lsp(args, metadata),
        "web_search" => summarize_web_search(args, metadata),
        "web_fetch" => summarize_web_fetch(args, metadata),
        "project_map" => summarize_project_map(metadata),
        "bash" | "powershell" => summarize_shell_command(metadata, result_content),
        _ => ToolResultSummary::default(),
    }
}

fn summarize_failed_tool_result(
    tool_name: &str,
    metadata: Option<&Value>,
    result_content: &str,
) -> ToolResultSummary {
    let mut lines = Vec::new();

    if matches!(tool_name, "bash" | "powershell") {
        let sections = parse_shell_output_sections(result_content);
        if let Some(exit_code) = sections.exit_code {
            lines.push(ToolSummaryLine {
                text: format!("failed with exit code {}", exit_code),
                tone: ToolSummaryTone::Warning,
            });
        } else if let Some(first_stderr) = sections.stderr_lines.first() {
            lines.push(ToolSummaryLine {
                text: truncate_for_summary(first_stderr, 72),
                tone: ToolSummaryTone::Warning,
            });
        } else if let Some(first_stdout) = sections.stdout_lines.first() {
            lines.push(ToolSummaryLine {
                text: truncate_for_summary(first_stdout, 72),
                tone: ToolSummaryTone::Warning,
            });
        }
    }

    if lines.is_empty() {
        if let Some(error_type) = metadata
            .and_then(|m| m.get("error_type"))
            .and_then(Value::as_str)
        {
            lines.push(ToolSummaryLine {
                text: humanize_error_type(error_type),
                tone: ToolSummaryTone::Warning,
            });
        }

        if let Some(first_line) = result_content.lines().find(|line| !line.trim().is_empty()) {
            let text = truncate_for_summary(first_line.trim(), 96);
            if lines.first().map(|line| line.text.as_str()) != Some(text.as_str()) {
                lines.push(ToolSummaryLine {
                    text,
                    tone: ToolSummaryTone::Warning,
                });
            }
        }
    }

    if metadata
        .and_then(|m| m.get("recoverable"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        lines.push(ToolSummaryLine {
            text: "recoverable error".to_string(),
            tone: ToolSummaryTone::Neutral,
        });
    }

    ToolResultSummary {
        lines,
        hide_body_by_default: false,
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

fn summarize_ls(
    args: &Value,
    metadata: Option<&Value>,
    result_content: &str,
) -> ToolResultSummary {
    let file_count = metadata
        .and_then(|m| m.get("file_count"))
        .and_then(Value::as_u64)
        .unwrap_or_else(|| {
            result_content
                .lines()
                .filter(|line| !line.ends_with('/'))
                .count() as u64
        });
    let dir_count = metadata
        .and_then(|m| m.get("dir_count"))
        .and_then(Value::as_u64)
        .unwrap_or_else(|| result_content.lines().filter(|line| line.ends_with('/')).count() as u64);
    let path = metadata
        .and_then(|m| m.get("path"))
        .and_then(Value::as_str)
        .or_else(|| args.get("path").and_then(Value::as_str))
        .unwrap_or(".");
    let recursive = metadata
        .and_then(|m| m.get("recursive"))
        .and_then(Value::as_bool)
        .unwrap_or_else(|| args.get("recursive").and_then(Value::as_bool).unwrap_or(false));

    let primary = match (file_count, dir_count) {
        (0, 0) => "listed empty directory".to_string(),
        (files, 0) if files == 1 => "listed 1 file".to_string(),
        (files, 0) => format!("listed {} files", files),
        (0, dirs) if dirs == 1 => "listed 1 directory".to_string(),
        (0, dirs) => format!("listed {} directories", dirs),
        (files, dirs) => format!("listed {} files and {} directories", files, dirs),
    };

    let mut lines = vec![ToolSummaryLine {
        text: primary,
        tone: ToolSummaryTone::Success,
    }];

    lines.push(ToolSummaryLine {
        text: shorten_display_path(path),
        tone: ToolSummaryTone::Neutral,
    });

    if recursive {
        lines.push(ToolSummaryLine {
            text: "recursive view".to_string(),
            tone: ToolSummaryTone::Warning,
        });
    }

    ToolResultSummary {
        lines,
        hide_body_by_default: true,
    }
}

fn summarize_memory(
    args: &Value,
    metadata: Option<&Value>,
    result_content: &str,
) -> ToolResultSummary {
    let action = metadata
        .and_then(|m| m.get("action"))
        .and_then(Value::as_str)
        .or_else(|| args.get("action").and_then(Value::as_str))
        .unwrap_or("");
    let scope = metadata
        .and_then(|m| m.get("scope"))
        .and_then(Value::as_str)
        .or_else(|| args.get("scope").and_then(Value::as_str))
        .unwrap_or("project");
    let name = metadata
        .and_then(|m| m.get("name"))
        .and_then(Value::as_str)
        .or_else(|| args.get("name").and_then(Value::as_str))
        .unwrap_or("");

    match action {
        "read" => {
            let mut lines = vec![ToolSummaryLine {
                text: "recalled 1 memory".to_string(),
                tone: ToolSummaryTone::Success,
            }];
            if !name.is_empty() {
                lines.push(ToolSummaryLine {
                    text: format!("{} ({})", name, scope),
                    tone: ToolSummaryTone::Neutral,
                });
            }
            ToolResultSummary {
                lines,
                hide_body_by_default: true,
            }
        }
        "list" => {
            let count = metadata
                .and_then(|m| m.get("count"))
                .and_then(Value::as_u64)
                .unwrap_or_else(|| {
                    if result_content.trim() == "No memories found." {
                        0
                    } else {
                        result_content.lines().count() as u64
                    }
                });
            ToolResultSummary {
                lines: vec![
                    ToolSummaryLine {
                        text: if count == 1 {
                            "listed 1 memory".to_string()
                        } else {
                            format!("listed {} memories", count)
                        },
                        tone: ToolSummaryTone::Success,
                    },
                    ToolSummaryLine {
                        text: format!("scope: {}", scope),
                        tone: ToolSummaryTone::Neutral,
                    },
                ],
                hide_body_by_default: true,
            }
        }
        _ => ToolResultSummary::default(),
    }
}

fn summarize_skill(
    args: &Value,
    metadata: Option<&Value>,
    result_content: &str,
) -> ToolResultSummary {
    let action = metadata
        .and_then(|m| m.get("action"))
        .and_then(Value::as_str)
        .or_else(|| args.get("action").and_then(Value::as_str))
        .unwrap_or("get");

    match action {
        "list" => {
            let count = metadata
                .and_then(|m| m.get("count"))
                .and_then(Value::as_u64)
                .unwrap_or_else(|| result_content.lines().filter(|line| line.contains(" /") || line.contains(" — ")).count() as u64);
            ToolResultSummary {
                lines: vec![ToolSummaryLine {
                    text: if count == 1 {
                        "listed 1 skill".to_string()
                    } else {
                        format!("listed {} skills", count)
                    },
                    tone: ToolSummaryTone::Success,
                }],
                hide_body_by_default: true,
            }
        }
        "get" => {
            let name = metadata
                .and_then(|m| m.get("name"))
                .and_then(Value::as_str)
                .or_else(|| args.get("name").and_then(Value::as_str))
                .unwrap_or("");
            let mut lines = vec![ToolSummaryLine {
                text: "read 1 skill".to_string(),
                tone: ToolSummaryTone::Success,
            }];
            if !name.is_empty() {
                lines.push(ToolSummaryLine {
                    text: name.to_string(),
                    tone: ToolSummaryTone::Neutral,
                });
            }
            ToolResultSummary {
                lines,
                hide_body_by_default: true,
            }
        }
        _ => ToolResultSummary::default(),
    }
}

fn summarize_discover_skills(result_content: &str) -> ToolResultSummary {
    let count = result_content
        .lines()
        .filter(|line| line.trim_start().starts_with("- **"))
        .count() as u64;

    ToolResultSummary {
        lines: vec![ToolSummaryLine {
            text: if count == 0 {
                "listed 0 skills".to_string()
            } else if count == 1 {
                "listed 1 skill".to_string()
            } else {
                format!("listed {} skills", count)
            },
            tone: ToolSummaryTone::Success,
        }],
        hide_body_by_default: true,
    }
}

fn summarize_lsp(args: &Value, metadata: Option<&Value>) -> ToolResultSummary {
    let operation = metadata
        .and_then(|m| m.get("operation"))
        .and_then(Value::as_str)
        .or_else(|| args.get("operation").and_then(Value::as_str))
        .unwrap_or("query");
    let file_path = metadata
        .and_then(|m| m.get("file_path"))
        .and_then(Value::as_str)
        .or_else(|| args.get("filePath").and_then(Value::as_str))
        .unwrap_or("");
    let line = metadata
        .and_then(|m| m.get("line"))
        .and_then(Value::as_u64)
        .or_else(|| args.get("line").and_then(Value::as_u64))
        .unwrap_or(0);
    let character = metadata
        .and_then(|m| m.get("character"))
        .and_then(Value::as_u64)
        .or_else(|| args.get("character").and_then(Value::as_u64))
        .unwrap_or(0);

    let primary = match operation {
        "hover" => "inspected symbol hover".to_string(),
        "goToDefinition" => "jumped to definition".to_string(),
        "findReferences" => "found symbol references".to_string(),
        "documentSymbol" => "listed document symbols".to_string(),
        _ => format!("ran lsp {}", operation),
    };
    let mut lines = vec![ToolSummaryLine {
        text: primary,
        tone: ToolSummaryTone::Success,
    }];
    if !file_path.is_empty() {
        lines.push(ToolSummaryLine {
            text: format!("{}:{}:{}", shorten_display_path(file_path), line + 1, character + 1),
            tone: ToolSummaryTone::Neutral,
        });
    }

    ToolResultSummary {
        lines,
        hide_body_by_default: true,
    }
}

fn summarize_web_search(args: &Value, metadata: Option<&Value>) -> ToolResultSummary {
    let Some(metadata) = metadata else {
        return ToolResultSummary::default();
    };

    let result_count = metadata
        .get("result_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let query = metadata
        .get("query")
        .and_then(Value::as_str)
        .or_else(|| args.get("query").and_then(Value::as_str))
        .unwrap_or("");

    let primary = if result_count == 0 {
        "found no web results".to_string()
    } else if result_count == 1 {
        "found 1 web result".to_string()
    } else {
        format!("found {} web results", result_count)
    };

    let mut lines = vec![ToolSummaryLine {
        text: primary,
        tone: ToolSummaryTone::Success,
    }];
    if !query.is_empty() {
        lines.push(ToolSummaryLine {
            text: format!("query: {}", truncate_for_summary(query, 56)),
            tone: ToolSummaryTone::Neutral,
        });
    }

    ToolResultSummary {
        lines,
        hide_body_by_default: true,
    }
}

fn summarize_web_fetch(args: &Value, metadata: Option<&Value>) -> ToolResultSummary {
    let Some(metadata) = metadata else {
        return ToolResultSummary::default();
    };

    let url = metadata
        .get("url")
        .and_then(Value::as_str)
        .or_else(|| args.get("url").and_then(Value::as_str))
        .unwrap_or("");
    let content_type = metadata
        .get("content_type")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let original_length = metadata
        .get("original_length")
        .and_then(Value::as_u64)
        .unwrap_or(0);

    let mut lines = vec![ToolSummaryLine {
        text: format!(
            "fetched {} of {}",
            human_readable_size(original_length),
            truncate_for_summary(content_type, 28)
        ),
        tone: ToolSummaryTone::Success,
    }];
    if !url.is_empty() {
        lines.push(ToolSummaryLine {
            text: truncate_for_summary(url, 72),
            tone: ToolSummaryTone::Neutral,
        });
    }

    ToolResultSummary {
        lines,
        hide_body_by_default: true,
    }
}

fn summarize_project_map(metadata: Option<&Value>) -> ToolResultSummary {
    let Some(metadata) = metadata else {
        return ToolResultSummary::default();
    };

    let project_type = metadata
        .get("project_type")
        .and_then(Value::as_str)
        .unwrap_or("project");
    let file_count = metadata
        .get("file_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let total_lines = metadata
        .get("total_lines")
        .and_then(Value::as_u64)
        .unwrap_or(0);

    ToolResultSummary {
        lines: vec![
            ToolSummaryLine {
                text: format!("mapped {} project", project_type.to_ascii_lowercase()),
                tone: ToolSummaryTone::Success,
            },
            ToolSummaryLine {
                text: format!("{} files · {} lines", file_count, total_lines),
                tone: ToolSummaryTone::Neutral,
            },
        ],
        hide_body_by_default: true,
    }
}

fn summarize_shell_command(metadata: Option<&Value>, result_content: &str) -> ToolResultSummary {
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

    let sections = parse_shell_output_sections(result_content);
    let command_type = metadata
        .get("command_type")
        .and_then(Value::as_str)
        .unwrap_or("generic");

    ToolResultSummary {
        lines,
        hide_body_by_default: matches!(command_type, "search" | "read" | "list")
            && sections.stderr_lines.is_empty()
            && sections.exit_code.is_none(),
    }
}

pub(crate) fn parse_shell_output_sections(result_content: &str) -> ShellOutputSections {
    let mut sections = ShellOutputSections::default();
    let mut in_stderr = false;

    for line in result_content.lines() {
        if line == "[stderr]" {
            in_stderr = true;
            continue;
        }
        if let Some(code) = parse_exit_code_line(line) {
            sections.exit_code = Some(code);
            continue;
        }
        if in_stderr {
            sections.stderr_lines.push(line.to_string());
        } else {
            sections.stdout_lines.push(line.to_string());
        }
    }

    sections
}

fn parse_exit_code_line(line: &str) -> Option<i32> {
    line.trim()
        .strip_prefix("[exit code: ")?
        .strip_suffix(']')?
        .parse::<i32>()
        .ok()
}

fn humanize_error_type(error_type: &str) -> String {
    match error_type {
        "Validation" => "validation error".to_string(),
        "Protocol" => "protocol error".to_string(),
        "NotFound" => "not found".to_string(),
        "PermissionDeny" => "permission denied".to_string(),
        "Permission" => "permission error".to_string(),
        "Execution" => "execution failed".to_string(),
        "QuotaExceeded" => "quota exceeded".to_string(),
        "Timeout" => "timed out".to_string(),
        "Unknown" => "unknown error".to_string(),
        other => other.to_ascii_lowercase(),
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

fn human_readable_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;

    if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{parse_shell_output_sections, summarize_tool_result, ToolSummaryTone};

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

    #[test]
    fn summarize_ls_hides_body_and_reports_counts() {
        let args = json!({"path": "/tmp/demo", "recursive": true});
        let metadata = json!({
            "path": "/tmp/demo",
            "recursive": true,
            "file_count": 3,
            "dir_count": 2
        });
        let summary = summarize_tool_result("ls", &args, Some(&metadata), "src/\nsrc/main.rs", false);
        assert!(summary.hide_body_by_default);
        assert_eq!(summary.lines[0].text, "listed 3 files and 2 directories");
        assert_eq!(summary.lines[2].text, "recursive view");
    }

    #[test]
    fn summarize_web_fetch_hides_body_and_reports_size() {
        let args = json!({"url": "https://example.com"});
        let metadata = json!({
            "url": "https://example.com",
            "content_type": "text/html; charset=utf-8",
            "original_length": 4096
        });
        let summary = summarize_tool_result("web_fetch", &args, Some(&metadata), "<html>...</html>", false);
        assert!(summary.hide_body_by_default);
        assert_eq!(summary.lines[0].text, "fetched 4.0 KB of text/html; charset=utf-8");
    }

    #[test]
    fn summarize_project_map_hides_body() {
        let metadata = json!({
            "project_type": "Rust",
            "file_count": 12,
            "total_lines": 840
        });
        let summary = summarize_tool_result("project_map", &json!({}), Some(&metadata), "## Project Overview", false);
        assert!(summary.hide_body_by_default);
        assert_eq!(summary.lines[0].text, "mapped rust project");
        assert_eq!(summary.lines[1].text, "12 files · 840 lines");
    }

    #[test]
    fn summarize_memory_read_hides_body() {
        let summary = summarize_tool_result(
            "memory",
            &json!({"action": "read", "name": "plan", "scope": "project"}),
            Some(&json!({"action": "read", "name": "plan", "scope": "project"})),
            "important memory body",
            false,
        );
        assert!(summary.hide_body_by_default);
        assert_eq!(summary.lines[0].text, "recalled 1 memory");
        assert_eq!(summary.lines[1].text, "plan (project)");
    }

    #[test]
    fn summarize_lsp_hides_body() {
        let summary = summarize_tool_result(
            "lsp",
            &json!({"operation": "hover", "filePath": "/tmp/src/main.rs", "line": 4, "character": 2}),
            Some(&json!({"operation": "hover", "file_path": "/tmp/src/main.rs", "line": 4, "character": 2})),
            "{\"contents\":\"demo\"}",
            false,
        );
        assert!(summary.hide_body_by_default);
        assert_eq!(summary.lines[0].text, "inspected symbol hover");
        assert!(summary.lines[1].text.contains(".../src/main.rs:5:3"));
    }

    #[test]
    fn summarize_shell_command_hides_body_for_read_style_commands() {
        let summary = summarize_tool_result(
            "bash",
            &json!({}),
            Some(&json!({
                "command_type": "read",
                "rewrite_suggestion": "Prefer read_file"
            })),
            "hello\nworld",
            false,
        );
        assert!(summary.hide_body_by_default);
    }

    #[test]
    fn parse_shell_output_sections_splits_stdout_stderr_and_exit_code() {
        let sections = parse_shell_output_sections("ok\n[stderr]\nwarn\n[exit code: 2]");
        assert_eq!(sections.stdout_lines, vec!["ok".to_string()]);
        assert_eq!(sections.stderr_lines, vec!["warn".to_string()]);
        assert_eq!(sections.exit_code, Some(2));
    }

    #[test]
    fn summarize_failed_tool_result_surfaces_error_type_and_first_line() {
        let summary = summarize_tool_result(
            "read_file",
            &json!({}),
            Some(&json!({
                "error_type": "Validation",
                "recoverable": true
            })),
            "File is too large to read at once.",
            true,
        );
        assert!(!summary.hide_body_by_default);
        assert_eq!(summary.lines[0].text, "validation error");
        assert_eq!(summary.lines[1].text, "File is too large to read at once.");
        assert_eq!(summary.lines[2].text, "recoverable error");
    }

    #[test]
    fn summarize_failed_shell_result_surfaces_exit_code() {
        let summary = summarize_tool_result(
            "bash",
            &json!({}),
            Some(&json!({
                "error_type": "Execution"
            })),
            "ok\n[stderr]\nwarn\n[exit code: 2]",
            true,
        );
        assert_eq!(summary.lines[0].text, "failed with exit code 2");
    }
}
