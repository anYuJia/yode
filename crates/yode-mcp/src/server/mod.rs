use std::sync::Arc;

use rmcp::handler::server::ServerHandler;
use rmcp::model::*;
use rmcp::service::RequestContext;
use rmcp::{ErrorData as McpError, RoleServer};
use serde_json::Value;
use tracing::{info, warn};

use yode_tools::registry::ToolRegistry;
use yode_tools::tool::{ToolAnnotations, ToolContext, ToolDefinition, ToolResult};

/// MCP Server that exposes yode's built-in tools.
#[derive(Clone)]
pub struct YodeMcpServer {
    registry: Arc<ToolRegistry>,
}

impl YodeMcpServer {
    pub fn new(registry: Arc<ToolRegistry>) -> Self {
        Self { registry }
    }
}

impl ServerHandler for YodeMcpServer {
    fn get_info(&self) -> ServerInfo {
        let capabilities = ServerCapabilities::builder().enable_tools().build();
        ServerInfo::new(capabilities)
            .with_server_info(Implementation::new("yode", env!("CARGO_PKG_VERSION")))
            .with_instructions("Yode AI coding assistant - exposes built-in file, search, and shell tools via MCP.")
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        Ok(ListToolsResult::with_all_items(definitions_to_mcp_tools(
            self.registry.definitions(),
        )))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let tool_name = request.name.as_ref();
        let tool = self.registry.get(tool_name).ok_or_else(|| {
            let msg = format!("Unknown tool: {}", tool_name);
            McpError::invalid_params(msg, None)
        })?;

        let params: Value = request
            .arguments
            .map(Value::Object)
            .unwrap_or_else(|| Value::Object(serde_json::Map::new()));

        let ctx = ToolContext::empty();
        match tool.execute(params, &ctx).await {
            Ok(result) => Ok(tool_result_to_call_result(result)),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Tool execution error: {}",
                e
            ))])),
        }
    }
}

fn definitions_to_mcp_tools(definitions: Vec<ToolDefinition>) -> Vec<rmcp::model::Tool> {
    definitions
        .into_iter()
        .map(|td| {
            let input_schema = tool_parameters_to_mcp_schema(&td.name, td.parameters);
            let annotations = annotations_to_mcp(td.annotations);
            rmcp::model::Tool::new(td.name, td.description, input_schema)
                .with_annotations(annotations)
        })
        .collect()
}

fn tool_parameters_to_mcp_schema(tool_name: &str, parameters: Value) -> Arc<JsonObject> {
    match serde_json::from_value(parameters) {
        Ok(schema) => schema,
        Err(err) => {
            warn!(
                tool = %tool_name,
                error = %err,
                "Failed to convert tool parameters to MCP schema"
            );
            Arc::default()
        }
    }
}

fn annotations_to_mcp(annotations: ToolAnnotations) -> rmcp::model::ToolAnnotations {
    rmcp::model::ToolAnnotations::new()
        .read_only(annotations.read_only_hint)
        .destructive(annotations.destructive_hint)
        .open_world(annotations.open_world_hint)
}

fn tool_result_to_call_result(result: ToolResult) -> CallToolResult {
    if result.is_error {
        CallToolResult::error(vec![Content::text(result.content)])
    } else {
        CallToolResult::success(vec![Content::text(result.content)])
    }
}

/// Run yode as an MCP server on stdio.
pub async fn run_mcp_server(registry: Arc<ToolRegistry>) -> anyhow::Result<()> {
    use rmcp::service::ServiceExt;
    use rmcp::transport::io::stdio;

    info!("Starting yode MCP server on stdio");
    let server = YodeMcpServer::new(registry);
    let transport = stdio();
    let service = server.serve(transport).await?;
    service.waiting().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        annotations_to_mcp, definitions_to_mcp_tools, tool_parameters_to_mcp_schema,
        tool_result_to_call_result,
    };
    use yode_tools::tool::{ToolAnnotations, ToolDefinition, ToolResult};

    #[test]
    fn maps_tool_definitions_to_mcp_tools() {
        let tools = definitions_to_mcp_tools(vec![ToolDefinition {
            name: "read_file".to_string(),
            description: "Read a file".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "file_path": { "type": "string" }
                }
            }),
            annotations: ToolAnnotations {
                read_only_hint: true,
                destructive_hint: false,
                open_world_hint: false,
            },
        }]);

        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name.as_ref(), "read_file");
        assert_eq!(tools[0].description.as_deref(), Some("Read a file"));
        assert!(tools[0].input_schema.contains_key("properties"));
        let annotations = tools[0].annotations.as_ref().expect("annotations");
        assert_eq!(annotations.read_only_hint, Some(true));
        assert_eq!(annotations.destructive_hint, Some(false));
        assert_eq!(annotations.open_world_hint, Some(false));
    }

    #[test]
    fn invalid_tool_parameters_fall_back_to_empty_schema() {
        let schema =
            tool_parameters_to_mcp_schema("invalid", serde_json::json!(["not", "an", "object"]));
        assert!(schema.is_empty());
    }

    #[test]
    fn maps_yode_annotations_to_mcp_annotations() {
        let annotations = annotations_to_mcp(ToolAnnotations {
            read_only_hint: false,
            destructive_hint: true,
            open_world_hint: true,
        });

        assert_eq!(annotations.read_only_hint, Some(false));
        assert_eq!(annotations.destructive_hint, Some(true));
        assert_eq!(annotations.open_world_hint, Some(true));
    }

    #[test]
    fn maps_tool_results_to_mcp_success_or_error() {
        let success = tool_result_to_call_result(ToolResult::success("ok".to_string()));
        assert_eq!(success.is_error, Some(false));

        let failure = tool_result_to_call_result(ToolResult::error("bad".to_string()));
        assert_eq!(failure.is_error, Some(true));
    }
}
