use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};

use crate::tool::{Tool, ToolCapabilities, ToolContext, ToolResult};

pub struct SnipTool;

#[async_trait]
impl Tool for SnipTool {
    fn name(&self) -> &str {
        "snip"
    }

    fn user_facing_name(&self) -> &str {
        "" 
    }

    fn activity_description(&self, params: &Value) -> String {
        let path = params.get("file_path").and_then(|v| v.as_str()).unwrap_or("");
        format!("Snipping file: {}", path)
    }

    fn description(&self) -> &str {
        "Extract a specific snippet from a file to use as context. Similar to read_file but optimized for creating focused snapshots."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Absolute path to the file to snip"
                },
                "start_line": {
                    "type": "integer",
                    "description": "1-based line number to start from"
                },
                "end_line": {
                    "type": "integer",
                    "description": "1-based line number to end at (inclusive)"
                }
            },
            "required": ["file_path", "start_line", "end_line"]
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
        let file_path = params.get("file_path").and_then(|v| v.as_str()).unwrap_or("");
        let start_line = params.get("start_line").and_then(|v| v.as_u64()).unwrap_or(1) as usize;
        let end_line = params.get("end_line").and_then(|v| v.as_u64()).unwrap_or(1) as usize;

        if file_path.is_empty() {
            return Ok(ToolResult::error("file_path is required".to_string()));
        }

        // Update read history (snip counts as read)
        if let Some(history) = &ctx.read_file_history {
            let mut h = history.lock().await;
            h.insert(std::path::PathBuf::from(file_path));
        }

        let content = tokio::fs::read_to_string(file_path).await?;
        let lines: Vec<&str> = content.lines().collect();
        
        let start = start_line.saturating_sub(1);
        let end = end_line.min(lines.len());
        
        if start >= lines.len() || start > end {
            return Ok(ToolResult::error(format!("Line range {}-{} is out of bounds (file has {} lines).", start_line, end_line, lines.len())));
        }

        let snippet = lines[start..end].join("\n");
        let output = format!(
            "[Snippet from {} (lines {}-{})]\n\n{}",
            file_path, start_line, end_line, snippet
        );

        Ok(ToolResult::success(output))
    }
}
