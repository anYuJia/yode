use crate::types::{
    ChatResponse, ContentBlock, Message, Role, StreamEvent, ToolCall, ToolDefinition, Usage,
};

use super::types::{
    GeminiContent, GeminiFunctionCall, GeminiFunctionDecl, GeminiFunctionResponse, GeminiPart,
    GeminiResponse, GeminiToolDeclaration, GeminiUsage,
};

pub(super) fn convert_messages(
    messages: &[Message],
) -> (Option<GeminiContent>, Vec<GeminiContent>) {
    let mut system = None;
    let mut contents = Vec::new();

    for message in messages {
        match message.role {
            Role::System => {
                if let Some(text) = &message.content {
                    system = Some(GeminiContent {
                        role: None,
                        parts: vec![GeminiPart::Text { text: text.clone() }],
                    });
                }
            }
            Role::User => {
                if let Some(text) = &message.content {
                    contents.push(GeminiContent {
                        role: Some("user".into()),
                        parts: vec![GeminiPart::Text { text: text.clone() }],
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

pub(super) fn parse_response(resp: &GeminiResponse) -> (Message, Usage) {
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
                        GeminiPart::FunctionResponse { .. } => {}
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

    (assistant_message(text, tool_calls), usage)
}

pub(super) fn gemini_usage_to_usage(usage: &GeminiUsage) -> Usage {
    Usage {
        prompt_tokens: usage.prompt_token_count,
        completion_tokens: usage.candidates_token_count,
        total_tokens: usage.total_token_count,
        cache_write_tokens: 0,
        cache_read_tokens: usage.cached_content_token_count,
    }
}

pub(super) async fn send_tool_call_events(
    tx: &tokio::sync::mpsc::Sender<StreamEvent>,
    tool_call: &ToolCall,
) {
    let _ = tx
        .send(StreamEvent::ToolCallStart {
            id: tool_call.id.clone(),
            name: tool_call.name.clone(),
        })
        .await;
    let _ = tx
        .send(StreamEvent::ToolCallDelta {
            id: tool_call.id.clone(),
            arguments: tool_call.arguments.clone(),
        })
        .await;
    let _ = tx
        .send(StreamEvent::ToolCallEnd {
            id: tool_call.id.clone(),
        })
        .await;
}

pub(super) fn done_event(message: Message, usage: Usage, model: String) -> StreamEvent {
    StreamEvent::Done(ChatResponse {
        message,
        usage,
        model,
        stop_reason: None,
    })
}

pub(super) fn assistant_message(text: String, tool_calls: Vec<ToolCall>) -> Message {
    Message {
        role: Role::Assistant,
        content: if text.is_empty() {
            None
        } else {
            Some(text.clone())
        },
        content_blocks: if text.is_empty() {
            vec![]
        } else {
            vec![ContentBlock::Text { text }]
        },
        reasoning: None,
        tool_calls,
        tool_call_id: None,
        images: Vec::new(),
    }
    .normalized()
}
