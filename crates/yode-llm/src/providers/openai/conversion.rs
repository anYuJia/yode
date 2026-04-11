use super::*;

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
        content: msg.content.clone(),
        reasoning_content: msg.reasoning.clone(),
        tool_calls,
        tool_call_id: msg.tool_call_id.clone(),
    }
}

pub(super) fn openai_message_to_internal(msg: &OpenAiMessage) -> Message {
    let tool_calls = msg
        .tool_calls
        .as_ref()
        .map(|tcs| {
            tcs.iter()
                .map(|tc| ToolCall {
                    id: tc.id.clone().unwrap_or_default(),
                    name: tc.function.name.clone().unwrap_or_default(),
                    arguments: tc.function.arguments.clone().unwrap_or_default(),
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
    if let Some(ref t) = msg.content {
        blocks.push(crate::types::ContentBlock::Text { text: t.clone() });
    }

    Message {
        role: string_to_role(&msg.role),
        content: msg.content.clone(),
        content_blocks: blocks,
        reasoning: msg.reasoning_content.clone(),
        tool_calls,
        tool_call_id: msg.tool_call_id.clone(),
        images: Vec::new(),
    }
    .normalized()
}

pub(super) fn tool_to_openai(tool: &ToolDefinition) -> OpenAiTool {
    OpenAiTool {
        tool_type: "function".to_string(),
        function: OpenAiToolFunction {
            name: tool.name.clone(),
            description: tool.description.clone(),
            parameters: tool.parameters.clone(),
        },
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
    }
}
