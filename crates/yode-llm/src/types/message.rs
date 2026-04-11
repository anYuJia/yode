use serde::{Deserialize, Serialize};

use super::protocol::ToolCall;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

/// Image data for multimodal messages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageData {
    /// Base64-encoded image data.
    pub base64: String,
    /// MIME type (e.g., "image/png", "image/jpeg", "image/gif", "image/webp").
    pub media_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text { text: String },
    Thinking {
        thinking: String,
        #[serde(default)]
        signature: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    /// Standardized content blocks (preferred for modern models)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub content_blocks: Vec<ContentBlock>,
    /// Legacy flat content string (for backward compatibility)
    pub content: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<String>,
    pub tool_calls: Vec<ToolCall>,
    pub tool_call_id: Option<String>,
    /// Images attached to this message (for multimodal support).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<ImageData>,
}

impl Message {
    pub fn system(content: impl Into<String>) -> Self {
        let text = content.into();
        Self {
            role: Role::System,
            content: Some(text.clone()),
            content_blocks: vec![ContentBlock::Text { text }],
            reasoning: None,
            tool_calls: Vec::new(),
            tool_call_id: None,
            images: Vec::new(),
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        let text = content.into();
        Self {
            role: Role::User,
            content: Some(text.clone()),
            content_blocks: vec![ContentBlock::Text { text }],
            reasoning: None,
            tool_calls: Vec::new(),
            tool_call_id: None,
            images: Vec::new(),
        }
    }

    /// Create a user message with images.
    pub fn user_with_images(content: impl Into<String>, images: Vec<ImageData>) -> Self {
        let text = content.into();
        Self {
            role: Role::User,
            content: Some(text.clone()),
            content_blocks: vec![ContentBlock::Text { text }],
            reasoning: None,
            tool_calls: Vec::new(),
            tool_call_id: None,
            images,
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        let text = content.into();
        Self {
            role: Role::Assistant,
            content: Some(text.clone()),
            content_blocks: vec![ContentBlock::Text { text }],
            reasoning: None,
            tool_calls: Vec::new(),
            tool_call_id: None,
            images: Vec::new(),
        }
    }

    /// Create an assistant message with reasoning.
    pub fn assistant_with_reasoning(content: Option<String>, reasoning: Option<String>) -> Self {
        let mut blocks = Vec::new();
        if let Some(reasoning_text) = &reasoning {
            blocks.push(ContentBlock::Thinking {
                thinking: reasoning_text.clone(),
                signature: None,
            });
        }
        if let Some(text) = &content {
            blocks.push(ContentBlock::Text { text: text.clone() });
        }

        Self {
            role: Role::Assistant,
            content,
            content_blocks: blocks,
            reasoning,
            tool_calls: Vec::new(),
            tool_call_id: None,
            images: Vec::new(),
        }
    }

    pub fn tool_result(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        let text = content.into();
        Self {
            role: Role::Tool,
            content: Some(text.clone()),
            content_blocks: vec![ContentBlock::Text { text }],
            reasoning: None,
            tool_calls: Vec::new(),
            tool_call_id: Some(tool_call_id.into()),
            images: Vec::new(),
        }
    }

    /// Create a tool result with an image.
    pub fn tool_result_with_image(
        tool_call_id: impl Into<String>,
        content: impl Into<String>,
        image: ImageData,
    ) -> Self {
        let text = content.into();
        Self {
            role: Role::Tool,
            content: Some(text.clone()),
            content_blocks: vec![ContentBlock::Text { text }],
            reasoning: None,
            tool_calls: Vec::new(),
            tool_call_id: Some(tool_call_id.into()),
            images: vec![image],
        }
    }

    /// Normalize the legacy flat fields and structured content blocks so they stay in sync.
    pub fn normalized(mut self) -> Self {
        self.normalize_in_place();
        self
    }

    /// Normalize the legacy flat fields and structured content blocks so they stay in sync.
    pub fn normalize_in_place(&mut self) {
        fn sanitize(value: Option<String>) -> Option<String> {
            value.and_then(|text| {
                if text.trim().is_empty() {
                    None
                } else {
                    Some(text)
                }
            })
        }

        self.content = sanitize(self.content.take());
        self.reasoning = sanitize(self.reasoning.take());

        let mut thinking_blocks = Vec::new();
        let mut text_blocks = Vec::new();

        for block in self.content_blocks.drain(..) {
            match block {
                ContentBlock::Thinking { thinking, signature } if !thinking.trim().is_empty() => {
                    thinking_blocks.push(ContentBlock::Thinking { thinking, signature });
                }
                ContentBlock::Text { text } if !text.trim().is_empty() => {
                    text_blocks.push(ContentBlock::Text { text });
                }
                _ => {}
            }
        }

        let block_reasoning = thinking_blocks
            .iter()
            .filter_map(|block| match block {
                ContentBlock::Thinking { thinking, .. } => Some(thinking.as_str()),
                _ => None,
            })
            .collect::<String>();
        let block_content = text_blocks
            .iter()
            .filter_map(|block| match block {
                ContentBlock::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<String>();

        if self.reasoning.is_none() && !block_reasoning.is_empty() {
            self.reasoning = Some(block_reasoning.clone());
        }
        if self.content.is_none() && !block_content.is_empty() {
            self.content = Some(block_content.clone());
        }

        match (&self.reasoning, block_reasoning.is_empty()) {
            (Some(reasoning), true) => {
                thinking_blocks.push(ContentBlock::Thinking {
                    thinking: reasoning.clone(),
                    signature: None,
                });
            }
            (Some(reasoning), false) if reasoning != &block_reasoning => {
                thinking_blocks = vec![ContentBlock::Thinking {
                    thinking: reasoning.clone(),
                    signature: None,
                }];
            }
            _ => {}
        }

        match (&self.content, block_content.is_empty()) {
            (Some(content), true) => {
                text_blocks.push(ContentBlock::Text {
                    text: content.clone(),
                });
            }
            (Some(content), false) if content != &block_content => {
                text_blocks = vec![ContentBlock::Text {
                    text: content.clone(),
                }];
            }
            _ => {}
        }

        self.content_blocks = thinking_blocks;
        self.content_blocks.extend(text_blocks);
    }
}

#[cfg(test)]
mod tests {
    use super::{ContentBlock, Message, Role};

    #[test]
    fn normalize_builds_blocks_from_flat_fields() {
        let message = Message {
            role: Role::Assistant,
            content: Some("final answer".to_string()),
            content_blocks: Vec::new(),
            reasoning: Some("step by step".to_string()),
            tool_calls: Vec::new(),
            tool_call_id: None,
            images: Vec::new(),
        }
        .normalized();

        assert_eq!(message.content_blocks.len(), 2);
        assert!(matches!(
            &message.content_blocks[0],
            ContentBlock::Thinking { thinking, .. } if thinking == "step by step"
        ));
        assert!(matches!(
            &message.content_blocks[1],
            ContentBlock::Text { text } if text == "final answer"
        ));
    }

    #[test]
    fn normalize_backfills_flat_fields_from_blocks() {
        let message = Message {
            role: Role::Assistant,
            content: None,
            content_blocks: vec![
                ContentBlock::Thinking {
                    thinking: "inspect repo".to_string(),
                    signature: Some("sig".to_string()),
                },
                ContentBlock::Text {
                    text: "done".to_string(),
                },
            ],
            reasoning: None,
            tool_calls: Vec::new(),
            tool_call_id: None,
            images: Vec::new(),
        }
        .normalized();

        assert_eq!(message.reasoning.as_deref(), Some("inspect repo"));
        assert_eq!(message.content.as_deref(), Some("done"));
        assert!(matches!(
            &message.content_blocks[0],
            ContentBlock::Thinking { signature, .. } if signature.as_deref() == Some("sig")
        ));
    }
}
