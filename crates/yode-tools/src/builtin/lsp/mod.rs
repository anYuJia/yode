use std::path::PathBuf;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;

use crate::tool::{Tool, ToolCapabilities, ToolContext, ToolResult};

pub struct LspTool;

#[async_trait]
impl Tool for LspTool {
    fn name(&self) -> &str {
        "lsp"
    }

    fn user_facing_name(&self) -> &str {
        "LSP"
    }

    fn activity_description(&self, params: &Value) -> String {
        let op = params
            .get("operation")
            .and_then(|v| v.as_str())
            .unwrap_or("query");
        let file = params
            .get("filePath")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        format!("LSP {}: {}", op, file)
    }

    fn description(&self) -> &str {
        "Interact with Language Server Protocol (LSP) servers for code intelligence. \
         Supports goToDefinition, findReferences, hover, and documentSymbol operations. \
         LSP servers are started on demand per language."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "operation": {
                    "type": "string",
                    "enum": ["goToDefinition", "findReferences", "hover", "documentSymbol"],
                    "description": "The LSP operation to perform"
                },
                "filePath": {
                    "type": "string",
                    "description": "Absolute path to the file"
                },
                "line": {
                    "type": "integer",
                    "description": "Line number (0-based)"
                },
                "character": {
                    "type": "integer",
                    "description": "Character offset (0-based)"
                }
            },
            "required": ["operation", "filePath", "line", "character"]
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
        let operation = params
            .get("operation")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let file_path = params
            .get("filePath")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let line = params.get("line").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
        let character = params
            .get("character")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32;

        if operation.is_empty() || file_path.is_empty() {
            return Ok(ToolResult::error(
                "operation and filePath are required".to_string(),
            ));
        }

        let lsp_mgr = ctx
            .lsp_manager
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("LSP manager not available"))?;

        let path = PathBuf::from(file_path);
        let mut mgr = lsp_mgr.lock().await;

        match mgr.execute(operation, &path, line, character).await {
            Ok(result) => {
                let formatted =
                    serde_json::to_string_pretty(&result).unwrap_or_else(|_| result.to_string());
                let metadata = serde_json::json!({
                    "operation": operation,
                    "file_path": file_path,
                    "line": line,
                    "character": character,
                });
                Ok(ToolResult::success_with_metadata(formatted, metadata))
            }
            Err(e) => Ok(ToolResult::error(format!("LSP operation failed: {}", e))),
        }
    }
}
