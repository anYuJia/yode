use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use std::path::Path;

use crate::tool::{Tool, ToolContext, ToolResult};

pub struct ReadFileTool;

#[async_trait]
impl Tool for ReadFileTool {
    fn name(&self) -> &str {
        "read_file"
    }

    fn user_facing_name(&self) -> &str {
        "Read File"
    }

    fn activity_description(&self, params: &Value) -> String {
        let file_path = params
            .get("file_path")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        format!("Reading file: {}", file_path)
    }

    fn description(&self) -> &str {
        r#"Reads a file from the local filesystem. You can access any file directly by using this tool.

Usage:
- The file_path parameter must be an absolute path.
- By default, it reads up to 2000 lines. Use offset and limit for larger files.
- Results are returned with line numbers starting at 1. When editing, preserve the exact indentation as it appears AFTER the line number prefix."#
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Absolute path to the file"
                },
                "offset": {
                    "type": "integer",
                    "description": "Line number to start from (1-based, inclusive)"
                },
                "limit": {
                    "type": "integer",
                    "description": "Max lines to read"
                }
            },
            "required": ["file_path"]
        })
    }

    fn requires_confirmation(&self) -> bool {
        false
    }

    async fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolResult> {
        let file_path = params
            .get("file_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: file_path"))?;

        // Update read history
        if let Some(history) = &ctx.read_file_history {
            let mut h = history.lock().await;
            h.insert(std::path::PathBuf::from(file_path));
        }

        let offset = params
            .get("offset")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .unwrap_or(1);

        let limit = params
            .get("limit")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .or(Some(2000)); // Default limit: 2000 lines

        tracing::debug!(
            file_path = %file_path,
            offset = offset,
            limit = ?limit,
            "Reading file"
        );

        let path = Path::new(file_path);
        if path.is_dir() {
            return Ok(ToolResult::error_typed(
                format!(
                    "'{}' is a directory, not a file. Use 'ls' to list its contents.",
                    file_path
                ),
                crate::tool::ToolErrorType::Validation,
                true,
                Some(format!("Call ls(path=\"{}\") instead.", file_path)),
            ));
        }

        let content = match tokio::fs::read_to_string(file_path).await {
            Ok(content) => content,
            Err(e) => {
                tracing::warn!(file_path = %file_path, error = %e, "Failed to read file");
                return Ok(ToolResult::error(format!(
                    "Failed to read file '{}': {}",
                    file_path, e
                )));
            }
        };

        let lines: Vec<&str> = content.lines().collect();
        let total_lines = lines.len();

        // offset is 1-based; clamp to valid range
        let start = if offset >= 1 { offset - 1 } else { 0 };
        let start = start.min(total_lines);

        let end = match limit {
            Some(lim) => (start + lim).min(total_lines),
            None => total_lines,
        };

        let mut output = String::new();
        let was_truncated = end < total_lines && limit.is_some();
        for (idx, line) in lines[start..end].iter().enumerate() {
            let line_num = start + idx + 1; // 1-based line number
                                            // cat -n format: right-justified 6-wide line number, then tab, then content
            output.push_str(&format!("{:>6}\t{}\n", line_num, line));
        }

        if was_truncated {
            output.push_str(&format!(
                "\n... (showing lines {}-{} of {} total, use offset/limit to read more)\n",
                start + 1,
                end,
                total_lines
            ));
        }

        tracing::debug!(
            file_path = %file_path,
            lines_returned = end - start,
            total_lines = total_lines,
            "File read successfully"
        );

        let metadata = json!({
            "file_path": file_path,
            "total_lines": total_lines,
            "start_line": start + 1,
            "end_line": end,
            "was_truncated": was_truncated,
            "file_size": content.len(),
        });

        Ok(ToolResult::success_with_metadata(output, metadata))
    }
}
