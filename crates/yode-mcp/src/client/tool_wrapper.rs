use anyhow::Result;
use async_trait::async_trait;
use rmcp::model::CallToolRequestParams;
use rmcp::service::Peer;
use rmcp::RoleClient;
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::{LazyLock, Mutex};
use std::time::Instant;

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

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct McpToolLatencyEntry {
    pub server: String,
    pub tool: String,
    pub calls: u64,
    pub errors: u64,
    pub avg_ms: u64,
    pub max_ms: u64,
    pub last_ms: u64,
}

#[derive(Debug, Default)]
struct McpToolLatencyState {
    entries: BTreeMap<(String, String), McpToolLatencyEntry>,
}

static MCP_TOOL_LATENCY: LazyLock<Mutex<McpToolLatencyState>> =
    LazyLock::new(|| Mutex::new(McpToolLatencyState::default()));

pub fn mcp_tool_latency_stats() -> Vec<McpToolLatencyEntry> {
    MCP_TOOL_LATENCY
        .lock()
        .map(|state| state.entries.values().cloned().collect())
        .unwrap_or_default()
}

fn record_mcp_tool_latency(server: &str, tool: &str, duration_ms: u64, is_error: bool) {
    if let Ok(mut state) = MCP_TOOL_LATENCY.lock() {
        let key = (server.to_string(), tool.to_string());
        let entry = state
            .entries
            .entry(key.clone())
            .or_insert_with(|| McpToolLatencyEntry {
                server: key.0.clone(),
                tool: key.1.clone(),
                ..McpToolLatencyEntry::default()
            });
        let total_before = entry.avg_ms.saturating_mul(entry.calls as u64);
        entry.calls = entry.calls.saturating_add(1);
        if is_error {
            entry.errors = entry.errors.saturating_add(1);
        }
        entry.last_ms = duration_ms;
        entry.max_ms = entry.max_ms.max(duration_ms);
        entry.avg_ms = total_before
            .saturating_add(duration_ms)
            .checked_div(entry.calls)
            .unwrap_or(duration_ms);
    }
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
        let started_at = Instant::now();

        let mut request = CallToolRequestParams::new(self.original_name.clone());
        if let Some(obj) = params.as_object() {
            request = request.with_arguments(obj.clone());
        }

        let result = self.peer.call_tool(request).await;
        let duration_ms = started_at.elapsed().as_millis() as u64;

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
                    record_mcp_tool_latency(
                        &self.server_name,
                        &self.original_name,
                        duration_ms,
                        true,
                    );
                    Ok(ToolResult::error(output))
                } else {
                    record_mcp_tool_latency(
                        &self.server_name,
                        &self.original_name,
                        duration_ms,
                        false,
                    );
                    Ok(ToolResult::success(output))
                }
            }
            Err(e) => {
                record_mcp_tool_latency(&self.server_name, &self.original_name, duration_ms, true);
                Ok(ToolResult::error(format!("MCP tool call failed: {}", e)))
            }
        }
    }
}

#[cfg(test)]
pub(crate) fn reset_mcp_tool_latency_stats() {
    if let Ok(mut state) = MCP_TOOL_LATENCY.lock() {
        *state = McpToolLatencyState::default();
    }
}

#[cfg(test)]
mod tests {
    use super::{mcp_tool_latency_stats, record_mcp_tool_latency, reset_mcp_tool_latency_stats};

    #[test]
    fn records_mcp_tool_latency_aggregates() {
        reset_mcp_tool_latency_stats();
        record_mcp_tool_latency("github", "list_prs", 12, false);
        record_mcp_tool_latency("github", "list_prs", 30, true);

        let stats = mcp_tool_latency_stats();
        assert_eq!(stats.len(), 1);
        assert_eq!(stats[0].calls, 2);
        assert_eq!(stats[0].errors, 1);
        assert_eq!(stats[0].max_ms, 30);
        assert_eq!(stats[0].last_ms, 30);
    }
}
