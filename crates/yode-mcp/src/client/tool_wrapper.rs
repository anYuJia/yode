use anyhow::Result;
use async_trait::async_trait;
use rmcp::model::CallToolRequestParams;
use serde_json::Value;
use std::collections::BTreeMap;
use std::future::Future;
use std::sync::{LazyLock, Mutex};
use std::time::Instant;

use yode_tools::tool::{Tool, ToolAnnotations, ToolCapabilities, ToolContext, ToolResult};

use super::McpConnection;

/// Wraps an MCP tool as a yode Tool trait implementation.
pub struct McpToolWrapper {
    pub tool_name: String,
    pub original_name: String,
    pub description: String,
    pub input_schema: Value,
    pub annotations: ToolAnnotations,
    pub server_name: String,
    pub(crate) connection: McpConnection,
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

#[derive(Debug, Clone, Default)]
struct McpToolLatencyAccumulator {
    entry: McpToolLatencyEntry,
    total_ms: u128,
}

#[derive(Debug, Default)]
struct McpToolLatencyState {
    entries: BTreeMap<(String, String), McpToolLatencyAccumulator>,
}

static MCP_TOOL_LATENCY: LazyLock<Mutex<McpToolLatencyState>> =
    LazyLock::new(|| Mutex::new(McpToolLatencyState::default()));
#[cfg(test)]
static MCP_TOOL_TEST_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

pub fn mcp_tool_latency_stats() -> Vec<McpToolLatencyEntry> {
    MCP_TOOL_LATENCY
        .lock()
        .map(|state| {
            state
                .entries
                .values()
                .map(|accumulator| accumulator.entry.clone())
                .collect()
        })
        .unwrap_or_default()
}

fn record_mcp_tool_latency(server: &str, tool: &str, duration_ms: u64, is_error: bool) {
    if let Ok(mut state) = MCP_TOOL_LATENCY.lock() {
        let key = (server.to_string(), tool.to_string());
        let accumulator =
            state
                .entries
                .entry(key.clone())
                .or_insert_with(|| McpToolLatencyAccumulator {
                    entry: McpToolLatencyEntry {
                        server: key.0.clone(),
                        tool: key.1.clone(),
                        ..McpToolLatencyEntry::default()
                    },
                    total_ms: 0,
                });
        let entry = &mut accumulator.entry;
        entry.calls = entry.calls.saturating_add(1);
        if is_error {
            entry.errors = entry.errors.saturating_add(1);
        }
        entry.last_ms = duration_ms;
        entry.max_ms = entry.max_ms.max(duration_ms);
        accumulator.total_ms = accumulator.total_ms.saturating_add(duration_ms as u128);
        entry.avg_ms = (accumulator.total_ms / entry.calls as u128).min(u64::MAX as u128) as u64;
    }
}

pub(crate) fn wrapper_tool_name(server_name: &str, tool_name: &str) -> String {
    format!("mcp__{}_{}", server_name, tool_name)
}

pub(crate) fn annotations_from_mcp(
    annotations: Option<&rmcp::model::ToolAnnotations>,
) -> ToolAnnotations {
    annotations
        .map(|annotations| ToolAnnotations {
            read_only_hint: annotations.read_only_hint.unwrap_or(false),
            destructive_hint: annotations.destructive_hint.unwrap_or(true),
            open_world_hint: annotations.open_world_hint.unwrap_or(true),
        })
        .unwrap_or_else(|| {
            ToolCapabilities {
                requires_confirmation: true,
                supports_auto_execution: false,
                read_only: false,
            }
            .into()
        })
}

pub(crate) fn extract_text_content(call_result: &rmcp::model::CallToolResult) -> String {
    let mut output = String::new();
    for content in &call_result.content {
        if let Some(text) = content.as_text() {
            if !output.is_empty() {
                output.push('\n');
            }
            output.push_str(&text.text);
        }
    }
    output
}

pub(crate) fn build_call_request(original_name: &str, params: Value) -> CallToolRequestParams {
    let mut request = CallToolRequestParams::new(original_name.to_string());
    if let Some(obj) = params.as_object() {
        request = request.with_arguments(obj.clone());
    }
    request
}

fn map_call_result(
    server_name: &str,
    original_name: &str,
    duration_ms: u64,
    result: Result<rmcp::model::CallToolResult, String>,
) -> ToolResult {
    match result {
        Ok(call_result) => {
            let output = extract_text_content(&call_result);
            if call_result.is_error.unwrap_or(false) {
                record_mcp_tool_latency(server_name, original_name, duration_ms, true);
                ToolResult::error(output)
            } else {
                record_mcp_tool_latency(server_name, original_name, duration_ms, false);
                ToolResult::success(output)
            }
        }
        Err(error) => {
            record_mcp_tool_latency(server_name, original_name, duration_ms, true);
            ToolResult::error(format!("MCP tool call failed: {}", error))
        }
    }
}

async fn execute_with_caller<F, Fut>(
    server_name: &str,
    original_name: &str,
    params: Value,
    caller: F,
) -> ToolResult
where
    F: FnOnce(CallToolRequestParams) -> Fut,
    Fut: Future<Output = Result<rmcp::model::CallToolResult, String>>,
{
    let started_at = Instant::now();
    let request = build_call_request(original_name, params);
    let result = caller(request).await;
    let duration_ms = started_at.elapsed().as_millis() as u64;
    map_call_result(server_name, original_name, duration_ms, result)
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

    fn capabilities(&self) -> ToolCapabilities {
        if self.annotations.read_only_hint {
            return ToolCapabilities {
                requires_confirmation: false,
                supports_auto_execution: true,
                read_only: true,
            };
        }

        ToolCapabilities {
            requires_confirmation: self.annotations.destructive_hint
                || self.annotations.open_world_hint,
            supports_auto_execution: !self.annotations.destructive_hint
                && !self.annotations.open_world_hint,
            read_only: false,
        }
    }

    fn definition(&self) -> yode_tools::tool::ToolDefinition {
        yode_tools::tool::ToolDefinition {
            name: self.name().to_string(),
            description: self.description().to_string(),
            parameters: self.parameters_schema(),
            annotations: self.annotations,
        }
    }

    async fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolResult> {
        tracing::debug!(
            server = %self.server_name,
            tool = %self.original_name,
            "Calling MCP tool"
        );
        Ok(execute_with_caller(
            &self.server_name,
            &self.original_name,
            params,
            |request| async move {
                self.connection
                    .call_tool(request)
                    .await
                    .map_err(|e| e.to_string())
            },
        )
        .await)
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
    use rmcp::model::{CallToolResult, Content};

    use super::{
        annotations_from_mcp, build_call_request, execute_with_caller, extract_text_content,
        map_call_result, mcp_tool_latency_stats, record_mcp_tool_latency,
        reset_mcp_tool_latency_stats, wrapper_tool_name, MCP_TOOL_TEST_LOCK,
    };

    #[test]
    fn records_mcp_tool_latency_aggregates() {
        let _guard = MCP_TOOL_TEST_LOCK.lock().unwrap();
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

    #[test]
    fn latency_average_uses_wide_accumulator() {
        let _guard = MCP_TOOL_TEST_LOCK.lock().unwrap();
        reset_mcp_tool_latency_stats();
        record_mcp_tool_latency("github", "slow", u64::MAX - 10, false);
        record_mcp_tool_latency("github", "slow", u64::MAX - 8, false);

        let stats = mcp_tool_latency_stats();
        assert_eq!(stats.len(), 1);
        assert_eq!(stats[0].calls, 2);
        assert_eq!(stats[0].avg_ms, u64::MAX - 9);
        assert_eq!(stats[0].max_ms, u64::MAX - 8);
    }

    #[test]
    fn wrapper_tool_name_is_namespaced_by_server() {
        assert_eq!(
            wrapper_tool_name("github", "list_prs"),
            "mcp__github_list_prs"
        );
    }

    #[test]
    fn maps_mcp_annotations_to_yode_annotations() {
        let annotations = rmcp::model::ToolAnnotations::new()
            .read_only(true)
            .destructive(false)
            .open_world(false);

        let mapped = annotations_from_mcp(Some(&annotations));
        assert!(mapped.read_only_hint);
        assert!(!mapped.destructive_hint);
        assert!(!mapped.open_world_hint);
    }

    #[test]
    fn missing_mcp_annotations_remain_conservative() {
        let mapped = annotations_from_mcp(None);
        assert!(!mapped.read_only_hint);
        assert!(mapped.destructive_hint);
        assert!(mapped.open_world_hint);
    }

    #[test]
    fn extract_text_content_joins_multiple_text_blocks() {
        let result = CallToolResult::success(vec![Content::text("first"), Content::text("second")]);
        assert_eq!(extract_text_content(&result), "first\nsecond");
    }

    #[test]
    fn build_call_request_copies_object_arguments() {
        let request = build_call_request(
            "search_issues",
            serde_json::json!({"query":"bugs","limit":3}),
        );
        assert_eq!(request.name.as_ref(), "search_issues");
        let args = request.arguments.unwrap();
        assert_eq!(args["query"], serde_json::json!("bugs"));
        assert_eq!(args["limit"], serde_json::json!(3));
    }

    #[tokio::test]
    async fn execute_with_caller_passes_request_and_maps_success() {
        let _guard = MCP_TOOL_TEST_LOCK.lock().unwrap();
        reset_mcp_tool_latency_stats();

        let result = execute_with_caller(
            "github",
            "list_prs",
            serde_json::json!({"state":"open"}),
            |request| async move {
                assert_eq!(request.name.as_ref(), "list_prs");
                assert_eq!(
                    request.arguments.unwrap()["state"],
                    serde_json::json!("open")
                );
                Ok(CallToolResult::success(vec![Content::text("done")]))
            },
        )
        .await;

        assert!(!result.is_error);
        assert_eq!(result.content, "done");
        let stats = mcp_tool_latency_stats();
        assert_eq!(stats.len(), 1);
        assert_eq!(stats[0].calls, 1);
        assert_eq!(stats[0].errors, 0);
    }

    #[test]
    fn map_call_result_handles_error_payloads_and_failures() {
        let _guard = MCP_TOOL_TEST_LOCK.lock().unwrap();
        reset_mcp_tool_latency_stats();

        let tool_error = map_call_result(
            "github",
            "list_prs",
            15,
            Ok(CallToolResult::error(vec![Content::text("bad request")])),
        );
        assert!(tool_error.is_error);
        assert_eq!(tool_error.content, "bad request");

        let call_failure = map_call_result(
            "github",
            "list_prs",
            30,
            Err("transport closed".to_string()),
        );
        assert!(call_failure.is_error);
        assert!(call_failure.content.contains("transport closed"));

        let stats = mcp_tool_latency_stats();
        assert_eq!(stats.len(), 1);
        assert_eq!(stats[0].calls, 2);
        assert_eq!(stats[0].errors, 2);
        assert_eq!(stats[0].max_ms, 30);
    }
}
