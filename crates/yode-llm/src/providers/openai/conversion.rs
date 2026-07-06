use tracing::warn;

use crate::types::{Message, Role, ToolCall, ToolDefinition, Usage};
use serde_json::{json, Value};

use super::types::{
    OpenAiFunction, OpenAiMessage, OpenAiTool, OpenAiToolCall, OpenAiToolFunction, OpenAiUsage,
};

fn role_to_string(role: &Role) -> String {
    match role {
        Role::System => "system".to_string(),
        Role::User => "user".to_string(),
        Role::Assistant => "assistant".to_string(),
        Role::Tool => "tool".to_string(),
    }
}

fn string_to_role(s: &str) -> Role {
    match s {
        "system" => Role::System,
        "user" => Role::User,
        "assistant" => Role::Assistant,
        "tool" => Role::Tool,
        other => {
            warn!("Unknown role '{}', defaulting to User", other);
            Role::User
        }
    }
}

pub(super) fn message_to_openai(msg: &Message) -> OpenAiMessage {
    let tool_calls = if msg.tool_calls.is_empty() {
        None
    } else {
        Some(
            msg.tool_calls
                .iter()
                .map(|tc| OpenAiToolCall {
                    id: Some(tc.id.clone()),
                    call_type: Some("function".to_string()),
                    function: OpenAiFunction {
                        name: Some(tc.name.clone()),
                        arguments: Some(tc.arguments.clone()),
                    },
                    index: None,
                })
                .collect(),
        )
    };

    OpenAiMessage {
        role: role_to_string(&msg.role),
        content: openai_content_from_message(msg),
        reasoning_content: msg.reasoning.clone(),
        tool_calls,
        tool_call_id: msg.tool_call_id.clone(),
    }
}

pub(super) fn openai_message_to_internal(msg: &OpenAiMessage) -> Message {
    let content = openai_content_text(&msg.content);
    let tool_calls = msg
        .tool_calls
        .as_ref()
        .map(|tcs| {
            tcs.iter()
                .enumerate()
                .map(|(index, tc)| ToolCall {
                    id: openai_tool_call_id(tc, index),
                    name: openai_tool_call_name(tc, index),
                    arguments: openai_tool_call_arguments(tc, index),
                })
                .collect()
        })
        .unwrap_or_default();

    let mut blocks = Vec::new();
    if let Some(ref r) = msg.reasoning_content {
        blocks.push(crate::types::ContentBlock::Thinking {
            thinking: r.clone(),
            signature: None,
        });
    }
    if let Some(ref t) = content {
        blocks.push(crate::types::ContentBlock::Text { text: t.clone() });
    }

    Message {
        role: string_to_role(&msg.role),
        content,
        content_blocks: blocks,
        reasoning: msg.reasoning_content.clone(),
        tool_calls,
        tool_call_id: msg.tool_call_id.clone(),
        images: Vec::new(),
    }
    .normalized()
}

fn openai_tool_call_id(tc: &OpenAiToolCall, index: usize) -> String {
    match tc.id.as_deref().filter(|id| !id.trim().is_empty()) {
        Some(id) => id.to_string(),
        None => {
            let fallback = format!("openai_tool_call_{index}");
            warn!(
                index,
                fallback, "OpenAI tool call is missing id; using stable fallback"
            );
            fallback
        }
    }
}

fn openai_tool_call_name(tc: &OpenAiToolCall, index: usize) -> String {
    match tc
        .function
        .name
        .as_deref()
        .filter(|name| !name.trim().is_empty())
    {
        Some(name) => name.to_string(),
        None => {
            warn!(index, "OpenAI tool call is missing function name");
            "unknown_tool".to_string()
        }
    }
}

fn openai_tool_call_arguments(tc: &OpenAiToolCall, index: usize) -> String {
    match tc
        .function
        .arguments
        .as_deref()
        .filter(|arguments| !arguments.trim().is_empty())
    {
        Some(arguments) => arguments.to_string(),
        None => {
            warn!(index, "OpenAI tool call is missing function arguments");
            "{}".to_string()
        }
    }
}

pub(super) fn openai_content_text(content: &Option<Value>) -> Option<String> {
    match content {
        Some(Value::String(text)) => Some(text.clone()),
        Some(Value::Array(parts)) => {
            let text = parts
                .iter()
                .filter_map(|part| part.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("");
            (!text.is_empty()).then_some(text)
        }
        _ => None,
    }
}

fn openai_content_from_message(msg: &Message) -> Option<Value> {
    if msg.images.is_empty() {
        return msg.content.as_ref().map(|content| json!(content));
    }

    let mut parts = Vec::new();
    if let Some(content) = msg.content.as_ref().filter(|content| !content.is_empty()) {
        parts.push(json!({
            "type": "text",
            "text": content,
        }));
    }
    for image in &msg.images {
        parts.push(json!({
            "type": "image_url",
            "image_url": {
                "url": format!("data:{};base64,{}", image.media_type, image.base64),
            },
        }));
    }
    Some(Value::Array(parts))
}

pub(super) fn tool_to_openai(tool: &ToolDefinition) -> OpenAiTool {
    OpenAiTool {
        tool_type: "function".to_string(),
        function: OpenAiToolFunction {
            name: tool.name.clone(),
            description: tool.description.clone(),
            parameters: sanitize_openai_tool_parameters(tool.parameters.clone()),
        },
    }
}

fn sanitize_openai_tool_parameters(value: Value) -> Value {
    let mut value = sanitize_openai_schema(value, true);
    if !matches!(value.get("type").and_then(Value::as_str), Some("object")) {
        value = json!({
            "type": "object",
            "properties": {},
            "required": []
        });
    }
    if let Value::Object(map) = &mut value {
        strip_openai_unsupported_schema_keywords(map);
        map.entry("properties".to_string())
            .or_insert_with(|| Value::Object(Default::default()));
        map.entry("required".to_string())
            .or_insert_with(|| Value::Array(Vec::new()));
    }
    value
}

fn sanitize_openai_schema(mut value: Value, _is_root: bool) -> Value {
    match &mut value {
        Value::Object(map) => {
            strip_openai_unsupported_schema_keywords(map);
            map.retain(|_, child| !child.is_null());
            for child in map.values_mut() {
                *child = sanitize_openai_schema(std::mem::take(child), false);
            }
            if map.get("type").and_then(Value::as_str) == Some("object") {
                map.entry("properties".to_string())
                    .or_insert_with(|| Value::Object(Default::default()));
                map.entry("required".to_string())
                    .or_insert_with(|| Value::Array(Vec::new()));
            }
        }
        Value::Array(items) => {
            for item in items {
                *item = sanitize_openai_schema(std::mem::take(item), false);
            }
        }
        _ => {}
    }
    value
}

fn strip_openai_unsupported_schema_keywords(map: &mut serde_json::Map<String, Value>) {
    for key in ["oneOf", "anyOf", "allOf", "enum", "const", "not"] {
        map.remove(key);
    }
}

pub(super) fn openai_usage_to_usage(usage: &OpenAiUsage) -> Usage {
    Usage {
        prompt_tokens: usage.prompt_tokens,
        completion_tokens: usage.completion_tokens,
        total_tokens: usage.total_tokens,
        cache_write_tokens: 0,
        cache_read_tokens: usage
            .prompt_tokens_details
            .as_ref()
            .map(|details| details.cached_tokens)
            .unwrap_or(0),
        cache_deleted_tokens: 0,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use crate::types::{ImageData, Message, ToolDefinition};

    use super::{
        message_to_openai, openai_content_text, openai_message_to_internal, tool_to_openai,
    };
    use crate::providers::openai::types::{OpenAiFunction, OpenAiMessage, OpenAiToolCall};

    #[test]
    fn message_to_openai_preserves_user_images_as_content_parts() {
        let message = Message::user_with_images(
            "inspect",
            vec![ImageData {
                base64: "ZmFrZQ==".to_string(),
                media_type: "image/png".to_string(),
            }],
        );

        let converted = message_to_openai(&message);
        let content = converted.content.unwrap();
        let parts = content.as_array().unwrap();
        assert_eq!(parts[0]["type"], "text");
        assert_eq!(parts[0]["text"], "inspect");
        assert_eq!(parts[1]["type"], "image_url");
        assert_eq!(
            parts[1]["image_url"]["url"],
            "data:image/png;base64,ZmFrZQ=="
        );
    }

    #[test]
    fn openai_message_to_internal_uses_stable_tool_call_fallbacks() {
        let message = OpenAiMessage {
            role: "assistant".to_string(),
            content: None,
            reasoning_content: None,
            tool_calls: Some(vec![OpenAiToolCall {
                id: None,
                call_type: Some("function".to_string()),
                function: OpenAiFunction {
                    name: None,
                    arguments: None,
                },
                index: None,
            }]),
            tool_call_id: None,
        };

        let converted = openai_message_to_internal(&message);

        assert_eq!(converted.tool_calls.len(), 1);
        assert_eq!(converted.tool_calls[0].id, "openai_tool_call_0");
        assert_eq!(converted.tool_calls[0].name, "unknown_tool");
        assert_eq!(converted.tool_calls[0].arguments, "{}");
    }

    #[test]
    fn tool_to_openai_adds_empty_required_for_object_schemas() {
        let tool = ToolDefinition {
            name: "project_map".to_string(),
            description: "map".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "depth": {
                        "type": "integer",
                        "description": "depth"
                    }
                }
            }),
            annotations: Default::default(),
        };

        let converted = tool_to_openai(&tool);
        assert_eq!(
            converted.function.parameters.get("required"),
            Some(&Value::Array(Vec::new()))
        );
    }

    #[test]
    fn tool_to_openai_removes_top_level_combination_keywords() {
        let tool = ToolDefinition {
            name: "team_run_ready".to_string(),
            description: "run ready team steps".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "team_id": { "type": "string" },
                    "team_name": { "type": "string" }
                },
                "allOf": [
                    { "anyOf": [{ "required": ["team_id"] }, { "required": ["team_name"] }] }
                ]
            }),
            annotations: Default::default(),
        };

        let parameters = tool_to_openai(&tool).function.parameters;
        assert_eq!(parameters["type"], "object");
        assert!(parameters.get("allOf").is_none());
        assert!(parameters.get("anyOf").is_none());
        assert!(parameters.get("properties").is_some());
        assert_eq!(parameters.get("required"), Some(&Value::Array(Vec::new())));
    }

    #[test]
    fn tool_to_openai_replaces_non_object_root_schema() {
        let tool = ToolDefinition {
            name: "bad_root".to_string(),
            description: "bad root".to_string(),
            parameters: json!({
                "oneOf": [
                    { "type": "object", "properties": { "path": { "type": "string" } } },
                    { "type": "object", "properties": { "url": { "type": "string" } } }
                ]
            }),
            annotations: Default::default(),
        };

        let parameters = tool_to_openai(&tool).function.parameters;
        assert_eq!(parameters["type"], "object");
        assert_eq!(parameters["properties"], json!({}));
        assert_eq!(parameters["required"], json!([]));
        assert!(parameters.get("oneOf").is_none());
    }

    #[test]
    fn openai_content_text_extracts_text_parts() {
        let content = Some(serde_json::json!([
            {"type":"text","text":"hello"},
            {"type":"image_url","image_url":{"url":"data:image/png;base64,xx"}},
            {"type":"text","text":" world"}
        ]));
        assert_eq!(
            openai_content_text(&content).as_deref(),
            Some("hello world")
        );
        assert_eq!(
            openai_content_text(&Some(Value::String("plain".to_string()))).as_deref(),
            Some("plain")
        );
    }
}
