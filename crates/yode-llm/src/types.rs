use serde::{Deserialize, Serialize};

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
    Text {
        text: String,
    },
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
        if let Some(ref r) = reasoning {
            blocks.push(ContentBlock::Thinking { thinking: r.clone(), signature: None });
        }
        if let Some(ref t) = content {
            blocks.push(ContentBlock::Text { text: t.clone() });
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<Message>,
    pub tools: Vec<ToolDefinition>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct ChatResponse {
    pub message: Message,
    pub usage: Usage,
    pub model: String,
}

#[derive(Debug, Clone, Default)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

#[derive(Debug, Clone)]
pub enum StreamEvent {
    TextDelta(String),
    ReasoningDelta(String),
    /// Real-time usage information (e.g. prompt tokens known at start)
    UsageUpdate(Usage),
    ToolCallStart { id: String, name: String },
    ToolCallDelta { id: String, arguments: String },
    ToolCallEnd { id: String },
    Done(ChatResponse),
    Error(String),
}

#[derive(Debug, Clone)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub provider: String,
}
