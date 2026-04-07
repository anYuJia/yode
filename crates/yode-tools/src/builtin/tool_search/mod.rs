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

    fn user_facing_name(&self) -> &str {
        "Tool Search"
    }

    fn activity_description(&self, params: &Value) -> String {
        let query = params.get("query").and_then(|v| v.as_str()).unwrap_or("");
        format!("Searching for tools: {}", query)
    }

    fn description(&self) -> &str {
        "Search for deferred tools by keyword or select them directly. Use 'select:<tool_name>' to forcefully load a tool."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Query to find deferred tools. Use 'select:<tool_name>' for direct selection, or keywords to search."
                },
                "max_results": {
                    "type": "integer",
                    "description": "Maximum number of results to return (default: 5)"
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

        let max_results = params
            .get("max_results")
            .and_then(|v| v.as_u64())
            .unwrap_or(5) as usize;

        if query.is_empty() {
            return Ok(ToolResult::error("'query' parameter is required".to_string()));
        }

        let registry = ctx
            .registry
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Tool registry not available"))?;

        let query_lower = query.to_lowercase();
        let mut matches = Vec::new();

        // Handle "select:" prefix
        if let Some(tool_name) = query_lower.strip_prefix("select:") {
            let req_name = tool_name.trim();
            let mut found = false;
            for tool in registry.list() {
                if tool.name().to_lowercase() == req_name {
                    matches.push(format!("- **{}**: {}", tool.name(), tool.description()));
                    found = true;
                    break;
                }
            }
            if !found {
                for (name, tool) in registry.list_deferred() {
                    if name.to_lowercase() == req_name {
                        matches.push(format!("- **{}** (deferred): {}", name, tool.description()));
                        break;
                    }
                }
            }
        } else {
            // Keyword search
            for tool in registry.list() {
                let name = tool.name().to_lowercase();
                let desc = tool.description().to_lowercase();
                if name.contains(&query_lower) || desc.contains(&query_lower) {
                    matches.push(format!("- **{}**: {}", tool.name(), tool.description()));
                }
            }

            for (name, tool) in registry.list_deferred() {
                let name_lower = name.to_lowercase();
                let desc = tool.description().to_lowercase();
                if name_lower.contains(&query_lower) || desc.contains(&query_lower) {
                    matches.push(format!("- **{}** (deferred): {}", name, tool.description()));
                }
            }
        }
        
        matches.truncate(max_results);

        if matches.is_empty() {
            let metadata = serde_json::json!({ "query": query, "count": 0 });
            Ok(ToolResult::success_with_metadata(format!(
                "No tools found matching '{}'",
                query
            ), metadata))
        } else {
            let metadata = serde_json::json!({ "query": query, "count": matches.len() });
            Ok(ToolResult::success_with_metadata(format!(
                "Found tool(s) matching '{}':\n{}",
                query,
                matches.join("\n")
            ), metadata))
        }
    }
}
