use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;

use crate::registry::ToolPoolPhase;
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
        let query = params.get("query").and_then(|v| v.as_str()).unwrap_or("");

        let max_results = params
            .get("max_results")
            .and_then(|v| v.as_u64())
            .unwrap_or(5) as usize;

        if query.is_empty() {
            return Ok(ToolResult::error(
                "'query' parameter is required".to_string(),
            ));
        }

        let registry = ctx
            .registry
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Tool registry not available"))?;
        let tool_pool = ctx.tool_pool_snapshot.as_ref();

        let query_lower = query.to_lowercase();
        let mut matches = Vec::new();

        // Handle "select:" prefix
        if let Some(tool_name) = query_lower.strip_prefix("select:") {
            let req_name = tool_name.trim();
            if let Some(snapshot) = tool_pool {
                if let Some(entry) = snapshot.find_entry(req_name) {
                    if !entry.visible_to_model {
                        let metadata = serde_json::json!({
                            "query": query,
                            "count": 0,
                            "blocked": true,
                            "tool": entry.name,
                            "permission_mode": snapshot.permission_mode,
                            "reason": entry.reason,
                            "matched_rule": entry.matched_rule,
                        });
                        return Ok(ToolResult::success_with_metadata(
                            format!(
                                "Tool '{}' is registered but unavailable in the current tool pool (mode: {}). Reason: {}",
                                entry.name, snapshot.permission_mode, entry.reason
                            ),
                            metadata,
                        ));
                    }

                    if let Some(tool) = registry.get(&entry.name) {
                        matches.push(match entry.phase {
                            ToolPoolPhase::Active => {
                                format!("- **{}**: {}", tool.name(), tool.description())
                            }
                            ToolPoolPhase::Deferred => {
                                format!("- **{}** (deferred): {}", entry.name, tool.description())
                            }
                        });
                    }
                }
            } else {
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
                            matches.push(format!(
                                "- **{}** (deferred): {}",
                                name,
                                tool.description()
                            ));
                            break;
                        }
                    }
                }
            }
        } else {
            if let Some(snapshot) = tool_pool {
                for entry in &snapshot.entries {
                    if !entry.visible_to_model {
                        continue;
                    }
                    let Some(tool) = registry.get(&entry.name) else {
                        continue;
                    };
                    let name = entry.name.to_lowercase();
                    let desc = tool.description().to_lowercase();
                    if name.contains(&query_lower) || desc.contains(&query_lower) {
                        matches.push(match entry.phase {
                            ToolPoolPhase::Active => {
                                format!("- **{}**: {}", tool.name(), tool.description())
                            }
                            ToolPoolPhase::Deferred => {
                                format!("- **{}** (deferred): {}", entry.name, tool.description())
                            }
                        });
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
        }

        matches.truncate(max_results);

        if matches.is_empty() {
            let metadata = serde_json::json!({
                "query": query,
                "count": 0,
                "permission_mode": tool_pool.map(|snapshot| snapshot.permission_mode.as_str()),
                "hidden_count": tool_pool.map(|snapshot| snapshot.deny_count()),
            });
            Ok(ToolResult::success_with_metadata(
                format!("No tools found matching '{}'", query),
                metadata,
            ))
        } else {
            let metadata = serde_json::json!({
                "query": query,
                "count": matches.len(),
                "permission_mode": tool_pool.map(|snapshot| snapshot.permission_mode.as_str()),
                "hidden_count": tool_pool.map(|snapshot| snapshot.deny_count()),
            });
            Ok(ToolResult::success_with_metadata(
                format!(
                    "Found tool(s) matching '{}':\n{}",
                    query,
                    matches.join("\n")
                ),
                metadata,
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::registry::{
        ToolOrigin, ToolPermissionState, ToolPoolEntry, ToolPoolPhase, ToolPoolSnapshot,
    };
    use serde_json::json;

    struct DummyTool {
        name: &'static str,
        description: &'static str,
    }

    #[async_trait]
    impl Tool for DummyTool {
        fn name(&self) -> &str {
            self.name
        }

        fn description(&self) -> &str {
            self.description
        }

        fn parameters_schema(&self) -> Value {
            json!({"type": "object", "properties": {}})
        }

        fn capabilities(&self) -> ToolCapabilities {
            ToolCapabilities {
                requires_confirmation: false,
                supports_auto_execution: true,
                read_only: true,
            }
        }

        async fn execute(&self, _params: Value, _ctx: &ToolContext) -> Result<ToolResult> {
            Ok(ToolResult::success("ok".to_string()))
        }
    }

    fn test_context() -> ToolContext {
        let mut registry = crate::registry::ToolRegistry::new();
        registry.register(Arc::new(DummyTool {
            name: "read_file",
            description: "Read repo files",
        }));
        registry.register_deferred(Arc::new(DummyTool {
            name: "write_file",
            description: "Write repo files",
        }));

        let mut ctx = ToolContext::empty();
        ctx.registry = Some(Arc::new(registry));
        ctx.tool_pool_snapshot = Some(ToolPoolSnapshot {
            permission_mode: "plan".to_string(),
            tool_search_enabled: true,
            entries: vec![
                ToolPoolEntry {
                    name: "read_file".to_string(),
                    phase: ToolPoolPhase::Active,
                    origin: ToolOrigin::Builtin,
                    permission: ToolPermissionState::Allow,
                    visible_to_model: true,
                    reason: "Plan mode allows this read-only tool.".to_string(),
                    matched_rule: None,
                },
                ToolPoolEntry {
                    name: "write_file".to_string(),
                    phase: ToolPoolPhase::Deferred,
                    origin: ToolOrigin::Builtin,
                    permission: ToolPermissionState::Deny,
                    visible_to_model: false,
                    reason: "Plan mode blocks mutating tools.".to_string(),
                    matched_rule: None,
                },
            ],
        });
        ctx
    }

    #[tokio::test]
    async fn tool_search_hides_denied_tools_from_keyword_results() {
        let tool = ToolSearchTool;
        let result = tool
            .execute(json!({ "query": "file" }), &test_context())
            .await
            .unwrap();

        assert!(!result.is_error);
        assert!(result.content.contains("read_file"));
        assert!(!result.content.contains("write_file"));
    }

    #[tokio::test]
    async fn tool_search_reports_blocked_select_for_hidden_tool() {
        let tool = ToolSearchTool;
        let result = tool
            .execute(json!({ "query": "select:write_file" }), &test_context())
            .await
            .unwrap();

        assert!(!result.is_error);
        assert!(result
            .content
            .contains("unavailable in the current tool pool"));
        assert_eq!(
            result
                .metadata
                .as_ref()
                .and_then(|value| value.get("blocked"))
                .and_then(|value| value.as_bool()),
            Some(true)
        );
    }
}
