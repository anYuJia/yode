use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use crate::tool::{Tool, ToolCapabilities, ToolContext, ToolResult};

pub mod auth;
pub use auth::McpAuthTool;

pub struct ListMcpResourcesTool;
pub struct ReadMcpResourceTool;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct McpResourceCacheStats {
    pub list_hits: u64,
    pub list_misses: u64,
    pub read_hits: u64,
    pub read_misses: u64,
    pub cached_list_entries: usize,
    pub cached_read_entries: usize,
}

#[derive(Debug, Default)]
struct McpResourceCacheState {
    lists: HashMap<String, Vec<crate::tool::McpResource>>,
    reads: HashMap<(String, String), String>,
    stats: McpResourceCacheStats,
}

static MCP_RESOURCE_CACHE: LazyLock<Mutex<McpResourceCacheState>> =
    LazyLock::new(|| Mutex::new(McpResourceCacheState::default()));

pub fn mcp_resource_cache_stats() -> McpResourceCacheStats {
    MCP_RESOURCE_CACHE
        .lock()
        .map(|cache| cache.stats.clone())
        .unwrap_or_default()
}

#[async_trait]
impl Tool for ListMcpResourcesTool {
    fn name(&self) -> &str {
        "list_mcp_resources"
    }

    fn user_facing_name(&self) -> &str {
        ""
    }

    fn activity_description(&self, params: &Value) -> String {
        let server = params
            .get("server")
            .and_then(|v| v.as_str())
            .unwrap_or("all servers");
        format!("Listing MCP resources from: {}", server)
    }

    fn description(&self) -> &str {
        "List available resources from configured MCP servers. Use this to find shared context or data provided by servers."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "server": {
                    "type": "string",
                    "description": "Optional server name to filter resources by. Omit to list all."
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
        let cache_key = server.unwrap_or("*").to_string();
        if let Ok(mut cache) = MCP_RESOURCE_CACHE.lock() {
            if let Some(resources) = cache.lists.get(&cache_key).cloned() {
                cache.stats.list_hits = cache.stats.list_hits.saturating_add(1);
                return render_list_resources(resources);
            }
            cache.stats.list_misses = cache.stats.list_misses.saturating_add(1);
        }

        let resources = provider.list_resources(server).await?;
        if let Ok(mut cache) = MCP_RESOURCE_CACHE.lock() {
            cache.lists.insert(cache_key, resources.clone());
            cache.stats.cached_list_entries = cache.lists.len();
        }

        render_list_resources(resources)
    }
}

fn render_list_resources(resources: Vec<crate::tool::McpResource>) -> Result<ToolResult> {
    if resources.is_empty() {
        return Ok(ToolResult::success("No MCP resources found.".to_string()));
    }

    let mut output = String::from("Available MCP resources:\n\n");
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

#[async_trait]
impl Tool for ReadMcpResourceTool {
    fn name(&self) -> &str {
        "read_mcp_resource"
    }

    fn user_facing_name(&self) -> &str {
        ""
    }

    fn aliases(&self) -> Vec<String> {
        vec!["ReadMcpResource".to_string()]
    }

    fn activity_description(&self, params: &Value) -> String {
        let uri = params.get("uri").and_then(|v| v.as_str()).unwrap_or("");
        format!("Reading MCP resource: {}", uri)
    }

    fn description(&self) -> &str {
        "Read a specific resource from an MCP server."
    }

    fn parameters_schema(&self) -> Value {
        json!({
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

        let cache_key = (server.to_string(), uri.to_string());
        if let Ok(mut cache) = MCP_RESOURCE_CACHE.lock() {
            if let Some(content) = cache.reads.get(&cache_key).cloned() {
                cache.stats.read_hits = cache.stats.read_hits.saturating_add(1);
                return Ok(ToolResult::success(content));
            }
            cache.stats.read_misses = cache.stats.read_misses.saturating_add(1);
        }

        let content = provider.read_resource(server, uri).await?;
        if let Ok(mut cache) = MCP_RESOURCE_CACHE.lock() {
            cache.reads.insert(cache_key, content.clone());
            cache.stats.cached_read_entries = cache.reads.len();
        }
        Ok(ToolResult::success(content))
    }
}

#[cfg(test)]
pub(crate) fn reset_mcp_resource_cache() {
    if let Ok(mut cache) = MCP_RESOURCE_CACHE.lock() {
        *cache = McpResourceCacheState::default();
    }
}

#[cfg(test)]
mod tests {
    use super::{
        mcp_resource_cache_stats, reset_mcp_resource_cache, ListMcpResourcesTool,
        ReadMcpResourceTool,
    };
    use crate::tool::{McpResource, McpResourceProvider, Tool, ToolContext};
    use anyhow::Result;
    use serde_json::json;
    use std::pin::Pin;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    struct MockMcpProvider {
        list_calls: AtomicUsize,
        read_calls: AtomicUsize,
    }

    impl MockMcpProvider {
        fn new() -> Self {
            Self {
                list_calls: AtomicUsize::new(0),
                read_calls: AtomicUsize::new(0),
            }
        }
    }

    impl McpResourceProvider for MockMcpProvider {
        fn list_resources(
            &self,
            server: Option<&str>,
        ) -> Pin<Box<dyn std::future::Future<Output = Result<Vec<McpResource>>> + Send + '_>>
        {
            let server = server.unwrap_or("all").to_string();
            self.list_calls.fetch_add(1, Ordering::SeqCst);
            Box::pin(async move {
                Ok(vec![McpResource {
                    server,
                    uri: "mcp://resource".to_string(),
                    name: "resource".to_string(),
                    description: Some("demo".to_string()),
                }])
            })
        }

        fn read_resource(
            &self,
            server: &str,
            uri: &str,
        ) -> Pin<Box<dyn std::future::Future<Output = Result<String>> + Send + '_>> {
            let response = format!("{}:{}", server, uri);
            self.read_calls.fetch_add(1, Ordering::SeqCst);
            Box::pin(async move { Ok(response) })
        }
    }

    #[tokio::test]
    async fn list_mcp_resources_uses_cache_on_repeated_calls() {
        reset_mcp_resource_cache();
        let provider = Arc::new(MockMcpProvider::new());
        let mut ctx = ToolContext::empty();
        ctx.mcp_resources = Some(provider.clone());

        let tool = ListMcpResourcesTool;
        let first = tool.execute(json!({"server": "demo"}), &ctx).await.unwrap();
        let second = tool.execute(json!({"server": "demo"}), &ctx).await.unwrap();

        assert!(!first.is_error);
        assert!(!second.is_error);
        assert_eq!(provider.list_calls.load(Ordering::SeqCst), 1);
        let stats = mcp_resource_cache_stats();
        assert_eq!(stats.list_misses, 1);
        assert_eq!(stats.list_hits, 1);
        assert_eq!(stats.cached_list_entries, 1);
    }

    #[tokio::test]
    async fn read_mcp_resource_uses_cache_on_repeated_calls() {
        reset_mcp_resource_cache();
        let provider = Arc::new(MockMcpProvider::new());
        let mut ctx = ToolContext::empty();
        ctx.mcp_resources = Some(provider.clone());

        let tool = ReadMcpResourceTool;
        let first = tool
            .execute(json!({"server": "demo", "uri": "mcp://resource"}), &ctx)
            .await
            .unwrap();
        let second = tool
            .execute(json!({"server": "demo", "uri": "mcp://resource"}), &ctx)
            .await
            .unwrap();

        assert!(!first.is_error);
        assert!(!second.is_error);
        assert_eq!(provider.read_calls.load(Ordering::SeqCst), 1);
        let stats = mcp_resource_cache_stats();
        assert_eq!(stats.read_misses, 1);
        assert_eq!(stats.read_hits, 1);
        assert_eq!(stats.cached_read_entries, 1);
    }
}
