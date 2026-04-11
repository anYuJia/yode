use serde_json::{json, Map, Value};

use yode_llm::types::ToolDefinition as LlmToolDefinition;
use yode_tools::registry::{ToolPoolSnapshot, ToolRegistry};
use yode_tools::tool::ToolResult;

use crate::tool_runtime::ToolResultTruncationView;

/// Maximum size for a single tool result before truncation.
const MAX_TOOL_RESULT_SIZE: usize = 50 * 1024;

pub(super) fn convert_tool_definitions(
    registry: &ToolRegistry,
    tool_pool: Option<&ToolPoolSnapshot>,
) -> Vec<LlmToolDefinition> {
    registry
        .definitions()
        .into_iter()
        .filter(|definition| {
            tool_pool
                .map(|snapshot| snapshot.active_visible_to_model(&definition.name))
                .unwrap_or(true)
        })
        .map(|td| LlmToolDefinition {
            name: td.name,
            description: td.description,
            parameters: td.parameters,
        })
        .collect()
}

fn metadata_object(result: &mut ToolResult) -> &mut Map<String, Value> {
    if !matches!(result.metadata, Some(Value::Object(_))) {
        result.metadata = Some(Value::Object(Map::new()));
    }

    match result.metadata {
        Some(Value::Object(ref mut object)) => object,
        _ => unreachable!("tool result metadata must be an object"),
    }
}

fn tool_runtime_object(result: &mut ToolResult) -> &mut Map<String, Value> {
    let metadata = metadata_object(result);
    if !matches!(metadata.get("tool_runtime"), Some(Value::Object(_))) {
        metadata.insert("tool_runtime".to_string(), Value::Object(Map::new()));
    }

    match metadata.get_mut("tool_runtime") {
        Some(Value::Object(object)) => object,
        _ => unreachable!("tool_runtime metadata must be an object"),
    }
}

pub(super) fn set_tool_runtime_truncation_metadata(
    result: &mut ToolResult,
    truncation: &ToolResultTruncationView,
) {
    let runtime = tool_runtime_object(result);
    runtime.insert(
        "truncation".to_string(),
        json!({
            "reason": truncation.reason,
            "original_bytes": truncation.original_bytes,
            "kept_bytes": truncation.kept_bytes,
            "omitted_bytes": truncation.omitted_bytes,
        }),
    );
}

pub(super) fn annotate_tool_result_runtime_metadata(
    result: &mut ToolResult,
    duration_ms: u64,
    progress_updates: u32,
    parallel_batch: Option<u32>,
    input_bytes: usize,
) {
    let output_bytes = result.content.len();
    let error_type = result.error_type.map(|kind| format!("{:?}", kind));
    let runtime = tool_runtime_object(result);
    runtime.insert("duration_ms".to_string(), json!(duration_ms));
    runtime.insert("progress_updates".to_string(), json!(progress_updates));
    runtime.insert("input_bytes".to_string(), json!(input_bytes));
    runtime.insert("output_bytes".to_string(), json!(output_bytes));
    if let Some(batch) = parallel_batch {
        runtime.insert("parallel_batch".to_string(), json!(batch));
    }
    if let Some(error_type) = error_type {
        runtime.insert("error_type".to_string(), json!(error_type));
    }
}

/// Truncate tool result if it exceeds the size limit.
pub(super) fn truncate_tool_result(result: ToolResult) -> ToolResult {
    if result.content.len() > MAX_TOOL_RESULT_SIZE {
        let original_len = result.content.len();
        let head_size = MAX_TOOL_RESULT_SIZE * 3 / 4;
        let tail_size = MAX_TOOL_RESULT_SIZE / 4;
        let head: String = result.content.chars().take(head_size).collect();
        let tail: String = result
            .content
            .chars()
            .rev()
            .take(tail_size)
            .collect::<String>()
            .chars()
            .rev()
            .collect();
        let mut truncated = ToolResult {
            content: format!(
                "{}\n\n... [TRUNCATED: Original {} bytes, content omitted to prevent context overflow. Use search tools (grep/glob) or targeted reads (offset/limit) to inspect the rest] ...\n\n{}",
                head,
                original_len,
                tail
            ),
            is_error: result.is_error,
            error_type: result.error_type,
            recoverable: result.recoverable,
            suggestion: result.suggestion,
            metadata: result.metadata,
        };
        let kept_bytes = truncated.content.len();
        set_tool_runtime_truncation_metadata(
            &mut truncated,
            &ToolResultTruncationView {
                reason: "single_result_limit".to_string(),
                original_bytes: original_len,
                kept_bytes,
                omitted_bytes: original_len.saturating_sub(kept_bytes),
            },
        );
        truncated
    } else {
        result
    }
}
