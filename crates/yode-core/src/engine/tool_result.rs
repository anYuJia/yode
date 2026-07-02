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
            name: td.name.clone(),
            description: if td.name == "batch" {
                format!(
                    "{}\n\nInclude `action_narrative`: one brief Simplified Chinese public progress note for this batch as a whole. Write what evidence this batch is collecting or checking. Keep it natural and specific; avoid generic titles such as \"全面分析\" or listing every nested tool.",
                    td.description
                )
            } else {
                td.description
            },
            parameters: if td.name == "batch" {
                with_batch_action_narrative_parameter(td.parameters)
            } else {
                td.parameters
            },
            annotations: LlmToolAnnotations {
                read_only_hint: td.annotations.read_only_hint,
                destructive_hint: td.annotations.destructive_hint,
                open_world_hint: td.annotations.open_world_hint,
            },
        })
        .collect()
}

fn with_batch_action_narrative_parameter(mut schema: Value) -> Value {
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
            "minLength": 8,
            "maxLength": 120,
            "description": "Required public progress note for this batch as a whole. Use natural Simplified Chinese, mention the evidence or area being checked, and avoid generic phase titles or listing each nested tool."
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
    result
        .metadata
        .get_or_insert_with(|| Value::Object(Map::new()));
    result
        .metadata
        .as_mut()
        .and_then(Value::as_object_mut)
        .expect("metadata was initialized as an object")
}

fn tool_runtime_object(result: &mut ToolResult) -> &mut Map<String, Value> {
    let metadata = metadata_object(result);
    metadata
        .entry("tool_runtime".to_string())
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .expect("tool_runtime metadata was initialized as an object")
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

pub(super) fn annotate_tool_result_activity_metadata(
    result: &mut ToolResult,
    tool_name: &str,
    input: &Value,
) {
    let metadata = metadata_object(result);
    if matches!(metadata.get("activity"), Some(Value::Object(_))) {
        return;
    }

    let kind = activity_kind(tool_name, metadata);
    let target = activity_target(tool_name, input, metadata);
    let label = activity_label(kind, &target, tool_name);
    let mut activity = Map::new();
    activity.insert("kind".to_string(), json!(kind));
    activity.insert("label".to_string(), json!(label));
    activity.insert("target".to_string(), json!(target));
    activity.insert("tool".to_string(), json!(tool_name));

    if let Some(command) = command_target(input, metadata) {
        activity.insert("command".to_string(), json!(command));
    }
    if let Some(path) = file_target(input, metadata) {
        activity.insert("file_path".to_string(), json!(path));
    }

    metadata.insert("activity".to_string(), Value::Object(activity));
}

fn activity_kind(tool_name: &str, metadata: &Map<String, Value>) -> &'static str {
    let lower = tool_name.to_ascii_lowercase();
    if lower.contains("write")
        || lower.contains("edit")
        || lower.contains("replace")
        || lower.contains("patch")
        || metadata.get("modified_files").is_some()
        || metadata.get("diff_preview").is_some()
    {
        "edit"
    } else if lower.contains("run")
        || lower.contains("command")
        || lower.contains("bash")
        || lower.contains("shell")
    {
        "run"
    } else if lower.contains("search")
        || lower.contains("grep")
        || lower.contains("url")
        || lower.contains("web")
    {
        "search"
    } else if lower.contains("read")
        || lower.contains("view")
        || lower.contains("list")
        || lower.contains("ls")
        || lower.contains("glob")
        || lower.contains("project_map")
        || lower.contains("resource")
    {
        "read"
    } else {
        "other"
    }
}

fn activity_target(tool_name: &str, input: &Value, metadata: &Map<String, Value>) -> String {
    command_target(input, metadata)
        .or_else(|| file_target(input, metadata))
        .or_else(|| string_field(metadata, &["uri", "server", "pattern", "path", "name"]))
        .or_else(|| {
            string_field(
                input.as_object().unwrap_or(metadata),
                &["query", "tool", "namespace"],
            )
        })
        .unwrap_or_else(|| tool_name.to_string())
}

fn activity_label(kind: &str, target: &str, tool_name: &str) -> String {
    let trimmed = target.trim();
    match kind {
        "edit" => format!("已修改 {}", fallback_target(trimmed, tool_name)),
        "run" => format!("已运行 {}", fallback_target(trimmed, tool_name)),
        "search" => format!("已搜索 {}", fallback_target(trimmed, tool_name)),
        "read" => format!("已读取 {}", fallback_target(trimmed, tool_name)),
        _ => fallback_target(trimmed, tool_name).to_string(),
    }
}

fn fallback_target<'a>(target: &'a str, tool_name: &'a str) -> &'a str {
    if target.is_empty() {
        tool_name
    } else {
        target
    }
}

fn command_target(input: &Value, metadata: &Map<String, Value>) -> Option<String> {
    input
        .as_object()
        .and_then(|object| string_field(object, &["cmd", "command", "CommandLine"]))
        .or_else(|| string_field(metadata, &["cmd", "command", "CommandLine"]))
}

fn file_target(input: &Value, metadata: &Map<String, Value>) -> Option<String> {
    string_field(
        metadata,
        &[
            "file_path",
            "TargetFile",
            "AbsolutePath",
            "Path",
            "SearchPath",
            "TargetContentFile",
            "path",
        ],
    )
    .or_else(|| {
        input.as_object().and_then(|object| {
            string_field(
                object,
                &[
                    "file_path",
                    "TargetFile",
                    "AbsolutePath",
                    "Path",
                    "SearchPath",
                    "TargetContentFile",
                    "path",
                ],
            )
        })
    })
}

fn string_field(object: &Map<String, Value>, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| object.get(*key).and_then(Value::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
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
        annotate_tool_result_activity_metadata, dynamic_single_tool_result_limit,
        dynamic_total_tool_results_limit, with_batch_action_narrative_parameter,
    };
    use serde_json::json;
    use yode_tools::tool::ToolResult;

    #[test]
    fn dynamic_tool_result_budgets_scale_with_context_window() {
        assert_eq!(dynamic_single_tool_result_limit(16_385), 8 * 1024);
        assert_eq!(dynamic_total_tool_results_limit(16_385), 32 * 1024);

        assert_eq!(dynamic_single_tool_result_limit(200_000), 20_000);
        assert_eq!(dynamic_total_tool_results_limit(200_000), 80_000);

        assert_eq!(dynamic_total_tool_results_limit(1_000_000), 200 * 1024);
    }

    #[test]
    fn action_narrative_is_required_only_for_batch_schema() {
        let schema = with_batch_action_narrative_parameter(json!({
            "type": "object",
            "properties": {
                "invocations": { "type": "array" }
            },
            "required": ["invocations"]
        }));

        assert!(schema
            .get("properties")
            .and_then(|properties| properties.get("action_narrative"))
            .is_some());
        assert!(schema
            .get("required")
            .and_then(|required| required.as_array())
            .is_some_and(|required| required
                .iter()
                .any(|value| value.as_str() == Some("action_narrative"))));
        assert!(schema
            .get("required")
            .and_then(|required| required.as_array())
            .is_some_and(|required| required
                .iter()
                .any(|value| value.as_str() == Some("invocations"))));
    }

    #[test]
    fn annotates_activity_metadata_for_command_tools() {
        let mut result = ToolResult::success("ok".to_string());
        annotate_tool_result_activity_metadata(
            &mut result,
            "exec_command",
            &json!({ "cmd": "git status --short" }),
        );

        let activity = result
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("activity"))
            .unwrap();
        assert_eq!(activity["kind"], json!("run"));
        assert_eq!(activity["target"], json!("git status --short"));
        assert_eq!(activity["command"], json!("git status --short"));
        assert_eq!(activity["label"], json!("已运行 git status --short"));
    }

    #[test]
    fn preserves_existing_activity_metadata() {
        let mut result = ToolResult::success_with_metadata(
            "ok".to_string(),
            json!({
                "activity": {
                    "kind": "read",
                    "target": "自定义目标"
                }
            }),
        );
        annotate_tool_result_activity_metadata(
            &mut result,
            "exec_command",
            &json!({ "cmd": "git status --short" }),
        );

        let activity = result
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("activity"))
            .unwrap();
        assert_eq!(activity["kind"], json!("read"));
        assert_eq!(activity["target"], json!("自定义目标"));
        assert!(activity.get("command").is_none());
    }
}
