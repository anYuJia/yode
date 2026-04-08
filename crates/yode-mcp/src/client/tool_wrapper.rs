use anyhow::Result;
use async_trait::async_trait;
use rmcp::model::CallToolRequestParams;
use rmcp::service::Peer;
use rmcp::RoleClient;
use serde_json::Value;

use yode_tools::tool::{Tool, ToolContext, ToolResult};

/// Wraps an MCP tool as a yode Tool trait implementation.
pub struct McpToolWrapper {
    pub tool_name: String,
    pub original_name: String,
    pub description: String,
    pub input_schema: Value,
    pub server_name: String,
    pub peer: Peer<RoleClient>,
}

#[async_trait]
impl Tool for McpToolWrapper {
    fn name(&self) -> &str {
        &self.tool_name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn parameters_schema(&self) -> Value {
        self.input_schema.clone()
    }

    fn requires_confirmation(&self) -> bool {
        true // All MCP tools require confirmation by default
    }

    async fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolResult> {
        tracing::debug!(
            server = %self.server_name,
            tool = %self.original_name,
            "Calling MCP tool"
        );

        let mut request = CallToolRequestParams::new(self.original_name.clone());
        if let Some(obj) = params.as_object() {
            request = request.with_arguments(obj.clone());
        }

        let result = self.peer.call_tool(request).await;

        match result {
            Ok(call_result) => {
                let mut output = String::new();
                for content in &call_result.content {
                    if let Some(text) = content.as_text() {
                        if !output.is_empty() {
                            output.push('\n');
                        }
                        output.push_str(&text.text);
                    }
                }

                if call_result.is_error.unwrap_or(false) {
                    Ok(ToolResult::error(output))
                } else {
                    Ok(ToolResult::success(output))
                }
            }
            Err(e) => Ok(ToolResult::error(format!("MCP tool call failed: {}", e))),
        }
    }
}
