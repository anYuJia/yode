use std::sync::Arc;

use rmcp::handler::server::ServerHandler;
use rmcp::model::*;
use rmcp::service::RequestContext;
use rmcp::{ErrorData as McpError, RoleServer};
use serde_json::Value;
use tracing::info;

use yode_tools::registry::ToolRegistry;
use yode_tools::tool::ToolContext;

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
        let capabilities = ServerCapabilities::builder()
            .enable_tools()
            .build();
        ServerInfo::new(capabilities)
            .with_server_info(Implementation::new("yode", env!("CARGO_PKG_VERSION")))
            .with_instructions("Yode AI coding assistant - exposes built-in file, search, and shell tools via MCP.")
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        let definitions = self.registry.definitions();
        let tools: Vec<rmcp::model::Tool> = definitions
            .into_iter()
            .map(|td| {
                let input_schema: Arc<JsonObject> = serde_json::from_value(td.parameters)
                    .unwrap_or_default();
                rmcp::model::Tool::new(td.name, td.description, input_schema)
            })
            .collect();

        Ok(ListToolsResult::with_all_items(tools))
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
            Ok(result) => {
                if result.is_error {
                    Ok(CallToolResult::error(vec![Content::text(result.content)]))
                } else {
                    Ok(CallToolResult::success(vec![Content::text(result.content)]))
                }
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Tool execution error: {}",
                e
            ))])),
        }
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
