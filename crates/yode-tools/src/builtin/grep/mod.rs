use std::path::Path;
use std::process::Command;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use std::collections::BTreeSet;

use crate::tool::{Tool, ToolCapabilities, ToolContext, ToolResult};

pub struct GrepTool;

#[async_trait]
impl Tool for GrepTool {
    fn name(&self) -> &str {
        "grep"
    }

    fn user_facing_name(&self) -> &str {
        "Search"
    }

    fn activity_description(&self, params: &Value) -> String {
        let pattern = params.get("pattern").and_then(|v| v.as_str()).unwrap_or("");
        format!("Searching for: {}", pattern)
    }

    fn description(&self) -> &str {
        r#"A powerful search tool built on ripgrep.

Usage:
- ALWAYS use Grep for search tasks. NEVER invoke `grep` or `rg` as a Bash command.
- Supports full regex syntax (e.g., "log.*Error", "function\s+\w+").
- Output modes: "content" (matching lines), "files_with_matches" (paths only), "count" (match counts).
- Supports multiline matching with multiline: true.
- Use Agent tool for open-ended searches requiring multiple rounds."#
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "Regex pattern to search for in file contents"
                },
                "path": {
                    "type": "string",
                    "description": "File or directory to search in. Defaults to current working directory."
                },
                "glob": {
                    "type": "string",
                    "description": "Glob pattern to filter files (e.g. '*.js', '*.{ts,tsx}') - maps to rg --glob"
                },
                "output_mode": {
                    "type": "string",
                    "enum": ["content", "files_with_matches", "count"],
                    "default": "files_with_matches",
                    "description": "Output mode: 'content' (matching lines), 'files_with_matches' (paths only), 'count' (match counts)."
                },
                "context": {
                    "type": "integer",
                    "description": "Number of lines to show before and after each match (rg -C)."
                },
                "context_before": {
                    "type": "integer",
                    "description": "Number of lines to show before each match (rg -B)."
                },
                "context_after": {
                    "type": "integer",
                    "description": "Number of lines to show after each match (rg -A)."
                },
                "case_insensitive": {
                    "type": "boolean",
                    "default": false,
                    "description": "Case insensitive search (rg -i)."
                },
                "multiline": {
                    "type": "boolean",
                    "default": false,
                    "description": "Enable multiline mode (rg -U)."
                },
                "head_limit": {
                    "type": "integer",
                    "default": 250,
                    "description": "Limit output to first N lines/entries. Pass 0 for unlimited."
                },
                "offset": {
                    "type": "integer",
                    "default": 0,
                    "description": "Skip first N lines/entries before applying head_limit."
                }
            },
            "required": ["pattern"]
        })
    }

    fn capabilities(&self) -> ToolCapabilities {
        ToolCapabilities {
            requires_confirmation: false,
            supports_auto_execution: true,
            read_only: true,
        }
    }

    async fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolResult> {
        let pattern = params.get("pattern").and_then(|v| v.as_str()).unwrap_or("");
        let path = params.get("path").and_then(|v| v.as_str()).unwrap_or(".");
        let output_mode = params
            .get("output_mode")
            .and_then(|v| v.as_str())
            .unwrap_or("files_with_matches");

        let mut args = vec!["--hidden".to_string()];

        // Output mode
        match output_mode {
            "files_with_matches" => args.push("-l".to_string()),
            "count" => args.push("-c".to_string()),
            _ => args.push("-n".to_string()), // content mode includes line numbers
        }

        // Context
        if let Some(c) = params.get("context").and_then(|v| v.as_u64()) {
            args.push("-C".to_string());
            args.push(c.to_string());
        } else {
            if let Some(b) = params.get("context_before").and_then(|v| v.as_u64()) {
                args.push("-B".to_string());
                args.push(b.to_string());
            }
            if let Some(a) = params.get("context_after").and_then(|v| v.as_u64()) {
                args.push("-A".to_string());
                args.push(a.to_string());
            }
        }

        if params
            .get("case_insensitive")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            args.push("-i".to_string());
        }

        if params
            .get("multiline")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            args.push("-U".to_string());
            args.push("--multiline-dotall".to_string());
        }

        if let Some(glob) = params.get("glob").and_then(|v| v.as_str()) {
            args.push("--glob".to_string());
            args.push(glob.to_string());
        }

        // Limit column length to avoid base64 noise
        args.push("--max-columns".to_string());
        args.push("500".to_string());

        // Pattern and Path
        args.push(pattern.to_string());
        args.push(path.to_string());

        let working_dir = ctx.working_dir.as_deref().unwrap_or_else(|| Path::new("."));

        // Execute ripgrep
        let output = match Command::new("rg")
            .args(&args)
            .current_dir(working_dir)
            .output()
        {
            Ok(o) => o,
            Err(_) => {
                // Fallback to internal implementation if rg is not installed
                return Ok(ToolResult::error(
                    "ripgrep (rg) is not installed in the system path.".to_string(),
                ));
            }
        };

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        if !output.status.success() && !stderr.is_empty() {
            return Ok(ToolResult::error(format!("rg error: {}", stderr)));
        }

        if stdout.is_empty() {
            return Ok(ToolResult::success_with_metadata(
                "No matches found.".to_string(),
                json!({
                    "pattern": pattern,
                    "path": path,
                    "output_mode": output_mode,
                    "file_count": 0,
                    "line_count": 0,
                    "match_count": 0,
                    "applied_limit": serde_json::Value::Null,
                    "applied_offset": 0,
                }),
            ));
        }

        // Apply head_limit and offset
        let head_limit = params
            .get("head_limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(250) as usize;
        let offset = params.get("offset").and_then(|v| v.as_u64()).unwrap_or(0) as usize;

        let lines: Vec<&str> = stdout.lines().collect();
        let total_count = lines.len();

        let start = offset.min(total_count);
        let end = if head_limit == 0 {
            total_count
        } else {
            (start + head_limit).min(total_count)
        };

        let result_lines = &lines[start..end];
        let mut final_output = result_lines.join("\n");

        if end < total_count {
            final_output.push_str(&format!(
                "\n\n[Showing results with pagination = limit: {}, offset: {}]",
                head_limit, offset
            ));
        }

        let applied_limit = if end < total_count && head_limit != 0 {
            Some(head_limit)
        } else {
            None
        };
        let file_count = match output_mode {
            "files_with_matches" | "count" => total_count,
            "content" => infer_content_file_count(result_lines, path, working_dir),
            _ => total_count,
        };
        let line_count = if output_mode == "content" {
            result_lines
                .iter()
                .filter(|line| !line.trim().is_empty() && **line != "--")
                .count()
        } else {
            0
        };
        let match_count = if output_mode == "count" {
            result_lines
                .iter()
                .filter_map(|line| parse_count_line(line))
                .sum()
        } else {
            0
        };

        Ok(ToolResult::success_with_metadata(
            final_output,
            json!({
                "pattern": pattern,
                "path": path,
                "output_mode": output_mode,
                "file_count": file_count,
                "line_count": line_count,
                "match_count": match_count,
                "applied_limit": applied_limit,
                "applied_offset": offset,
            }),
        ))
    }
}

fn parse_count_line(line: &str) -> Option<usize> {
    let trimmed = line.trim();
    if let Ok(value) = trimmed.parse::<usize>() {
        return Some(value);
    }
    trimmed.rsplit(':').next()?.parse().ok()
}

fn infer_content_file_count(lines: &[&str], search_path: &str, working_dir: &Path) -> usize {
    let candidate = working_dir.join(search_path);
    if candidate.is_file() {
        return 1;
    }

    let mut files = BTreeSet::new();
    for line in lines {
        if let Some((file, _rest)) = split_grep_content_prefix(line) {
            files.insert(file.to_string());
        }
    }
    files.len().max(1)
}

fn split_grep_content_prefix(line: &str) -> Option<(&str, &str)> {
    let (prefix, rest) = line.split_once(':')?;
    if prefix.chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }
    Some((prefix, rest))
}

#[cfg(test)]
mod tests {
    use super::{infer_content_file_count, parse_count_line};
    use std::path::Path;

    #[test]
    fn parse_count_line_supports_plain_and_prefixed_output() {
        assert_eq!(parse_count_line("4"), Some(4));
        assert_eq!(parse_count_line("src/main.rs:12"), Some(12));
    }

    #[test]
    fn infer_content_file_count_uses_unique_file_prefixes() {
        let lines = vec![
            "src/main.rs:12:todo",
            "src/lib.rs:8:todo",
            "src/main.rs:13:todo",
        ];
        assert_eq!(infer_content_file_count(&lines, ".", Path::new(".")), 2);
    }
}
