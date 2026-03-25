use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;

use crate::tool::{Tool, ToolCapabilities, ToolContext, ToolResult};

pub struct ListMcpResourcesTool;
pub struct ReadMcpResourceTool;

#[async_trait]
impl Tool for ListMcpResourcesTool {
    fn name(&self) -> &str {
        "list_mcp_resources"
    }

    fn description(&self) -> &str {
        "List available resources from configured MCP servers. Each resource includes the server name, URI, name, and description."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "server": {
                    "type": "string",
                    "description": "Optional server name to filter resources by. If omitted, lists resources from all servers."
                }
            }
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
        let provider = ctx
            .mcp_resources
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("MCP resource provider not available"))?;

        let server = params.get("server").and_then(|v| v.as_str());
        let resources = provider.list_resources(server).await?;

        if resources.is_empty() {
            return Ok(ToolResult::success("No MCP resources found.".to_string()));
        }

        let mut output = String::new();
        for resource in &resources {
            output.push_str(&format!(
                "- [{}] {}: {}{}\n",
                resource.server,
                resource.name,
                resource.uri,
                resource
                    .description
                    .as_ref()
                    .map(|d| format!(" - {}", d))
                    .unwrap_or_default()
            ));
        }

        Ok(ToolResult::success(output))
    }
}

#[async_trait]
impl Tool for ReadMcpResourceTool {
    fn name(&self) -> &str {
        "read_mcp_resource"
    }

    fn description(&self) -> &str {
        "Read a specific resource from an MCP server by server name and URI."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "server": {
                    "type": "string",
                    "description": "The MCP server name"
                },
                "uri": {
                    "type": "string",
                    "description": "The resource URI to read"
                }
            },
            "required": ["server", "uri"]
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
        let provider = ctx
            .mcp_resources
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("MCP resource provider not available"))?;

        let server = params
            .get("server")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("'server' parameter is required"))?;
        let uri = params
            .get("uri")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("'uri' parameter is required"))?;

        let content = provider.read_resource(server, uri).await?;
        Ok(ToolResult::success(content))
    }
}
