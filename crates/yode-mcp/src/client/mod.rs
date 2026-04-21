mod tool_wrapper;
use self::tool_wrapper::wrapper_tool_name;

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, LazyLock, Mutex};

use anyhow::Result;
use rmcp::model::CallToolRequestParams;
use rmcp::service::{Peer, RunningService, ServiceExt};
use rmcp::transport::{ConfigureCommandExt, TokioChildProcess};
use rmcp::RoleClient;
use tokio::process::Command;
use tracing::info;

use crate::config::McpServerConfig;
use yode_tools::registry::ToolRegistry;
use yode_tools::tool::Tool;

pub use tool_wrapper::{mcp_tool_latency_stats, McpToolLatencyEntry, McpToolWrapper};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct McpReconnectDiagnostic {
    pub server: String,
    pub attempts: u64,
    pub failures: u64,
    pub last_error: Option<String>,
    pub next_backoff_secs: u64,
}

static MCP_RECONNECT_DIAGNOSTICS: LazyLock<Mutex<BTreeMap<String, McpReconnectDiagnostic>>> =
    LazyLock::new(|| Mutex::new(BTreeMap::new()));

pub fn mcp_reconnect_diagnostics() -> Vec<McpReconnectDiagnostic> {
    MCP_RECONNECT_DIAGNOSTICS
        .lock()
        .map(|state| state.values().cloned().collect())
        .unwrap_or_default()
}

fn reconnect_backoff_secs(failure_count: u64) -> u64 {
    match failure_count {
        0 => 0,
        1 => 1,
        2 => 2,
        3 => 4,
        4 => 8,
        _ => 16,
    }
}

fn record_mcp_connect_result(server: &str, success: bool, error: Option<String>) {
    if let Ok(mut state) = MCP_RECONNECT_DIAGNOSTICS.lock() {
        let entry = state
            .entry(server.to_string())
            .or_insert_with(|| McpReconnectDiagnostic {
                server: server.to_string(),
                ..McpReconnectDiagnostic::default()
            });
        entry.attempts = entry.attempts.saturating_add(1);
        if success {
            entry.last_error = None;
            entry.next_backoff_secs = 0;
        } else {
            entry.failures = entry.failures.saturating_add(1);
            entry.last_error = error;
            entry.next_backoff_secs = reconnect_backoff_secs(entry.failures);
        }
    }
}

/// A connected MCP client managing one external server.
pub struct McpClient {
    pub server_name: String,
    peer: Peer<RoleClient>,
    service: RunningService<RoleClient, ()>,
}

impl McpClient {
    /// Connect to an MCP server via stdio transport.
    pub async fn connect(name: &str, config: &McpServerConfig) -> Result<Self> {
        info!(server = %name, command = %config.command, "Connecting to MCP server");

        let env_vars: HashMap<String, String> = config
            .env
            .iter()
            .map(|(k, v)| {
                // Expand $ENV_VAR references in values
                let expanded = if let Some(stripped) = v.strip_prefix('$') {
                    std::env::var(stripped).unwrap_or_default()
                } else {
                    v.clone()
                };
                (k.clone(), expanded)
            })
            .collect();

        let args = config.args.clone();
        let command = config.command.clone();

        let service = match ()
            .serve(TokioChildProcess::new(Command::new(&command).configure(
                |cmd| {
                    cmd.args(&args);
                    for (k, v) in &env_vars {
                        cmd.env(k, v);
                    }
                },
            ))?)
            .await
        {
            Ok(service) => {
                record_mcp_connect_result(name, true, None);
                service
            }
            Err(err) => {
                record_mcp_connect_result(name, false, Some(err.to_string()));
                return Err(err.into());
            }
        };

        let peer_info = service.peer_info();
        if let Some(info) = peer_info {
            info!(
                server = %name,
                server_name = %info.server_info.name,
                "MCP server connected"
            );
        } else {
            info!(server = %name, "MCP server connected (no peer info)");
        }

        let peer = service.peer().clone();

        Ok(Self {
            server_name: name.to_string(),
            peer,
            service,
        })
    }

    /// Discover tools from the connected server and register them as wrapped Tool implementations.
    pub async fn discover_and_register(&self, registry: &mut ToolRegistry) -> Result<usize> {
        let wrappers = self.discover_wrapped_tools().await?;
        let count = wrappers.len();
        for wrapper in wrappers {
            registry.register(wrapper);
        }
        Ok(count)
    }

    pub async fn discover_wrapped_tools(&self) -> Result<Vec<Arc<dyn Tool>>> {
        let tools_result = self.peer.list_tools(Default::default()).await?;
        let tools = tools_result.tools;
        let count = tools.len();

        info!(
            server = %self.server_name,
            tool_count = count,
            "Discovered MCP tools"
        );

        let mut wrappers: Vec<Arc<dyn Tool>> = Vec::with_capacity(count);
        for tool in tools {
            let wrapper = McpToolWrapper {
                tool_name: wrapper_tool_name(&self.server_name, &tool.name),
                original_name: tool.name.to_string(),
                description: tool
                    .description
                    .clone()
                    .map(|c| c.to_string())
                    .unwrap_or_default(),
                input_schema: serde_json::to_value(&tool.input_schema).unwrap_or_default(),
                server_name: self.server_name.clone(),
                peer: self.peer.clone(),
            };
            wrappers.push(Arc::new(wrapper));
        }

        Ok(wrappers)
    }

    /// Call a tool on this MCP server.
    pub async fn call_tool(&self, tool_name: &str, arguments: serde_json::Value) -> Result<String> {
        let mut request = CallToolRequestParams::new(tool_name.to_string());
        if let Some(obj) = arguments.as_object() {
            request = request.with_arguments(obj.clone());
        }

        let result = self.peer.call_tool(request).await?;

        // Extract text content from the result
        let mut output = String::new();
        for content in &result.content {
            if let Some(text) = content.as_text() {
                if !output.is_empty() {
                    output.push('\n');
                }
                output.push_str(&text.text);
            }
        }

        Ok(output)
    }

    /// Gracefully shut down the connection.
    pub async fn shutdown(self) -> Result<()> {
        info!(server = %self.server_name, "Shutting down MCP client");
        self.service.cancel().await?;
        Ok(())
    }

    /// List resources available on this MCP server.
    pub async fn list_resources(&self) -> Result<Vec<(String, String, Option<String>)>> {
        let result = self.peer.list_resources(Default::default()).await?;
        let resources = result
            .resources
            .iter()
            .map(|r| {
                let name = r.name.clone();
                let uri = r.uri.clone();
                let description = r.description.clone();
                (name, uri, description)
            })
            .collect();
        Ok(resources)
    }

    /// Read a specific resource by URI.
    pub async fn read_resource(&self, uri: &str) -> Result<String> {
        let params = rmcp::model::ReadResourceRequestParams::new(uri);
        let result = self.peer.read_resource(params).await?;

        let mut output = String::new();
        for content in &result.contents {
            match content {
                rmcp::model::ResourceContents::TextResourceContents { text, .. } => {
                    if !output.is_empty() {
                        output.push('\n');
                    }
                    output.push_str(text);
                }
                rmcp::model::ResourceContents::BlobResourceContents { blob, .. } => {
                    if !output.is_empty() {
                        output.push('\n');
                    }
                    output.push_str(&format!("[binary blob: {} bytes]", blob.len()));
                }
            }
        }
        Ok(output)
    }
}

#[cfg(test)]
pub(crate) fn reset_mcp_reconnect_diagnostics() {
    if let Ok(mut state) = MCP_RECONNECT_DIAGNOSTICS.lock() {
        state.clear();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use rmcp::model::Tool;
    use serde_json::json;

    use super::{
        mcp_reconnect_diagnostics, record_mcp_connect_result, reset_mcp_reconnect_diagnostics,
    };
    use super::tool_wrapper::wrapper_tool_name;

    #[test]
    fn reconnect_diagnostics_track_failures_and_backoff() {
        reset_mcp_reconnect_diagnostics();
        record_mcp_connect_result("github", false, Some("timeout".to_string()));
        record_mcp_connect_result("github", false, Some("timeout".to_string()));
        record_mcp_connect_result("github", true, None);

        let stats = mcp_reconnect_diagnostics();
        assert_eq!(stats.len(), 1);
        assert_eq!(stats[0].attempts, 3);
        assert_eq!(stats[0].failures, 2);
        assert_eq!(stats[0].next_backoff_secs, 0);
        assert_eq!(stats[0].last_error, None);
    }

    #[test]
    fn wrapper_name_matches_discovery_shape() {
        let input_schema: Arc<rmcp::model::JsonObject> =
            serde_json::from_value(json!({"type":"object"})).unwrap();
        let tool = Tool::new("search_issues", "desc", input_schema);
        assert_eq!(
            wrapper_tool_name("github", &tool.name),
            "mcp__github_search_issues"
        );
    }
}
