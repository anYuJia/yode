use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};

use crate::tool::{Tool, ToolContext, ToolResult};

pub struct BatchTool;

#[async_trait]
impl Tool for BatchTool {
    fn name(&self) -> &str {
        "batch"
    }

    fn user_facing_name(&self) -> &str {
        "Batch"
    }

    fn activity_description(&self, params: &Value) -> String {
        let count = params
            .get("invocations")
            .and_then(|v| v.as_array())
            .map(|a| a.len())
            .unwrap_or(0);
        format!("Executing {} tools in parallel", count)
    }

    fn description(&self) -> &str {
        "Execute multiple tool calls in parallel. Only works with read-only tools (read_file, glob, grep, ls). Returns results for each invocation."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "invocations": {
                    "type": "array",
                    "description": "Array of tool invocations to execute in parallel",
                    "items": {
                        "type": "object",
                        "properties": {
                            "tool_name": {
                                "type": "string",
                                "description": "Name of the tool to invoke"
                            },
                            "params": {
                                "type": "object",
                                "description": "Parameters to pass to the tool"
                            }
                        },
                        "required": ["tool_name", "params"]
                    }
                }
            },
            "required": ["invocations"]
        })
    }

    fn requires_confirmation(&self) -> bool {
        false
    }

    async fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolResult> {
        let registry = match &ctx.registry {
            Some(r) => Arc::clone(r),
            None => {
                return Ok(ToolResult::error(
                    "Tool registry not available for batch execution.".to_string(),
                ));
            }
        };

        let invocations = params
            .get("invocations")
            .and_then(|v| v.as_array())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: invocations"))?;

        if invocations.is_empty() {
            return Ok(ToolResult::error("No invocations provided.".to_string()));
        }

        // Only allow read-only tools
        const ALLOWED_TOOLS: &[&str] = &["read_file", "glob", "grep", "ls"];

        // Validate all invocations first
        for (i, inv) in invocations.iter().enumerate() {
            let tool_name = inv
                .get("tool_name")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Invocation {} missing tool_name", i))?;

            if !ALLOWED_TOOLS.contains(&tool_name) {
                return Ok(ToolResult::error(format!(
                    "Tool '{}' is not allowed in batch mode. Only read-only tools are permitted: {:?}",
                    tool_name, ALLOWED_TOOLS
                )));
            }

            if registry.get(tool_name).is_none() {
                return Ok(ToolResult::error(format!("Unknown tool: '{}'", tool_name)));
            }
        }

        // Execute all invocations in parallel
        let mut handles = Vec::new();

        for (i, inv) in invocations.iter().enumerate() {
            let tool_name = inv.get("tool_name").unwrap().as_str().unwrap().to_string();
            let tool_params = inv.get("params").cloned().unwrap_or_else(|| json!({}));
            let tool = registry.get(&tool_name).unwrap();

            handles.push(tokio::spawn(async move {
                let ctx = ToolContext::empty();
                match tool.execute(tool_params, &ctx).await {
                    Ok(result) => (i, tool_name, result),
                    Err(e) => (i, tool_name, ToolResult::error(format!("Error: {}", e))),
                }
            }));
        }

        let mut results = Vec::new();
        for handle in handles {
            match handle.await {
                Ok((i, name, result)) => {
                    results.push(json!({
                        "index": i,
                        "tool": name,
                        "content": result.content,
                        "is_error": result.is_error,
                    }));
                }
                Err(e) => {
                    results.push(json!({
                        "error": format!("Task join error: {}", e),
                    }));
                }
            }
        }

        // Sort by index
        results.sort_by_key(|r| r.get("index").and_then(|v| v.as_u64()).unwrap_or(0));

        let metadata = json!({
            "invocation_count": invocations.len(),
            "results": results,
        });

        Ok(ToolResult::success_with_metadata(
            serde_json::to_string_pretty(&results).unwrap(),
            metadata,
        ))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use anyhow::Result;
    use async_trait::async_trait;
    use serde_json::{json, Value};

    use crate::registry::ToolRegistry;
    use crate::tool::{Tool, ToolContext, ToolResult};

    use super::BatchTool;

    struct DummyReadTool;

    #[async_trait]
    impl Tool for DummyReadTool {
        fn name(&self) -> &str {
            "read_file"
        }

        fn description(&self) -> &str {
            "dummy read"
        }

        fn parameters_schema(&self) -> Value {
            json!({"type":"object"})
        }

        async fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolResult> {
            Ok(ToolResult::success(
                params.get("value").and_then(|v| v.as_str()).unwrap_or("ok").to_string(),
            ))
        }
    }

    struct DummyMutatingTool;

    #[async_trait]
    impl Tool for DummyMutatingTool {
        fn name(&self) -> &str {
            "write_file"
        }

        fn description(&self) -> &str {
            "dummy write"
        }

        fn parameters_schema(&self) -> Value {
            json!({"type":"object"})
        }

        async fn execute(&self, _params: Value, _ctx: &ToolContext) -> Result<ToolResult> {
            Ok(ToolResult::success("mutated".to_string()))
        }
    }

    #[tokio::test]
    async fn batch_executes_allowed_tools_and_preserves_order() {
        let registry = ToolRegistry::new();
        registry.register(Arc::new(DummyReadTool));

        let mut ctx = ToolContext::empty();
        ctx.registry = Some(Arc::new(registry));

        let result = BatchTool
            .execute(
                json!({
                    "invocations": [
                        {"tool_name":"read_file","params":{"value":"first"}},
                        {"tool_name":"read_file","params":{"value":"second"}}
                    ]
                }),
                &ctx,
            )
            .await
            .unwrap();

        assert!(!result.is_error);
        let results = result.metadata.as_ref().unwrap()["results"].as_array().unwrap();
        assert_eq!(results[0]["content"], json!("first"));
        assert_eq!(results[1]["content"], json!("second"));
    }

    #[tokio::test]
    async fn batch_rejects_non_readonly_tools() {
        let registry = ToolRegistry::new();
        registry.register(Arc::new(DummyMutatingTool));

        let mut ctx = ToolContext::empty();
        ctx.registry = Some(Arc::new(registry));

        let result = BatchTool
            .execute(
                json!({
                    "invocations": [
                        {"tool_name":"write_file","params":{}}
                    ]
                }),
                &ctx,
            )
            .await
            .unwrap();

        assert!(result.is_error);
        assert!(result.content.contains("not allowed in batch mode"));
    }
}
