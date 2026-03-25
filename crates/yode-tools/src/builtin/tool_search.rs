use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;

use crate::tool::{Tool, ToolCapabilities, ToolContext, ToolResult};

pub struct ToolSearchTool;

#[async_trait]
impl Tool for ToolSearchTool {
    fn name(&self) -> &str {
        "tool_search"
    }

    fn description(&self) -> &str {
        "Search for available tools by name or description keyword. \
         Use this when you need a tool but aren't sure of its exact name. \
         Returns matching tool names and descriptions."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Search query to match against tool names and descriptions"
                }
            },
            "required": ["query"]
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
        let query = params
            .get("query")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if query.is_empty() {
            return Ok(ToolResult::error("'query' parameter is required".to_string()));
        }

        let registry = ctx
            .registry
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Tool registry not available"))?;

        let query_lower = query.to_lowercase();
        let mut matches = Vec::new();

        // Search active tools
        for tool in registry.list() {
            let name = tool.name().to_lowercase();
            let desc = tool.description().to_lowercase();
            if name.contains(&query_lower) || desc.contains(&query_lower) {
                matches.push(format!("- **{}**: {}", tool.name(), tool.description()));
            }
        }

        // Search deferred tools
        for (name, tool) in registry.list_deferred() {
            let name_lower = name.to_lowercase();
            let desc = tool.description().to_lowercase();
            if name_lower.contains(&query_lower) || desc.contains(&query_lower) {
                matches.push(format!("- **{}** (deferred): {}", name, tool.description()));
            }
        }

        if matches.is_empty() {
            Ok(ToolResult::success(format!(
                "No tools found matching '{}'",
                query
            )))
        } else {
            Ok(ToolResult::success(format!(
                "Found {} tool(s) matching '{}':\n{}",
                matches.len(),
                query,
                matches.join("\n")
            )))
        }
    }
}
