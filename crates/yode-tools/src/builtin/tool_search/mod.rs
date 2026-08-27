use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;

use crate::registry::ToolPoolPhase;
use crate::semantic_router::{SemanticToolMatch, SemanticToolRouter};
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
        format!("Routing tools for: {}", query)
    }

    fn description(&self) -> &str {
        "Semantically route a task intent to the best available tools, including deferred tools. Use 'select:<tool_name>' for an explicit tool selection."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Natural-language intent to route to tools, or select:<tool_name> for explicit selection."
                },
                "max_results": {
                    "type": "integer",
                    "minimum": 1,
                    "maximum": 20,
                    "description": "Maximum number of ranked results (default: 5)"
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
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim();
        let max_results = params
            .get("max_results")
            .and_then(Value::as_u64)
            .unwrap_or(5)
            .clamp(1, 20) as usize;
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
        let query_lower = query.to_ascii_lowercase();

        if let Some(tool_name) = query_lower.strip_prefix("select:") {
            return explicit_select(tool_name.trim(), query, registry, tool_pool).await;
        }

        let router = SemanticToolRouter;
        let mut ranked: Vec<(SemanticToolMatch, ToolPoolPhase, String)> = Vec::new();

        if let Some(snapshot) = tool_pool {
            for entry in &snapshot.entries {
                if !entry.visible_to_model {
                    continue;
                }
                let Some(tool) = registry.get(&entry.name) else {
                    continue;
                };
                let Some(score) = router.score(query, tool.as_ref()) else {
                    continue;
                };
                ranked.push((score, entry.phase, tool.description().to_string()));
            }
        } else {
            for tool in registry.list() {
                if let Some(score) = router.score(query, tool.as_ref()) {
                    ranked.push((score, ToolPoolPhase::Active, tool.description().to_string()));
                }
            }
            for (name, tool) in registry.list_deferred() {
                if let Some(mut score) = router.score(query, tool.as_ref()) {
                    score.name = name;
                    ranked.push((
                        score,
                        ToolPoolPhase::Deferred,
                        tool.description().to_string(),
                    ));
                }
            }
        }

        ranked.sort_by(|left, right| {
            right
                .0
                .score
                .cmp(&left.0.score)
                .then_with(|| left.0.name.cmp(&right.0.name))
        });
        ranked.truncate(max_results);

        if ranked.is_empty() {
            return Ok(ToolResult::success_with_metadata(
                format!("No tools found matching intent '{}'", query),
                serde_json::json!({
                    "query": query,
                    "count": 0,
                    "router": "semantic_v1",
                    "permission_mode": tool_pool.map(|snapshot| snapshot.permission_mode.as_str()),
                    "hidden_count": tool_pool.map(|snapshot| snapshot.deny_count()),
                }),
            ));
        }

        let rendered = ranked
            .iter()
            .map(|(matched, phase, description)| {
                let phase_label = if *phase == ToolPoolPhase::Deferred {
                    ", deferred"
                } else {
                    ""
                };
                let why = if matched.reasons.is_empty() {
                    String::new()
                } else {
                    format!(" — {}", matched.reasons.join("; "))
                };
                format!(
                    "- **{}** (score {}{}): {}{}",
                    matched.name, matched.score, phase_label, description, why
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        let result_meta = ranked
            .iter()
            .map(|(matched, phase, _)| {
                serde_json::json!({
                    "name": matched.name,
                    "score": matched.score,
                    "phase": match phase { ToolPoolPhase::Active => "active", ToolPoolPhase::Deferred => "deferred" },
                    "reasons": matched.reasons,
                })
            })
            .collect::<Vec<_>>();

        Ok(ToolResult::success_with_metadata(
            format!("Best tools for '{}':\n{}", query, rendered),
            serde_json::json!({
                "query": query,
                "count": ranked.len(),
                "router": "semantic_v1",
                "results": result_meta,
                "permission_mode": tool_pool.map(|snapshot| snapshot.permission_mode.as_str()),
                "hidden_count": tool_pool.map(|snapshot| snapshot.deny_count()),
            }),
        ))
    }
}

async fn explicit_select(
    req_name: &str,
    original_query: &str,
    registry: &std::sync::Arc<crate::registry::ToolRegistry>,
    tool_pool: Option<&crate::registry::ToolPoolSnapshot>,
) -> Result<ToolResult> {
    if let Some(snapshot) = tool_pool {
        if let Some(entry) = snapshot.find_entry(req_name) {
            if !entry.visible_to_model {
                return Ok(ToolResult::success_with_metadata(
                    format!(
                        "Tool '{}' is registered but unavailable in the current tool pool (mode: {}). Reason: {}",
                        entry.name, snapshot.permission_mode, entry.reason
                    ),
                    serde_json::json!({
                        "query": original_query,
                        "count": 0,
                        "blocked": true,
                        "tool": entry.name,
                        "permission_mode": snapshot.permission_mode,
                        "reason": entry.reason,
                        "matched_rule": entry.matched_rule,
                        "router": "semantic_v1"
                    }),
                ));
            }
            if let Some(tool) = registry.get(&entry.name) {
                let activated =
                    entry.phase == ToolPoolPhase::Deferred && registry.activate_tool(&entry.name);
                let content = if activated {
                    format!(
                        "Activated tool '{}' into the active pool.\n\n- **{}**: {}",
                        entry.name,
                        entry.name,
                        tool.description()
                    )
                } else {
                    format!("- **{}**: {}", entry.name, tool.description())
                };
                return Ok(ToolResult::success_with_metadata(
                    content,
                    serde_json::json!({
                        "query": original_query,
                        "count": 1,
                        "activated_tool": activated.then_some(entry.name.clone()),
                        "router": "semantic_v1",
                        "permission_mode": snapshot.permission_mode,
                    }),
                ));
            }
        }
    } else {
        for tool in registry.list() {
            if tool.name().eq_ignore_ascii_case(req_name) {
                return Ok(ToolResult::success_with_metadata(
                    format!("- **{}**: {}", tool.name(), tool.description()),
                    serde_json::json!({"query": original_query, "count": 1, "router": "semantic_v1"}),
                ));
            }
        }
        for (name, tool) in registry.list_deferred() {
            if name.eq_ignore_ascii_case(req_name) {
                let activated = registry.activate_tool(&name);
                return Ok(ToolResult::success_with_metadata(
                    format!(
                        "Activated tool '{}' into the active pool.\n\n- **{}**: {}",
                        name,
                        name,
                        tool.description()
                    ),
                    serde_json::json!({
                        "query": original_query,
                        "count": 1,
                        "activated_tool": activated.then_some(name),
                        "router": "semantic_v1"
                    }),
                ));
            }
        }
    }

    Ok(ToolResult::success_with_metadata(
        format!(
            "Tool '{}' was not found in the current tool pool.",
            req_name
        ),
        serde_json::json!({"query": original_query, "count": 0, "router": "semantic_v1"}),
    ))
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
        read_only: bool,
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
            json!({"type":"object","properties":{}})
        }
        fn capabilities(&self) -> ToolCapabilities {
            ToolCapabilities {
                requires_confirmation: !self.read_only,
                supports_auto_execution: self.read_only,
                read_only: self.read_only,
            }
        }
        async fn execute(&self, _params: Value, _ctx: &ToolContext) -> Result<ToolResult> {
            Ok(ToolResult::success("ok".to_string()))
        }
    }

    fn test_context(write_file_visible: bool) -> ToolContext {
        let registry = crate::registry::ToolRegistry::new();
        registry.register(Arc::new(DummyTool {
            name: "read_file",
            description: "Read repo files",
            read_only: true,
        }));
        registry.register_deferred(Arc::new(DummyTool {
            name: "write_file",
            description: "Write repo files",
            read_only: false,
        }));
        registry.register_deferred(Arc::new(DummyTool {
            name: "test_runner",
            description: "Run repository tests and verify changes",
            read_only: false,
        }));
        let mut ctx = ToolContext::empty();
        ctx.registry = Some(Arc::new(registry));
        ctx.tool_pool_snapshot = Some(ToolPoolSnapshot {
            permission_mode: "plan".to_string(),
            tool_search_enabled: true,
            tool_search_reason: Some("test".to_string()),
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
                    permission: if write_file_visible {
                        ToolPermissionState::Allow
                    } else {
                        ToolPermissionState::Deny
                    },
                    visible_to_model: write_file_visible,
                    reason: if write_file_visible {
                        "Loaded by tool_search.".to_string()
                    } else {
                        "Plan mode blocks mutating tools.".to_string()
                    },
                    matched_rule: None,
                },
                ToolPoolEntry {
                    name: "test_runner".to_string(),
                    phase: ToolPoolPhase::Deferred,
                    origin: ToolOrigin::Builtin,
                    permission: ToolPermissionState::Allow,
                    visible_to_model: true,
                    reason: "Verification is allowed.".to_string(),
                    matched_rule: None,
                },
            ],
        });
        ctx
    }

    #[tokio::test]
    async fn tool_search_hides_denied_tools_from_results() {
        let result = ToolSearchTool
            .execute(json!({"query":"file"}), &test_context(false))
            .await
            .unwrap();
        assert!(!result.is_error);
        assert!(result.content.contains("read_file"));
        assert!(!result.content.contains("write_file"));
    }

    #[tokio::test]
    async fn tool_search_reports_blocked_select_for_hidden_tool() {
        let result = ToolSearchTool
            .execute(json!({"query":"select:write_file"}), &test_context(false))
            .await
            .unwrap();
        assert!(result
            .content
            .contains("unavailable in the current tool pool"));
        assert_eq!(
            result
                .metadata
                .as_ref()
                .and_then(|v| v.get("blocked"))
                .and_then(Value::as_bool),
            Some(true)
        );
    }

    #[tokio::test]
    async fn tool_search_select_activates_visible_deferred_tool() {
        let ctx = test_context(true);
        let result = ToolSearchTool
            .execute(json!({"query":"select:write_file"}), &ctx)
            .await
            .unwrap();
        assert!(result.content.contains("Activated tool 'write_file'"));
        assert!(ctx
            .registry
            .as_ref()
            .unwrap()
            .definitions()
            .iter()
            .any(|definition| definition.name == "write_file"));
    }

    #[tokio::test]
    async fn semantic_verification_query_ranks_test_runner() {
        let result = ToolSearchTool
            .execute(
                json!({"query":"verify the fix with tests","max_results":1}),
                &test_context(true),
            )
            .await
            .unwrap();
        assert!(result.content.contains("test_runner"));
        assert_eq!(
            result
                .metadata
                .as_ref()
                .and_then(|v| v.get("router"))
                .and_then(Value::as_str),
            Some("semantic_v1")
        );
    }
}
