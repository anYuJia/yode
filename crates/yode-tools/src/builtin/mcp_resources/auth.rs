use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};

use crate::tool::{Tool, ToolCapabilities, ToolContext, ToolResult};

pub struct McpAuthTool;

#[async_trait]
impl Tool for McpAuthTool {
    fn name(&self) -> &str {
        "mcp_auth"
    }

    fn aliases(&self) -> Vec<String> {
        vec!["McpAuth".to_string()]
    }

    fn user_facing_name(&self) -> &str {
        ""
    }

    fn activity_description(&self, params: &Value) -> String {
        let server = params.get("server").and_then(|v| v.as_str()).unwrap_or("server");
        format!("Authenticating MCP server: {}", server)
    }

    fn description(&self) -> &str {
        "Start the authentication flow for an MCP server that requires it. \
         This tool returns an authorization URL for the user to visit in their browser."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "server": {
                    "type": "string",
                    "description": "The name of the MCP server to authenticate"
                }
            },
            "required": ["server"]
        })
    }

    fn capabilities(&self) -> ToolCapabilities {
        ToolCapabilities {
            requires_confirmation: false,
            supports_auto_execution: true,
            read_only: false,
        }
    }

    async fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolResult> {
        let server = params.get("server").and_then(|v| v.as_str()).unwrap_or("");
        
        // Mock authentication URL for now
        let auth_url = format!("https://yode.dev/mcp/auth/{}", server);
        
        let msg = format!(
            "To use the '{}' MCP server, please complete authorization in your browser:\n\n{}\n\nOnce completed, the server's tools will become available automatically.",
            server, auth_url
        );

        Ok(ToolResult::success(msg))
    }
}
