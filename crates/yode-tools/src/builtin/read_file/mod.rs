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
        let start = offset.saturating_sub(1);
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

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::sync::Arc;

    use serde_json::json;
    use tokio::sync::Mutex;

    use crate::tool::{Tool, ToolContext, ToolErrorType};

    use super::ReadFileTool;

    fn temp_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("yode-read-file-{}-{}", name, uuid::Uuid::new_v4()))
    }

    #[tokio::test]
    async fn reads_offset_limit_and_records_history() {
        let path = temp_path("range.txt");
        tokio::fs::write(&path, "one\ntwo\nthree\nfour\n").await.unwrap();

        let history = Arc::new(Mutex::new(HashSet::new()));
        let mut ctx = ToolContext::empty();
        ctx.read_file_history = Some(history.clone());

        let result = ReadFileTool
            .execute(
                json!({
                    "file_path": path.display().to_string(),
                    "offset": 2,
                    "limit": 2
                }),
                &ctx,
            )
            .await
            .unwrap();

        assert!(!result.is_error);
        assert!(result.content.contains("     2\ttwo"));
        assert!(result.content.contains("     3\tthree"));
        assert!(!result.content.contains("     1\tone"));
        assert_eq!(
            result.metadata.as_ref().unwrap()["start_line"],
            json!(2)
        );
        assert_eq!(result.metadata.as_ref().unwrap()["end_line"], json!(3));
        assert_eq!(
            result.metadata.as_ref().unwrap()["was_truncated"],
            json!(true)
        );

        let recorded = history.lock().await;
        assert!(recorded.contains(&path));

        let _ = tokio::fs::remove_file(&path).await;
    }

    #[tokio::test]
    async fn returns_validation_error_for_directories() {
        let dir = temp_path("dir");
        tokio::fs::create_dir_all(&dir).await.unwrap();

        let result = ReadFileTool
            .execute(
                json!({
                    "file_path": dir.display().to_string()
                }),
                &ToolContext::empty(),
            )
            .await
            .unwrap();

        assert!(result.is_error);
        assert_eq!(result.error_type, Some(ToolErrorType::Validation));
        assert!(result.content.contains("is a directory"));
        assert!(
            result
                .suggestion
                .as_deref()
                .unwrap_or("")
                .contains("Call ls")
        );

        let _ = tokio::fs::remove_dir_all(&dir).await;
    }
}
