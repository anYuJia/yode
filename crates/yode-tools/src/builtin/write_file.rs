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

    fn description(&self) -> &str {
        "Write content to a file. Creates parent directories if they do not exist."
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

    async fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolResult> {
        let file_path = params
            .get("file_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: file_path"))?;

        let content = params
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: content"))?;

        tracing::debug!(file_path = %file_path, "Writing file");

        let path = std::path::Path::new(file_path);

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
                tracing::debug!(
                    file_path = %file_path,
                    bytes = byte_count,
                    "File written successfully"
                );
                Ok(ToolResult::success(format!(
                    "Successfully wrote {} bytes to '{}'",
                    byte_count, file_path
                )))
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
