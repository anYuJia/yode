use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use std::process::Command;

use crate::tool::{Tool, ToolCapabilities, ToolContext, ToolErrorType, ToolResult};

pub struct FileDiffTool;

#[async_trait]
impl Tool for FileDiffTool {
    fn name(&self) -> &str {
        "file_diff"
    }

    fn description(&self) -> &str {
        "Compare two files using unified diff format. Shows line-by-line differences between file_a and file_b."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "file_a": {
                    "type": "string",
                    "description": "Path to the first file"
                },
                "file_b": {
                    "type": "string",
                    "description": "Path to the second file"
                },
                "context_lines": {
                    "type": "integer",
                    "description": "Number of context lines around changes (default: 3)",
                    "default": 3
                }
            },
            "required": ["file_a", "file_b"]
        })
    }

    fn capabilities(&self) -> ToolCapabilities {
        ToolCapabilities {
            requires_confirmation: false,
            supports_auto_execution: true,
            read_only: true,
        }
    }

    async fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolResult> {
        let file_a = params
            .get("file_a")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: file_a"))?;
        let file_b = params
            .get("file_b")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: file_b"))?;
        let context_lines = params
            .get("context_lines")
            .and_then(|v| v.as_i64())
            .unwrap_or(3);

        // Validate files exist
        if !std::path::Path::new(file_a).exists() {
            return Ok(ToolResult::error_typed(
                format!("File not found: {}", file_a),
                ToolErrorType::NotFound,
                true,
                Some("Check the file path and try again".to_string()),
            ));
        }
        if !std::path::Path::new(file_b).exists() {
            return Ok(ToolResult::error_typed(
                format!("File not found: {}", file_b),
                ToolErrorType::NotFound,
                true,
                Some("Check the file path and try again".to_string()),
            ));
        }

        let output = Command::new("diff")
            .args([
                "-u",
                &format!("--label={}", file_a),
                &format!("--label={}", file_b),
                "-U",
                &context_lines.to_string(),
                file_a,
                file_b,
            ])
            .output()
            .map_err(|e| anyhow::anyhow!("Failed to run diff: {}", e))?;

        let stdout = String::from_utf8_lossy(&output.stdout);

        // diff exits with 0 = identical, 1 = different, 2 = error
        match output.status.code() {
            Some(0) => Ok(ToolResult::success("Files are identical.".to_string())),
            Some(1) => Ok(ToolResult::success(stdout.to_string())),
            _ => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                Ok(ToolResult::error(format!("diff failed: {}", stderr.trim())))
            }
        }
    }
}
