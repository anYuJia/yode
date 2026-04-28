use crate::providers::streaming_shared::{emit_tool_call_end, emit_tool_call_start};
use crate::types::{
    Message, RestoreSystemBlockHint, Role, StopReason, StreamEvent, ToolCall, ToolDefinition, Usage,
};

use super::types::{
    GeminiContent, GeminiFunctionCall, GeminiFunctionDecl, GeminiFunctionResponse,
    GeminiInlineData, GeminiPart, GeminiResponse, GeminiToolDeclaration, GeminiUsage,
};

pub(super) fn convert_messages(
    messages: &[Message],
    restore_system_blocks: &[RestoreSystemBlockHint],
) -> (Option<GeminiContent>, Vec<GeminiContent>) {
    let mut system_parts = Vec::new();
    let mut contents = Vec::new();

    for message in messages {
        match message.role {
            Role::System => {
                if let Some(text) = &message.content {
                    if !text.is_empty() {
                        system_parts.push(GeminiPart::Text { text: text.clone() });
                    }
                }
            }
            Role::User => {
                let mut parts = Vec::new();
                if let Some(text) = &message.content {
                    if !text.is_empty() {
                        parts.push(GeminiPart::Text { text: text.clone() });
                    }
                }
                for image in &message.images {
                    parts.push(GeminiPart::InlineData {
                        inline_data: GeminiInlineData {
                            mime_type: image.media_type.clone(),
                            data: image.base64.clone(),
                        },
                    });
                }
                if !parts.is_empty() {
                    contents.push(GeminiContent {
                        role: Some("user".into()),
                        parts,
                    });
                }
            }
            Role::Assistant => {
                let mut parts = Vec::new();
                if let Some(text) = &message.content {
                    if !text.is_empty() {
                        parts.push(GeminiPart::Text { text: text.clone() });
                    }
                }
                for tool_call in &message.tool_calls {
                    let args: serde_json::Value = serde_json::from_str(&tool_call.arguments)
                        .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
                    parts.push(GeminiPart::FunctionCall {
                        function_call: GeminiFunctionCall {
                            name: tool_call.name.clone(),
                            args,
                        },
                    });
                }
                if !parts.is_empty() {
                    contents.push(GeminiContent {
                        role: Some("model".into()),
                        parts,
                    });
                }
            }
            Role::Tool => {
                if let Some(text) = &message.content {
                    let name = message
                        .tool_call_id
                        .as_deref()
                        .unwrap_or("function")
                        .to_string();
                    contents.push(GeminiContent {
                        role: Some("user".into()),
                        parts: vec![GeminiPart::FunctionResponse {
                            function_response: GeminiFunctionResponse {
                                name,
                                response: serde_json::json!({ "result": text }),
                            },
                        }],
                    });
                }
            }
        }
    }

    for block in restore_system_blocks
        .iter()
        .filter(|block| !block.content.is_empty())
    {
        system_parts.push(GeminiPart::Text {
            text: format!("[Post-compact restore: {}]\n{}", block.kind, block.content),
        });
    }

    let system = (!system_parts.is_empty()).then_some(GeminiContent {
        role: None,
        parts: system_parts,
    });

    (system, contents)
}

pub(super) fn convert_tools(tools: &[ToolDefinition]) -> Vec<GeminiToolDeclaration> {
    if tools.is_empty() {
        return vec![];
    }

    vec![GeminiToolDeclaration {
        function_declarations: tools
            .iter()
            .map(|tool| GeminiFunctionDecl {
                name: tool.name.clone(),
                description: tool.description.clone(),
                parameters: tool.parameters.clone(),
            })
            .collect(),
    }]
}

pub(super) fn parse_response(resp: &GeminiResponse) -> (Message, Usage, Option<StopReason>) {
    let mut text = String::new();
    let mut tool_calls = Vec::new();
    let mut tool_call_counter = 0u32;

    if let Some(candidates) = &resp.candidates {
        if let Some(candidate) = candidates.first() {
            if let Some(content) = &candidate.content {
                for part in &content.parts {
                    match part {
                        GeminiPart::Text { text: part_text } => text.push_str(part_text),
                        GeminiPart::FunctionCall { function_call } => {
                            tool_call_counter += 1;
                            tool_calls.push(ToolCall {
                                id: format!("gemini_tc_{}", tool_call_counter),
                                name: function_call.name.clone(),
                                arguments: serde_json::to_string(&function_call.args)
                                    .unwrap_or_default(),
                            });
                        }
                        GeminiPart::FunctionResponse { .. } | GeminiPart::InlineData { .. } => {}
                    }
                }
            }
        }
    }

    let usage = resp
        .usage_metadata
        .as_ref()
        .map(gemini_usage_to_usage)
        .unwrap_or_default();

    let stop_reason = resp
        .candidates
        .as_ref()
        .and_then(|candidates| candidates.first())
        .and_then(|candidate| candidate.finish_reason.as_deref())
        .map(map_gemini_finish_reason)
        .or_else(|| (!tool_calls.is_empty()).then_some(StopReason::ToolUse));

    (assistant_message(text, tool_calls), usage, stop_reason)
}

pub(super) fn map_gemini_finish_reason(reason: &str) -> StopReason {
    match reason {
        "STOP" => StopReason::EndTurn,
        "MAX_TOKENS" => StopReason::MaxTokens,
        "SAFETY" | "RECITATION" | "BLOCKLIST" | "PROHIBITED_CONTENT" | "SPII" => {
            StopReason::ContentFilter
        }
        "MALFORMED_FUNCTION_CALL" => StopReason::ToolUse,
        other => StopReason::Other(other.to_string()),
    }
}

pub(super) fn gemini_usage_to_usage(usage: &GeminiUsage) -> Usage {
    Usage {
        prompt_tokens: usage.prompt_token_count,
        completion_tokens: usage.candidates_token_count,
        total_tokens: usage.total_token_count,
        cache_write_tokens: 0,
        cache_read_tokens: usage.cached_content_token_count,
        cache_deleted_tokens: 0,
    }
}

pub(super) async fn send_tool_call_events(
    tx: &tokio::sync::mpsc::Sender<StreamEvent>,
    tool_call: &ToolCall,
) {
    emit_tool_call_start(tx, tool_call.id.clone(), tool_call.name.clone()).await;
    let _ = tx
        .send(StreamEvent::ToolCallDelta {
            id: tool_call.id.clone(),
            arguments: tool_call.arguments.clone(),
        })
        .await;
    emit_tool_call_end(tx, tool_call.id.clone()).await;
}

pub(super) fn assistant_message(text: String, tool_calls: Vec<ToolCall>) -> Message {
    Message::assistant_with_reasoning_and_tools(
        if text.is_empty() { None } else { Some(text) },
        None,
        tool_calls,
    )
}

#[cfg(test)]
mod tests {
    use crate::types::{ImageData, Message, RestoreSystemBlockHint, StopReason};

    use super::{convert_messages, map_gemini_finish_reason, GeminiPart};

    #[test]
    fn gemini_conversion_preserves_multiple_system_messages() {
        let messages = vec![
            Message::system("base system"),
            Message::system("[Context summary] compacted"),
            Message::system("[Post-compact restore: files]\n- src/main.rs"),
            Message::user("resume"),
        ];

        let (system, contents) = convert_messages(&messages, &[]);
        let system = system.expect("system instruction");

        assert_eq!(contents.len(), 1);
        assert_eq!(system.parts.len(), 3);
        assert!(matches!(
            &system.parts[0],
            GeminiPart::Text { text } if text == "base system"
        ));
        assert!(matches!(
            &system.parts[1],
            GeminiPart::Text { text } if text == "[Context summary] compacted"
        ));
        assert!(matches!(
            &system.parts[2],
            GeminiPart::Text { text } if text.starts_with("[Post-compact restore: files]")
        ));
    }

    #[test]
    fn gemini_conversion_appends_restore_blocks_from_provider_hints() {
        let messages = vec![Message::system("base system"), Message::user("resume")];
        let (system, _contents) = convert_messages(
            &messages,
            &[
                RestoreSystemBlockHint {
                    kind: "runtime".to_string(),
                    content: "- Runtime cwd: /tmp".to_string(),
                },
                RestoreSystemBlockHint {
                    kind: "files".to_string(),
                    content: "- Recent files read: src/main.rs".to_string(),
                },
            ],
        );
        let system = system.expect("system instruction");

        assert_eq!(system.parts.len(), 3);
        assert!(matches!(
            &system.parts[1],
            GeminiPart::Text { text } if text.starts_with("[Post-compact restore: runtime]")
        ));
        assert!(matches!(
            &system.parts[2],
            GeminiPart::Text { text } if text.starts_with("[Post-compact restore: files]")
        ));
    }

    #[test]
    fn gemini_conversion_preserves_user_images() {
        let messages = vec![Message::user_with_images(
            "look",
            vec![ImageData {
                base64: "ZmFrZQ==".to_string(),
                media_type: "image/png".to_string(),
            }],
        )];
        let (_system, contents) = convert_messages(&messages, &[]);
        assert_eq!(contents.len(), 1);
        assert!(matches!(
            &contents[0].parts[0],
            GeminiPart::Text { text } if text == "look"
        ));
        assert!(matches!(
            &contents[0].parts[1],
            GeminiPart::InlineData { inline_data }
                if inline_data.mime_type == "image/png" && inline_data.data == "ZmFrZQ=="
        ));
    }

    #[test]
    fn gemini_finish_reason_maps_to_common_stop_reason() {
        assert_eq!(map_gemini_finish_reason("STOP"), StopReason::EndTurn);
        assert_eq!(
            map_gemini_finish_reason("MAX_TOKENS"),
            StopReason::MaxTokens
        );
        assert_eq!(
            map_gemini_finish_reason("SAFETY"),
            StopReason::ContentFilter
        );
        assert_eq!(
            map_gemini_finish_reason("MALFORMED_FUNCTION_CALL"),
            StopReason::ToolUse
        );
    }
}
