use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};

use crate::tool::{Tool, ToolContext, ToolResult};

pub struct WriteFileTool;

#[async_trait]
impl Tool for WriteFileTool {
    fn name(&self) -> &str {
        "write_file"
    }

    fn user_facing_name(&self) -> &str {
        "Write File"
    }

    fn activity_description(&self, params: &Value) -> String {
        let file_path = params
            .get("file_path")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        format!("Writing file: {}", file_path)
    }

    fn description(&self) -> &str {
        r#"Writes a file to the local filesystem.

Usage:
- This tool will overwrite the existing file if there is one at the provided path.
- If this is an existing file, you MUST use the `read_file` tool first to read the file's contents. This tool will fail if you did not read the file first.
- Prefer the `edit_file` tool for modifying existing files — it only sends the diff. Only use this tool to create new files or for complete rewrites.
- NEVER create documentation files (*.md) or README files unless explicitly requested by the User.
- Only use emojis if the user explicitly requests it. Avoid writing emojis to files unless asked."#
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Absolute path to write to"
                },
                "content": {
                    "type": "string",
                    "description": "Content to write"
                }
            },
            "required": ["file_path", "content"]
        })
    }

    fn requires_confirmation(&self) -> bool {
        true
    }

    async fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolResult> {
        let file_path = params
            .get("file_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: file_path"))?;

        let content = params
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: content"))?;

        let path = std::path::Path::new(file_path);

        // --- Mandatory Pre-read Check for Existing Files ---
        if path.exists() {
            if let Some(history) = &ctx.read_file_history {
                let h = history.lock().await;
                if !h.contains(&std::path::PathBuf::from(file_path)) {
                    return Ok(ToolResult::error_typed(
                        format!("File '{}' exists but has not been read yet. You must use 'read_file' before overwriting an existing file.", file_path),
                        crate::tool::ToolErrorType::Validation,
                        true,
                        Some(format!("Call read_file(file_path=\"{}\") first.", file_path)),
                    ));
                }
            }
        }

        tracing::debug!(file_path = %file_path, "Writing file");

        // Create parent directories if they don't exist
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                tracing::debug!(parent = %parent.display(), "Creating parent directories");
                if let Err(e) = tokio::fs::create_dir_all(parent).await {
                    tracing::warn!(
                        parent = %parent.display(),
                        error = %e,
                        "Failed to create parent directories"
                    );
                    return Ok(ToolResult::error(format!(
                        "Failed to create parent directories for '{}': {}",
                        file_path, e
                    )));
                }
            }
        }

        match tokio::fs::write(file_path, content).await {
            Ok(()) => {
                let byte_count = content.len();
                let line_count = content.lines().count();
                let preview_lines = content
                    .lines()
                    .take(5)
                    .map(|line| line.to_string())
                    .collect::<Vec<_>>();
                tracing::debug!(
                    file_path = %file_path,
                    bytes = byte_count,
                    "File written successfully"
                );
                let metadata = json!({
                    "file_path": file_path,
                    "byte_count": byte_count,
                    "line_count": line_count,
                    "diff_preview": {
                        "removed": [],
                        "added": preview_lines,
                        "more_removed": 0,
                        "more_added": line_count.saturating_sub(5),
                    },
                });
                Ok(ToolResult::success_with_metadata(
                    format!(
                        "Successfully wrote {} bytes ({} lines) to '{}'",
                        byte_count, line_count, file_path
                    ),
                    metadata,
                ))
            }
            Err(e) => {
                tracing::warn!(file_path = %file_path, error = %e, "Failed to write file");
                Ok(ToolResult::error(format!(
                    "Failed to write file '{}': {}",
                    file_path, e
                )))
            }
        }
    }
}
