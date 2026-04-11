mod request_conversion;
mod streaming;

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use eventsource_stream::Eventsource;
use futures::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tracing::{debug, error, warn};

use self::request_conversion::anthropic_usage_to_usage;

use crate::provider::LlmProvider;
use crate::types::{
    ChatRequest, ChatResponse, Message, ModelInfo, Role, StreamEvent, ToolCall, ToolDefinition,
    Usage,
};

// ── Anthropic API request types ─────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct AnthropicRequest {
    model: String,
    max_tokens: u32,
    messages: Vec<AnthropicMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<AnthropicTool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    thinking: Option<AnthropicThinkingConfig>,
    stream: bool,
}

#[derive(Debug, Serialize, Clone)]
struct AnthropicThinkingConfig {
    #[serde(rename = "type")]
    thinking_type: String,
    budget_tokens: u32,
}

#[derive(Debug, Serialize, Deserialize)]
struct AnthropicMessage {
    role: String,
    content: AnthropicContent,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum AnthropicContent {
    Text(String),
    Blocks(Vec<ContentBlock>),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ContentBlock {
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
struct ImageSource {
    #[serde(rename = "type")]
    source_type: String,
    media_type: String,
    data: String,
}

#[derive(Debug, Serialize)]
struct AnthropicTool {
    name: String,
    description: String,
    input_schema: serde_json::Value,
}

// ── Anthropic API response types ────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct AnthropicResponse {
    #[allow(dead_code)]
    id: String,
    content: Vec<ContentBlock>,
    model: String,
    #[serde(default)]
    usage: Option<AnthropicUsage>,
    #[allow(dead_code)]
    stop_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AnthropicUsage {
    #[serde(default)]
    input_tokens: u32,
    #[serde(default)]
    cache_creation_input_tokens: u32,
    #[serde(default)]
    cache_read_input_tokens: u32,
    #[serde(default)]
    output_tokens: u32,
}

// ── Stream event types ──────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum AnthropicStreamEvent {
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
    /// Get the event type name for logging
    fn event_type(&self) -> &str {
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
struct AnthropicMessageDelta {
    #[allow(dead_code)]
    pub stop_reason: Option<String>,
    #[allow(dead_code)]
    pub stop_sequence: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AnthropicMessageStart {
    model: String,
    #[serde(default)]
    usage: Option<AnthropicUsage>,
    #[allow(dead_code)]
    id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ContentBlockStart {
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
enum ContentBlockDelta {
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
struct AnthropicErrorResponse {
    error: AnthropicErrorDetail,
}

#[derive(Debug, Deserialize)]
struct AnthropicErrorDetail {
    message: String,
    #[serde(default)]
    #[allow(dead_code)]
    r#type: Option<String>,
}

// ── Provider implementation ─────────────────────────────────────────────────

pub struct AnthropicProvider {
    name: String,
    api_key: String,
    base_url: String,
    client: Client,
}

impl AnthropicProvider {
    pub fn new(
        name: impl Into<String>,
        api_key: impl Into<String>,
        base_url: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            api_key: api_key.into(),
            base_url: base_url.into(),
            client: Client::builder()
                .user_agent(format!("Yode/{}", env!("CARGO_PKG_VERSION")))
                .build()
                .expect("Failed to build HTTP client"),
        }
    }

    fn messages_url(&self) -> String {
        format!("{}/v1/messages", self.base_url.trim_end_matches('/'))
    }

}

#[async_trait]
impl LlmProvider for AnthropicProvider {
    fn name(&self) -> &str {
        &self.name
    }

    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse> {
        let (system, messages) = Self::convert_messages(&request.messages);
        let tools = Self::convert_tools(&request.tools);
        let max_tokens = request.max_tokens.unwrap_or(4096);

        // Enable thinking support - required for proper reasoning separation
        // Some APIs (like DashScope) may not support it, but we try anyway
        let thinking = Some(AnthropicThinkingConfig {
            thinking_type: "enabled".to_string(),
            budget_tokens: 1024,
        });

        let body = AnthropicRequest {
            model: request.model.clone(),
            max_tokens,
            messages,
            system,
            tools,
            temperature: if thinking.is_some() {
                None
            } else {
                request.temperature
            },
            thinking: thinking.clone(),
            stream: false,
        };

        debug!("Sending Anthropic chat request to {}", self.messages_url());

        let resp = self
            .client
            .post(self.messages_url())
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .context("Failed to send Anthropic chat request")?;

        let status = resp.status();
        if !status.is_success() {
            let error_text = resp.text().await.unwrap_or_default();
            if let Ok(err_resp) = serde_json::from_str::<AnthropicErrorResponse>(&error_text) {
                return Err(anyhow!(
                    "Anthropic API error ({}): {}",
                    status,
                    err_resp.error.message
                ));
            }
            return Err(anyhow!("Anthropic API error ({}): {}", status, error_text));
        }

        let api_resp: AnthropicResponse = resp
            .json()
            .await
            .context("Failed to parse Anthropic response")?;

        // Convert response
        let mut text_content = String::new();
        let mut reasoning_content = String::new();
        let mut tool_calls = Vec::new();
        let mut content_blocks = Vec::new();

        for block in &api_resp.content {
            match block {
                ContentBlock::Text { text } => {
                    text_content.push_str(text);
                    content_blocks.push(crate::types::ContentBlock::Text { text: text.clone() });
                }
                ContentBlock::Thinking {
                    thinking,
                    signature,
                } => {
                    reasoning_content.push_str(thinking);
                    content_blocks.push(crate::types::ContentBlock::Thinking {
                        thinking: thinking.clone(),
                        signature: signature.clone(),
                    });
                }
                ContentBlock::ToolUse { id, name, input } => {
                    tool_calls.push(ToolCall {
                        id: id.clone(),
                        name: name.clone(),
                        arguments: serde_json::to_string(input).unwrap_or_default(),
                    });
                }
                _ => {}
            }
        }

        let usage = api_resp
            .usage
            .map(|u| anthropic_usage_to_usage(&u))
            .unwrap_or_default();

        let message = Message {
            role: Role::Assistant,
            content: if text_content.is_empty() {
                None
            } else {
                Some(text_content)
            },
            reasoning: if reasoning_content.is_empty() {
                None
            } else {
                Some(reasoning_content)
            },
            content_blocks,
            tool_calls,
            tool_call_id: None,
            images: Vec::new(),
        }
        .normalized();

        let stop_reason = match api_resp.stop_reason.as_deref() {
            Some("end_turn") => Some(crate::types::StopReason::EndTurn),
            Some("tool_use") => Some(crate::types::StopReason::ToolUse),
            Some("max_tokens") => Some(crate::types::StopReason::MaxTokens),
            Some("stop_sequence") => Some(crate::types::StopReason::StopSequence),
            Some(other) => Some(crate::types::StopReason::Other(other.to_string())),
            None => None,
        };

        Ok(ChatResponse {
            message,
            usage,
            model: api_resp.model,
            stop_reason,
        })
    }

    async fn chat_stream(&self, request: ChatRequest, tx: mpsc::Sender<StreamEvent>) -> Result<()> {
        self.send_chat_stream_request(request, tx).await
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        // Anthropic doesn't have a list models endpoint
        Ok(vec![
            ModelInfo {
                id: "claude-3-5-sonnet-20241022".to_string(),
                name: "Claude 3.5 Sonnet".to_string(),
                provider: self.name.clone(),
            },
            ModelInfo {
                id: "claude-3-5-haiku-20241022".to_string(),
                name: "Claude 3.5 Haiku".to_string(),
                provider: self.name.clone(),
            },
        ])
    }
}
