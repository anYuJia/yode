use std::collections::HashSet;

use crate::types::{
    AnthropicRequestHints, Message, RestoreSystemBlockHint, Role, ToolDefinition, Usage,
};

use super::types::{
    AnthropicContent, AnthropicMessage, AnthropicTool, AnthropicUsage, CacheControl,
    CacheEditDelete, ContentBlock, ImageSource, SystemTextBlock,
};
use super::AnthropicProvider;

fn default_cache_control() -> CacheControl {
    CacheControl {
        cache_type: "ephemeral".to_string(),
        scope: None,
        ttl: None,
    }
}

fn ensure_blocks(content: &mut AnthropicContent) -> &mut Vec<ContentBlock> {
    match content {
        AnthropicContent::Blocks(blocks) => blocks,
        AnthropicContent::Text(text) => {
            let original = std::mem::take(text);
            *content = AnthropicContent::Blocks(vec![ContentBlock::Text {
                text: original,
                cache_control: None,
            }]);
            ensure_blocks(content)
        }
    }
}

fn parse_tool_call_input(tool_name: &str, arguments: &str) -> serde_json::Value {
    match serde_json::from_str(arguments) {
        Ok(value) => value,
        Err(err) => {
            tracing::warn!(
                tool_name,
                error = %err,
                "failed to parse Anthropic tool call arguments as JSON; preserving raw arguments"
            );
            serde_json::json!({ "raw_arguments": arguments })
        }
    }
}

fn insert_cache_edits_after_tool_results(blocks: &mut Vec<ContentBlock>, deleted_refs: &[String]) {
    if deleted_refs.is_empty() {
        return;
    }

    let insert_at = blocks
        .iter()
        .rposition(|block| matches!(block, ContentBlock::ToolResult { .. }))
        .map(|index| index + 1)
        .unwrap_or(blocks.len());

    blocks.insert(
        insert_at,
        ContentBlock::CacheEdits {
            edits: deleted_refs
                .iter()
                .map(|cache_reference| CacheEditDelete {
                    edit_type: "delete".to_string(),
                    cache_reference: cache_reference.clone(),
                })
                .collect(),
        },
    );
}

fn apply_anthropic_prompt_cache_hints(
    messages: &mut [AnthropicMessage],
    hints: &AnthropicRequestHints,
) {
    if messages.is_empty() || !hints.enable_prompt_caching {
        return;
    }

    if let Some(last_message) = messages.last_mut() {
        let blocks = ensure_blocks(&mut last_message.content);
        if let Some(block) = blocks.last_mut() {
            match block {
                ContentBlock::Text { cache_control, .. }
                | ContentBlock::Thinking { cache_control, .. } => {
                    *cache_control = Some(default_cache_control());
                }
                ContentBlock::ToolUse { .. }
                | ContentBlock::ToolResult { .. }
                | ContentBlock::Image { .. }
                | ContentBlock::CacheEdits { .. }
                | ContentBlock::Unknown => {
                    blocks.push(ContentBlock::Text {
                        text: String::new(),
                        cache_control: Some(default_cache_control()),
                    });
                }
            }
        } else {
            blocks.push(ContentBlock::Text {
                text: String::new(),
                cache_control: Some(default_cache_control()),
            });
        }
    }

    let last_cache_marker_index = messages.len().saturating_sub(1);
    let pending_refs = hints
        .pending_deleted_cache_references
        .iter()
        .cloned()
        .collect::<HashSet<_>>();
    let pinned_refs = hints
        .pinned_deleted_cache_references
        .iter()
        .cloned()
        .collect::<HashSet<_>>();

    let mut pinned_groups = Vec::<(usize, Vec<String>)>::new();
    let mut matched_pending_refs = Vec::new();

    for (index, message) in messages.iter_mut().enumerate() {
        if message.role != "user" {
            continue;
        }

        let blocks = ensure_blocks(&mut message.content);
        let mut pinned_for_message = Vec::new();

        for block in blocks.iter_mut() {
            if let ContentBlock::ToolResult {
                tool_use_id,
                cache_reference,
                ..
            } = block
            {
                if index < last_cache_marker_index {
                    *cache_reference = Some(tool_use_id.clone());
                }
                if pinned_refs.contains(tool_use_id) {
                    pinned_for_message.push(tool_use_id.clone());
                }
                if pending_refs.contains(tool_use_id) {
                    matched_pending_refs.push(tool_use_id.clone());
                }
            }
        }

        if !pinned_for_message.is_empty() {
            pinned_for_message.sort();
            pinned_for_message.dedup();
            pinned_groups.push((index, pinned_for_message));
        }
    }

    for (index, refs) in pinned_groups {
        if let Some(message) = messages.get_mut(index) {
            let blocks = ensure_blocks(&mut message.content);
            insert_cache_edits_after_tool_results(blocks, &refs);
        }
    }

    if !matched_pending_refs.is_empty() {
        matched_pending_refs.sort();
        matched_pending_refs.dedup();

        for message in messages.iter_mut().rev() {
            if message.role != "user" {
                continue;
            }
            let blocks = ensure_blocks(&mut message.content);
            insert_cache_edits_after_tool_results(blocks, &matched_pending_refs);
            break;
        }
    }
}

impl AnthropicProvider {
    /// Convert internal messages to Anthropic format.
    /// Extracts system message separately, merges tool results into user messages.
    pub(super) fn convert_messages(
        messages: &[Message],
        hints: Option<&AnthropicRequestHints>,
        restore_system_blocks: &[RestoreSystemBlockHint],
    ) -> (Option<Vec<SystemTextBlock>>, Vec<AnthropicMessage>) {
        let mut system_blocks = Vec::new();
        let mut anthropic_msgs: Vec<AnthropicMessage> = Vec::new();

        for msg in messages {
            match msg.role {
                Role::System => {
                    if let Some(text) = msg.content.as_ref().filter(|text| !text.is_empty()) {
                        system_blocks.push(SystemTextBlock {
                            block_type: "text".to_string(),
                            text: text.clone(),
                            cache_control: None,
                        });
                    }
                }
                Role::User => {
                    let content = if msg.images.is_empty() {
                        AnthropicContent::Text(msg.content.clone().unwrap_or_default())
                    } else {
                        let mut blocks = Vec::new();

                        if let Some(text) = &msg.content {
                            if !text.is_empty() {
                                blocks.push(ContentBlock::Text {
                                    text: text.clone(),
                                    cache_control: None,
                                });
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
                            blocks.push(ContentBlock::Text {
                                text: text.clone(),
                                cache_control: None,
                            });
                        }
                    }

                    for tc in &msg.tool_calls {
                        let input = parse_tool_call_input(&tc.name, &tc.arguments);
                        blocks.push(ContentBlock::ToolUse {
                            id: tc.id.clone(),
                            name: tc.name.clone(),
                            input,
                        });
                    }

                    if blocks.is_empty() {
                        blocks.push(ContentBlock::Text {
                            text: String::new(),
                            cache_control: None,
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
                        cache_reference: None,
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

        for block in restore_system_blocks
            .iter()
            .filter(|block| !block.content.is_empty())
        {
            system_blocks.push(SystemTextBlock {
                block_type: "text".to_string(),
                text: format!("[Post-compact restore: {}]\n{}", block.kind, block.content),
                cache_control: None,
            });
        }

        if let Some(hints) = hints {
            apply_anthropic_prompt_cache_hints(&mut anthropic_msgs, hints);
        }

        if hints.is_some_and(|h| h.enable_prompt_caching) {
            if let Some(last_block) = system_blocks.last_mut() {
                last_block.cache_control = Some(default_cache_control());
            }
        }

        let system = (!system_blocks.is_empty()).then_some(system_blocks);

        (system, anthropic_msgs)
    }

    pub(super) fn convert_tools(
        tools: &[ToolDefinition],
        hints: Option<&AnthropicRequestHints>,
    ) -> Vec<AnthropicTool> {
        let enable_prompt_caching = hints.is_some_and(|h| h.enable_prompt_caching);
        let last_index = tools.len().saturating_sub(1);
        tools
            .iter()
            .enumerate()
            .map(|(index, t)| AnthropicTool {
                name: t.name.clone(),
                description: t.description.clone(),
                input_schema: t.parameters.clone(),
                cache_control: (enable_prompt_caching && index == last_index)
                    .then(default_cache_control),
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
        cache_deleted_tokens: usage.cache_deleted_input_tokens,
    }
}

#[cfg(test)]
mod tests {
    use crate::types::{AnthropicRequestHints, Message, RestoreSystemBlockHint, ToolDefinition};

    use super::{
        anthropic_usage_to_usage, AnthropicContent, AnthropicProvider, AnthropicUsage, ContentBlock,
    };

    #[test]
    fn anthropic_conversion_adds_cache_metadata_for_cached_microcompact() {
        let messages = vec![
            Message::system("system"),
            Message::user("user"),
            Message::assistant("assistant"),
            Message::tool_result("tc1", "tool result"),
            Message::user("tail"),
        ];
        let hints = AnthropicRequestHints {
            enable_prompt_caching: true,
            pending_deleted_cache_references: vec!["tc1".to_string()],
            pinned_deleted_cache_references: vec![],
        };

        let (system, converted) = AnthropicProvider::convert_messages(&messages, Some(&hints), &[]);

        assert!(system.is_some());
        let tool_result_user = &converted[2];
        let tail_user = converted.last().unwrap();

        match &tool_result_user.content {
            AnthropicContent::Blocks(blocks) => {
                assert!(blocks.iter().any(|block| matches!(
                    block,
                    ContentBlock::ToolResult {
                        cache_reference: Some(reference),
                        ..
                    } if reference == "tc1"
                )));
            }
            AnthropicContent::Text(_) => panic!("expected tool result blocks"),
        }

        match &tail_user.content {
            AnthropicContent::Blocks(blocks) => {
                assert!(blocks.iter().any(|block| matches!(
                    block,
                    ContentBlock::CacheEdits { edits }
                    if edits.iter().any(|edit| edit.cache_reference == "tc1")
                )));
                assert!(blocks.iter().any(|block| matches!(
                    block,
                    ContentBlock::Text {
                        cache_control: Some(_),
                        ..
                    }
                )));
            }
            AnthropicContent::Text(_) => panic!("expected cache-control block"),
        }
    }

    #[test]
    fn anthropic_conversion_preserves_multiple_system_messages() {
        let messages = vec![
            Message::system("base system"),
            Message::system("[Context summary] compacted"),
            Message::system("[Post-compact restore: runtime]\n- cwd: /tmp"),
            Message::user("resume"),
        ];
        let hints = AnthropicRequestHints {
            enable_prompt_caching: true,
            pending_deleted_cache_references: vec![],
            pinned_deleted_cache_references: vec![],
        };

        let (system, converted) = AnthropicProvider::convert_messages(&messages, Some(&hints), &[]);
        let system = system.expect("system blocks");

        assert_eq!(system.len(), 3);
        assert_eq!(system[0].text, "base system");
        assert_eq!(system[1].text, "[Context summary] compacted");
        assert!(system[2]
            .text
            .starts_with("[Post-compact restore: runtime]"));
        assert!(system[0].cache_control.is_none());
        assert!(system[1].cache_control.is_none());
        assert!(system[2].cache_control.is_some());
        assert_eq!(converted.len(), 1);
    }

    #[test]
    fn anthropic_conversion_preserves_invalid_tool_arguments_as_raw_payload() {
        let mut assistant = Message::assistant("");
        assistant.tool_calls.push(crate::types::ToolCall {
            id: "call-1".to_string(),
            name: "broken_tool".to_string(),
            arguments: "{not-json".to_string(),
        });

        let (_system, converted) = AnthropicProvider::convert_messages(&[assistant], None, &[]);

        match &converted[0].content {
            AnthropicContent::Blocks(blocks) => {
                let tool_use = blocks
                    .iter()
                    .find_map(|block| match block {
                        ContentBlock::ToolUse { input, .. } => Some(input),
                        _ => None,
                    })
                    .expect("tool use block");

                assert_eq!(tool_use["raw_arguments"], "{not-json");
            }
            AnthropicContent::Text(_) => panic!("expected tool use blocks"),
        }
    }

    #[test]
    fn anthropic_conversion_marks_last_tool_for_prompt_cache() {
        let tools = vec![
            ToolDefinition {
                name: "read_file".to_string(),
                description: "read".to_string(),
                parameters: serde_json::json!({"type":"object"}),
                annotations: Default::default(),
            },
            ToolDefinition {
                name: "edit_file".to_string(),
                description: "edit".to_string(),
                parameters: serde_json::json!({"type":"object"}),
                annotations: Default::default(),
            },
        ];
        let hints = AnthropicRequestHints {
            enable_prompt_caching: true,
            pending_deleted_cache_references: vec![],
            pinned_deleted_cache_references: vec![],
        };

        let converted = AnthropicProvider::convert_tools(&tools, Some(&hints));
        assert!(converted[0].cache_control.is_none());
        assert!(converted[1].cache_control.is_some());
    }

    #[test]
    fn anthropic_conversion_replays_pinned_cache_edits_near_original_tool_results() {
        let messages = vec![
            Message::system("system"),
            Message::user("user"),
            Message::assistant("assistant"),
            Message::tool_result("tc1", "tool result"),
            Message::user("tail"),
        ];
        let hints = AnthropicRequestHints {
            enable_prompt_caching: true,
            pending_deleted_cache_references: vec![],
            pinned_deleted_cache_references: vec!["tc1".to_string()],
        };

        let (_system, converted) =
            AnthropicProvider::convert_messages(&messages, Some(&hints), &[]);
        let tool_result_user = &converted[2];

        match &tool_result_user.content {
            AnthropicContent::Blocks(blocks) => {
                assert!(blocks.iter().any(|block| matches!(
                    block,
                    ContentBlock::CacheEdits { edits }
                    if edits.iter().any(|edit| edit.cache_reference == "tc1")
                )));
            }
            AnthropicContent::Text(_) => panic!("expected tool result blocks"),
        }
    }

    #[test]
    fn anthropic_conversion_appends_restore_blocks_from_provider_hints() {
        let messages = vec![Message::system("base system"), Message::user("resume")];
        let (system, converted) = AnthropicProvider::convert_messages(
            &messages,
            None,
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

        let system = system.expect("system blocks");
        assert_eq!(converted.len(), 1);
        assert_eq!(system.len(), 3);
        assert_eq!(system[0].text, "base system");
        assert_eq!(
            system[1].text,
            "[Post-compact restore: runtime]\n- Runtime cwd: /tmp"
        );
        assert_eq!(
            system[2].text,
            "[Post-compact restore: files]\n- Recent files read: src/main.rs"
        );
    }

    #[test]
    fn anthropic_usage_conversion_tracks_deleted_cache_tokens() {
        let usage = anthropic_usage_to_usage(&AnthropicUsage {
            input_tokens: 1000,
            cache_creation_input_tokens: 200,
            cache_read_input_tokens: 300,
            cache_deleted_input_tokens: 150,
            output_tokens: 80,
        });

        assert_eq!(usage.prompt_tokens, 1500);
        assert_eq!(usage.cache_deleted_tokens, 150);
    }
}
