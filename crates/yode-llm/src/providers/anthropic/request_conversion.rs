use crate::types::{Message, Role, ToolDefinition, Usage};

use super::types::{
    AnthropicContent, AnthropicMessage, AnthropicTool, AnthropicUsage, ContentBlock, ImageSource,
};
use super::AnthropicProvider;

impl AnthropicProvider {
    /// Convert internal messages to Anthropic format.
    /// Extracts system message separately, merges tool results into user messages.
    pub(super) fn convert_messages(
        messages: &[Message],
    ) -> (Option<String>, Vec<AnthropicMessage>) {
        let mut system_prompt = None;
        let mut anthropic_msgs: Vec<AnthropicMessage> = Vec::new();

        for msg in messages {
            match msg.role {
                Role::System => {
                    system_prompt = msg.content.clone();
                }
                Role::User => {
                    let content = if msg.images.is_empty() {
                        AnthropicContent::Text(msg.content.clone().unwrap_or_default())
                    } else {
                        let mut blocks = Vec::new();

                        if let Some(text) = &msg.content {
                            if !text.is_empty() {
                                blocks.push(ContentBlock::Text { text: text.clone() });
                            }
                        }

                        for img in &msg.images {
                            blocks.push(ContentBlock::Image {
                                source: ImageSource {
                                    source_type: "base64".to_string(),
                                    media_type: img.media_type.clone(),
                                    data: img.base64.clone(),
                                },
                            });
                        }

                        AnthropicContent::Blocks(blocks)
                    };

                    anthropic_msgs.push(AnthropicMessage {
                        role: "user".to_string(),
                        content,
                    });
                }
                Role::Assistant => {
                    let mut blocks = Vec::new();

                    if let Some(text) = &msg.content {
                        if !text.is_empty() {
                            blocks.push(ContentBlock::Text { text: text.clone() });
                        }
                    }

                    for tc in &msg.tool_calls {
                        let input: serde_json::Value =
                            serde_json::from_str(&tc.arguments).unwrap_or_default();
                        blocks.push(ContentBlock::ToolUse {
                            id: tc.id.clone(),
                            name: tc.name.clone(),
                            input,
                        });
                    }

                    if blocks.is_empty() {
                        blocks.push(ContentBlock::Text {
                            text: String::new(),
                        });
                    }

                    anthropic_msgs.push(AnthropicMessage {
                        role: "assistant".to_string(),
                        content: AnthropicContent::Blocks(blocks),
                    });
                }
                Role::Tool => {
                    let block = ContentBlock::ToolResult {
                        tool_use_id: msg.tool_call_id.clone().unwrap_or_default(),
                        content: msg.content.clone().unwrap_or_default(),
                    };

                    if let Some(last) = anthropic_msgs.last_mut() {
                        if last.role == "user" {
                            if let AnthropicContent::Blocks(blocks) = &mut last.content {
                                blocks.push(block);
                                continue;
                            }
                        }
                    }

                    anthropic_msgs.push(AnthropicMessage {
                        role: "user".to_string(),
                        content: AnthropicContent::Blocks(vec![block]),
                    });
                }
            }
        }

        (system_prompt, anthropic_msgs)
    }

    pub(super) fn convert_tools(tools: &[ToolDefinition]) -> Vec<AnthropicTool> {
        tools
            .iter()
            .map(|t| AnthropicTool {
                name: t.name.clone(),
                description: t.description.clone(),
                input_schema: t.parameters.clone(),
            })
            .collect()
    }
}

pub(super) fn anthropic_usage_to_usage(usage: &AnthropicUsage) -> Usage {
    let prompt_tokens = usage
        .input_tokens
        .saturating_add(usage.cache_creation_input_tokens)
        .saturating_add(usage.cache_read_input_tokens);
    Usage {
        prompt_tokens,
        completion_tokens: usage.output_tokens,
        total_tokens: prompt_tokens.saturating_add(usage.output_tokens),
        cache_write_tokens: usage.cache_creation_input_tokens,
        cache_read_tokens: usage.cache_read_input_tokens,
    }
}
