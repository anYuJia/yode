use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};

use crate::tool::{Tool, ToolCapabilities, ToolContext, ToolResult};

pub struct SendUserFileTool;

#[async_trait]
impl Tool for SendUserFileTool {
    fn name(&self) -> &str {
        "send_user_file"
    }

    fn user_facing_name(&self) -> &str {
        "" 
    }

    fn aliases(&self) -> Vec<String> {
        vec!["SendUserFile".to_string()]
    }

    fn activity_description(&self, params: &Value) -> String {
        let path = params.get("file_path").and_then(|v| v.as_str()).unwrap_or("");
        format!("Sending file to user: {}", path)
    }

    fn description(&self) -> &str {
        "Send a file to the user. Use this when you've generated a file the user should see, or when they've requested a specific document."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Absolute path to the file to send"
                },
                "description": {
                    "type": "string",
                    "description": "A short description of the file and why it's being sent"
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

    async fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolResult> {
        let file_path = params
            .get("file_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("'file_path' is required"))?;

        if !std::path::Path::new(file_path).exists() {
            return Ok(ToolResult::error(format!("File '{}' does not exist.", file_path)));
        }

        Ok(ToolResult::success(format!("File sent to user: {}", file_path)))
    }
}
