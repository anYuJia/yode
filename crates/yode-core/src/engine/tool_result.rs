use serde_json::{json, Map, Value};

use yode_llm::types::{ToolAnnotations as LlmToolAnnotations, ToolDefinition as LlmToolDefinition};
use yode_tools::registry::{ToolPoolSnapshot, ToolRegistry};
use yode_tools::tool::ToolResult;

use crate::tool_runtime::ToolResultTruncationView;

/// Maximum size for a single tool result before truncation.
const MAX_TOOL_RESULT_SIZE: usize = 50 * 1024;
const MIN_DYNAMIC_TOOL_RESULT_SIZE: usize = 8 * 1024;
const MAX_DYNAMIC_TOTAL_TOOL_RESULTS_SIZE: usize = 200 * 1024;
const MIN_DYNAMIC_TOTAL_TOOL_RESULTS_SIZE: usize = 32 * 1024;

pub(super) fn dynamic_single_tool_result_limit(context_window_tokens: usize) -> usize {
    let estimated_context_bytes = context_window_tokens.saturating_mul(4);
    let budget = estimated_context_bytes / 40;
    budget.clamp(MIN_DYNAMIC_TOOL_RESULT_SIZE, MAX_TOOL_RESULT_SIZE)
}

pub(super) fn dynamic_total_tool_results_limit(context_window_tokens: usize) -> usize {
    let estimated_context_bytes = context_window_tokens.saturating_mul(4);
    let budget = estimated_context_bytes / 10;
    budget.clamp(
        MIN_DYNAMIC_TOTAL_TOOL_RESULTS_SIZE,
        MAX_DYNAMIC_TOTAL_TOOL_RESULTS_SIZE,
    )
}

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
            description: format!(
                "{}\n\nAlways include `action_narrative`: a Simplified Chinese public process note shown before this tool runs. It must be model-written user-facing narration, not hidden reasoning, not a tool title, and not a fixed template. Match Codex's reasoning-summary style: one natural paragraph, usually 1-3 complete sentences, calm and concrete about what you are checking and why. Good: “我看截图里这些句子像‘过程旁白’，夹在‘已思考/已执行工具/已探索’之间。我会确认它们在代码里是怎么生成和渲染的，看是固定模板、工具 metadata，还是模型返回的字段。”",
                td.description
            ),
            parameters: with_action_narrative_parameter(td.parameters),
            annotations: LlmToolAnnotations {
                read_only_hint: td.annotations.read_only_hint,
                destructive_hint: td.annotations.destructive_hint,
                open_world_hint: td.annotations.open_world_hint,
            },
        })
        .collect()
}

fn with_action_narrative_parameter(mut schema: Value) -> Value {
    let Some(object) = schema.as_object_mut() else {
        return schema;
    };
    let properties = object
        .entry("properties".to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    let Some(properties) = properties.as_object_mut() else {
        return schema;
    };
    properties.insert(
        "action_narrative".to_string(),
        json!({
            "type": "string",
            "minLength": 12,
            "maxLength": 260,
            "description": "Required public process note shown before this tool runs. Write a natural Simplified Chinese Codex-style paragraph, usually 1-3 complete sentences, about the immediate visible work: what evidence/files/output you are about to inspect and what distinction you are trying to confirm. Good: “我看截图里这些句子像‘过程旁白’，夹在‘已思考/已执行工具/已探索’之间。我会确认它们在代码里是怎么生成和渲染的，看是固定模板、工具 metadata，还是模型返回的字段。” Bad: terse tool titles like “查看 README”, vague filler like “分析项目结构”, hidden reasoning, English self-talk, or repeated fixed templates."
        }),
    );
    match object.get_mut("required") {
        Some(Value::Array(required)) => {
            if !required
                .iter()
                .any(|value| value.as_str() == Some("action_narrative"))
            {
                required.push(Value::String("action_narrative".to_string()));
            }
        }
        _ => {
            object.insert(
                "required".to_string(),
                Value::Array(vec![Value::String("action_narrative".to_string())]),
            );
        }
    }
    schema
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

/// Truncate tool result if it exceeds the context-aware size limit.
pub(super) fn truncate_tool_result(result: ToolResult, context_window_tokens: usize) -> ToolResult {
    let max_tool_result_size = dynamic_single_tool_result_limit(context_window_tokens);
    if result.content.len() > max_tool_result_size {
        let original_len = result.content.len();
        let head_size = max_tool_result_size * 3 / 4;
        let tail_size = max_tool_result_size / 4;
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

#[cfg(test)]
mod tests {
    use super::{
        dynamic_single_tool_result_limit, dynamic_total_tool_results_limit,
        with_action_narrative_parameter,
    };
    use serde_json::json;

    #[test]
    fn dynamic_tool_result_budgets_scale_with_context_window() {
        assert_eq!(dynamic_single_tool_result_limit(16_385), 8 * 1024);
        assert_eq!(dynamic_total_tool_results_limit(16_385), 32 * 1024);

        assert_eq!(dynamic_single_tool_result_limit(200_000), 20_000);
        assert_eq!(dynamic_total_tool_results_limit(200_000), 80_000);

        assert_eq!(dynamic_total_tool_results_limit(1_000_000), 200 * 1024);
    }

    #[test]
    fn action_narrative_is_required_public_preamble_in_tool_schema() {
        let schema = with_action_narrative_parameter(json!({
            "type": "object",
            "properties": {
                "file_path": { "type": "string" }
            },
            "required": ["file_path"]
        }));

        assert!(schema
            .get("properties")
            .and_then(|properties| properties.get("action_narrative"))
            .is_some());
        assert_eq!(
            schema
                .get("properties")
                .and_then(|properties| properties.get("action_narrative"))
                .and_then(|schema| schema.get("minLength"))
                .and_then(|value| value.as_u64()),
            Some(12)
        );
        assert_eq!(
            schema
                .get("properties")
                .and_then(|properties| properties.get("action_narrative"))
                .and_then(|schema| schema.get("maxLength"))
                .and_then(|value| value.as_u64()),
            Some(260)
        );
        assert!(schema
            .get("required")
            .and_then(|required| required.as_array())
            .is_some_and(|required| required
                .iter()
                .any(|value| value.as_str() == Some("action_narrative"))));
    }
}
