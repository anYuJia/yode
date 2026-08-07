use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use std::path::Path;
use tokio::io::{AsyncBufReadExt, BufReader};

use crate::tool::{Tool, ToolCapabilities, ToolContext, ToolResult};

pub struct ReadFileTool;

const DEFAULT_READ_LINES: usize = 2_000;
const MAX_READ_LINES: usize = 20_000;

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

    fn capabilities(&self) -> ToolCapabilities {
        ToolCapabilities {
            requires_confirmation: false,
            supports_auto_execution: true,
            read_only: true,
        }
    }

    async fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolResult> {
        let file_path = params
            .get("file_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: file_path"))?;

        let offset = params
            .get("offset")
            .and_then(|v| v.as_u64())
            .and_then(|value| usize::try_from(value).ok())
            .unwrap_or(1)
            .max(1);

        let limit = params
            .get("limit")
            .and_then(|v| v.as_u64())
            .and_then(|value| usize::try_from(value).ok())
            .unwrap_or(DEFAULT_READ_LINES);
        if limit == 0 || limit > MAX_READ_LINES {
            return Ok(ToolResult::error_typed(
                format!("limit must be between 1 and {MAX_READ_LINES} lines"),
                crate::tool::ToolErrorType::Validation,
                true,
                Some("Use pagination with a smaller limit.".to_string()),
            ));
        }

        tracing::debug!(
            file_path = %file_path,
            offset = offset,
            limit = limit,
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

        let metadata = match tokio::fs::metadata(path).await {
            Ok(metadata) => metadata,
            Err(e) => {
                tracing::warn!(file_path = %file_path, error = %e, "Failed to read file");
                return Ok(ToolResult::error(format!(
                    "Failed to read file '{}': {}",
                    file_path, e
                )));
            }
        };
        let file = match tokio::fs::File::open(path).await {
            Ok(file) => file,
            Err(e) => {
                tracing::warn!(file_path = %file_path, error = %e, "Failed to open file");
                return Ok(ToolResult::error(format!(
                    "Failed to read file '{}': {}",
                    file_path, e
                )));
            }
        };
        let mut reader = BufReader::new(file);
        let mut output = String::new();
        let mut line = String::new();
        let mut line_number = 0usize;
        let mut returned = 0usize;
        let mut has_more = false;
        loop {
            if ctx
                .cancellation
                .as_ref()
                .is_some_and(tokio_util::sync::CancellationToken::is_cancelled)
            {
                return Ok(ToolResult::error_typed(
                    "File read cancelled.".to_string(),
                    crate::tool::ToolErrorType::Execution,
                    false,
                    None,
                ));
            }
            line.clear();
            let bytes = match reader.read_line(&mut line).await {
                Ok(bytes) => bytes,
                Err(error) => {
                    return Ok(ToolResult::error(format!(
                        "Failed to read file '{}': {}",
                        file_path, error
                    )));
                }
            };
            if bytes == 0 {
                break;
            }
            line_number = line_number.saturating_add(1);
            if line_number < offset {
                continue;
            }
            if returned >= limit {
                has_more = true;
                break;
            }
            let text = line.strip_suffix('\n').unwrap_or(&line);
            let text = text.strip_suffix('\r').unwrap_or(text);
            output.push_str(&format!("{:>6}\t{}\n", line_number, text));
            returned = returned.saturating_add(1);
        }

        if has_more {
            output.push_str(&format!(
                "\n... (showing lines {}-{}, more lines are available; use offset/limit to continue)\n",
                offset,
                offset.saturating_add(returned).saturating_sub(1),
            ));
        }

        // A failed read must never authorize a subsequent overwrite.
        if let Some(history) = &ctx.read_file_history {
            history
                .lock()
                .await
                .insert(normalize_history_path(file_path));
        }

        let end_line = if returned == 0 {
            offset.saturating_sub(1).min(line_number)
        } else {
            offset.saturating_add(returned).saturating_sub(1)
        };
        let total_lines = (!has_more).then_some(line_number);

        tracing::debug!(
            file_path = %file_path,
            lines_returned = returned,
            lines_scanned = line_number,
            total_lines = ?total_lines,
            "File read successfully"
        );

        let metadata = json!({
            "file_path": file_path,
            "total_lines": total_lines,
            "total_lines_known": total_lines.is_some(),
            "start_line": offset,
            "end_line": end_line,
            "was_truncated": has_more,
            "file_size": metadata.len(),
            "lines_scanned": line_number,
        });

        Ok(ToolResult::success_with_metadata(output, metadata))
    }
}

fn normalize_history_path(file_path: &str) -> std::path::PathBuf {
    std::fs::canonicalize(file_path).unwrap_or_else(|_| std::path::PathBuf::from(file_path))
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
        tokio::fs::write(&path, "one\ntwo\nthree\nfour\n")
            .await
            .unwrap();

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
        assert_eq!(result.metadata.as_ref().unwrap()["start_line"], json!(2));
        assert_eq!(result.metadata.as_ref().unwrap()["end_line"], json!(3));
        assert_eq!(
            result.metadata.as_ref().unwrap()["was_truncated"],
            json!(true)
        );

        let recorded = history.lock().await;
        let normalized_path = super::normalize_history_path(path.to_str().unwrap());
        assert!(recorded.contains(&normalized_path));

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
        assert!(result
            .suggestion
            .as_deref()
            .unwrap_or("")
            .contains("Call ls"));

        let _ = tokio::fs::remove_dir_all(&dir).await;
    }
}
