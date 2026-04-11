use serde::{Deserialize, Serialize};

// ── Anthropic API request types ─────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub(super) struct AnthropicRequest {
    pub(super) model: String,
    pub(super) max_tokens: u32,
    pub(super) messages: Vec<AnthropicMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) system: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(super) tools: Vec<AnthropicTool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) thinking: Option<AnthropicThinkingConfig>,
    pub(super) stream: bool,
}

#[derive(Debug, Serialize, Clone)]
pub(super) struct AnthropicThinkingConfig {
    #[serde(rename = "type")]
    pub(super) thinking_type: String,
    pub(super) budget_tokens: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct AnthropicMessage {
    pub(super) role: String,
    pub(super) content: AnthropicContent,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub(super) enum AnthropicContent {
    Text(String),
    Blocks(Vec<ContentBlock>),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(super) enum ContentBlock {
    #[serde(rename = "text")]
    Text {
        #[serde(default)]
        text: String,
    },
    #[serde(rename = "thinking")]
    Thinking {
        #[serde(default)]
        thinking: String,
        #[serde(default, rename = "signature")]
        signature: Option<String>,
    },
    #[serde(rename = "image")]
    Image { source: ImageSource },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        content: String,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub(super) struct ImageSource {
    #[serde(rename = "type")]
    pub(super) source_type: String,
    pub(super) media_type: String,
    pub(super) data: String,
}

#[derive(Debug, Serialize)]
pub(super) struct AnthropicTool {
    pub(super) name: String,
    pub(super) description: String,
    pub(super) input_schema: serde_json::Value,
}

// ── Anthropic API response types ────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub(super) struct AnthropicResponse {
    #[allow(dead_code)]
    pub(super) id: String,
    pub(super) content: Vec<ContentBlock>,
    pub(super) model: String,
    #[serde(default)]
    pub(super) usage: Option<AnthropicUsage>,
    #[allow(dead_code)]
    pub(super) stop_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct AnthropicUsage {
    #[serde(default)]
    pub(super) input_tokens: u32,
    #[serde(default)]
    pub(super) cache_creation_input_tokens: u32,
    #[serde(default)]
    pub(super) cache_read_input_tokens: u32,
    #[serde(default)]
    pub(super) output_tokens: u32,
}

// ── Stream event types ──────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub(super) enum AnthropicStreamEvent {
    #[serde(rename = "message_start")]
    MessageStart { message: AnthropicMessageStart },
    #[serde(rename = "content_block_start")]
    ContentBlockStart {
        #[allow(dead_code)]
        #[serde(default)]
        index: u32,
        content_block: ContentBlockStart,
    },
    #[serde(rename = "content_block_delta")]
    ContentBlockDelta {
        #[allow(dead_code)]
        #[serde(default)]
        index: u32,
        delta: ContentBlockDelta,
    },
    #[serde(rename = "content_block_stop")]
    ContentBlockStop {
        #[allow(dead_code)]
        #[serde(default)]
        index: u32,
    },
    #[serde(rename = "message_delta")]
    MessageDelta {
        delta: AnthropicMessageDelta,
        usage: Option<AnthropicUsage>,
    },
    #[serde(rename = "message_stop")]
    MessageStop {},
    #[serde(rename = "ping")]
    Ping {},
    #[serde(rename = "error")]
    Error { error: AnthropicErrorDetail },
    #[serde(other)]
    Unknown,
}

impl AnthropicStreamEvent {
    pub(super) fn event_type(&self) -> &str {
        match self {
            AnthropicStreamEvent::MessageStart { .. } => "message_start",
            AnthropicStreamEvent::ContentBlockStart { .. } => "content_block_start",
            AnthropicStreamEvent::ContentBlockDelta { .. } => "content_block_delta",
            AnthropicStreamEvent::ContentBlockStop { .. } => "content_block_stop",
            AnthropicStreamEvent::MessageDelta { .. } => "message_delta",
            AnthropicStreamEvent::MessageStop { .. } => "message_stop",
            AnthropicStreamEvent::Ping { .. } => "ping",
            AnthropicStreamEvent::Error { .. } => "error",
            AnthropicStreamEvent::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Deserialize)]
pub(super) struct AnthropicMessageDelta {
    #[allow(dead_code)]
    pub(super) stop_reason: Option<String>,
    #[allow(dead_code)]
    pub(super) stop_sequence: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct AnthropicMessageStart {
    pub(super) model: String,
    #[serde(default)]
    pub(super) usage: Option<AnthropicUsage>,
    #[allow(dead_code)]
    pub(super) id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(super) enum ContentBlockStart {
    #[serde(rename = "text", alias = "text_start")]
    Text {
        #[serde(default)]
        text: String,
    },
    #[serde(rename = "thinking", alias = "thinking_start")]
    Thinking {
        #[serde(default)]
        thinking: String,
        #[serde(default, rename = "signature")]
        signature: Option<String>,
    },
    #[serde(rename = "tool_use", alias = "tool_use_start")]
    ToolUse { id: String, name: String },
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(super) enum ContentBlockDelta {
    #[serde(rename = "text_delta", alias = "text")]
    TextDelta { text: String },
    #[serde(rename = "thinking_delta", alias = "thinking")]
    ThinkingDelta {
        thinking: String,
        #[serde(default, rename = "signature")]
        signature: Option<String>,
    },
    #[serde(rename = "input_json_delta", alias = "input_json")]
    InputJsonDelta { partial_json: String },
    #[serde(other)]
    Unknown,
}

// ── Error types ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub(super) struct AnthropicErrorResponse {
    pub(super) error: AnthropicErrorDetail,
}

#[derive(Debug, Deserialize)]
pub(super) struct AnthropicErrorDetail {
    pub(super) message: String,
    #[serde(default)]
    #[allow(dead_code)]
    pub(super) r#type: Option<String>,
}
