use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use eventsource_stream::Eventsource;
use futures::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tracing::{debug, error, warn};

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

#[derive(Debug, Serialize)]
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
    Text { text: String },
    #[serde(rename = "thinking")]
    Thinking {
        thinking: String,
        #[serde(default, rename = "signature")]
        #[allow(dead_code)]
        _signature: Option<String>,
    },
    #[serde(rename = "image")]
    Image {
        source: ImageSource,
    },
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
    output_tokens: u32,
}

// ── Stream event types ──────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum AnthropicStreamEvent {
    #[serde(rename = "message_start")]
    MessageStart {
        message: AnthropicMessageStart,
    },
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
        #[allow(dead_code)]
        delta: serde_json::Value,
        usage: Option<AnthropicUsage>,
    },
    #[serde(rename = "message_stop")]
    MessageStop {},
    #[serde(rename = "ping")]
    Ping {},
    #[serde(rename = "error")]
    Error {
        error: AnthropicErrorDetail,
    },
    #[serde(other)]
    Unknown,
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
    Text { text: String },
    #[serde(rename = "thinking", alias = "thinking_start")]
    Thinking {
        thinking: String,
        #[serde(default, rename = "signature")]
        #[allow(dead_code)]
        _signature: Option<String>,
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
        #[allow(dead_code)]
        _signature: Option<String>,
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
    pub fn new(name: impl Into<String>, api_key: impl Into<String>, base_url: impl Into<String>) -> Self {
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

    /// Convert internal messages to Anthropic format.
    /// Extracts system message separately, merges tool results into user messages.
    fn convert_messages(messages: &[Message]) -> (Option<String>, Vec<AnthropicMessage>) {
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
                    // Anthropic expects tool results as user messages with tool_result content blocks
                    let block = ContentBlock::ToolResult {
                        tool_use_id: msg.tool_call_id.clone().unwrap_or_default(),
                        content: msg.content.clone().unwrap_or_default(),
                    };

                    // Try to merge into previous user message if it has tool_result blocks
                    if let Some(last) = anthropic_msgs.last_mut() {
                        if last.role == "user" {
                            match &mut last.content {
                                AnthropicContent::Blocks(blocks) => {
                                    blocks.push(block);
                                    continue;
                                }
                                _ => {}
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

    fn convert_tools(tools: &[ToolDefinition]) -> Vec<AnthropicTool> {
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

#[async_trait]
impl LlmProvider for AnthropicProvider {
    fn name(&self) -> &str {
        &self.name
    }

    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse> {
        let (system, messages) = Self::convert_messages(&request.messages);
        let tools = Self::convert_tools(&request.tools);
        let max_tokens = request.max_tokens.unwrap_or(4096);

        // Automatically enable thinking if max_tokens is high enough (Claude-style)
        let thinking = if max_tokens >= 2048 {
            Some(AnthropicThinkingConfig {
                thinking_type: "enabled".to_string(),
                budget_tokens: 1024.min(max_tokens - 1024),
            })
        } else {
            None
        };

        let body = AnthropicRequest {
            model: request.model.clone(),
            max_tokens,
            messages,
            system,
            tools,
            temperature: if thinking.is_some() { None } else { request.temperature },
            thinking,
            stream: false,
        };

        debug!("Sending Anthropic chat request to {}", self.messages_url());

        let resp = self
            .client
            .post(&self.messages_url())
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
                ContentBlock::Thinking { thinking, .. } => {
                    reasoning_content.push_str(thinking);
                    content_blocks.push(crate::types::ContentBlock::Thinking { thinking: thinking.clone(), signature: None });
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
            .map(|u| Usage {
                prompt_tokens: u.input_tokens,
                completion_tokens: u.output_tokens,
                total_tokens: u.input_tokens + u.output_tokens,
            })
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
        };

        Ok(ChatResponse {
            message,
            usage,
            model: api_resp.model,
        })
    }

    async fn chat_stream(
        &self,
        request: ChatRequest,
        tx: mpsc::Sender<StreamEvent>,
    ) -> Result<()> {
        let (system, messages) = Self::convert_messages(&request.messages);
        let tools = Self::convert_tools(&request.tools);
        let max_tokens = request.max_tokens.unwrap_or(4096);

        // Automatically enable thinking if max_tokens is high enough (Claude-style)
        let thinking = if max_tokens >= 2048 {
            Some(AnthropicThinkingConfig {
                thinking_type: "enabled".to_string(),
                budget_tokens: 1024.min(max_tokens - 1024),
            })
        } else {
            None
        };

        let body = AnthropicRequest {
            model: request.model.clone(),
            max_tokens,
            messages,
            system,
            tools,
            temperature: if thinking.is_some() { None } else { request.temperature },
            thinking,
            stream: true,
        };

        debug!(
            "Sending Anthropic streaming request to {}",
            self.messages_url()
        );

        let body_json = serde_json::to_string(&body).context("Failed to serialize request")?;

        let resp = self
            .client
            .post(&self.messages_url())
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .body(body_json)
            .send()
            .await
            .context("Failed to send Anthropic streaming request")?;

        let status = resp.status();
        if !status.is_success() {
            let error_text = resp.text().await.unwrap_or_default();
            if let Ok(err_resp) = serde_json::from_str::<AnthropicErrorResponse>(&error_text) {
                let msg = format!("Anthropic API error ({}): {}", status, err_resp.error.message);
                let _ = tx.send(StreamEvent::Error(msg.clone())).await;
                return Err(anyhow!(msg));
            }
            let msg = format!("Anthropic API error ({}): {}", status, error_text);
            let _ = tx.send(StreamEvent::Error(msg.clone())).await;
            return Err(anyhow!(msg));
        }

        let mut event_stream = resp.bytes_stream().eventsource();

        // Accumulated state for building the final ChatResponse (Claude-style)
        let mut content_blocks: std::collections::BTreeMap<u32, crate::types::ContentBlock> = std::collections::BTreeMap::new();
        let mut tool_calls: Vec<ToolCall> = Vec::new();
        let mut current_tool_args = String::new();
        let mut model = request.model.clone();
        let mut final_usage = Usage::default();

        while let Some(event_result) = event_stream.next().await {
            let event = match event_result {
                Ok(ev) => ev,
                Err(e) => {
                    let msg = format!("SSE stream error: {}", e);
                    error!("{}", msg);
                    let _ = tx.send(StreamEvent::Error(msg)).await;
                    continue;
                }
            };

            let data = event.data;

            let stream_event: AnthropicStreamEvent = match serde_json::from_str(&data) {
                Ok(e) => e,
                Err(e) => {
                    warn!("Failed to parse Anthropic stream event: {} (data: {})", e, data);
                    continue;
                }
            };

            match stream_event {
                AnthropicStreamEvent::MessageStart { message } => {
                    model = message.model;
                    if let Some(u) = message.usage {
                        final_usage.prompt_tokens = u.input_tokens;
                        let _ = tx.send(StreamEvent::UsageUpdate(Usage {
                            prompt_tokens: u.input_tokens,
                            completion_tokens: 0,
                            total_tokens: u.input_tokens,
                        })).await;
                    }
                }
                AnthropicStreamEvent::ContentBlockStart {
                    index,
                    content_block,
                } => match content_block {
                    ContentBlockStart::Text { text } => {
                        content_blocks.insert(index, crate::types::ContentBlock::Text { text: text.clone() });
                        if !text.is_empty() {
                            let _ = tx.send(StreamEvent::TextDelta(text)).await;
                        }
                    }
                    ContentBlockStart::Thinking { thinking, .. } => {
                        content_blocks.insert(index, crate::types::ContentBlock::Thinking { thinking: thinking.clone(), signature: None });
                        if !thinking.is_empty() {
                            let _ = tx.send(StreamEvent::ReasoningDelta(thinking)).await;
                        }
                    }
                    ContentBlockStart::ToolUse { id, name } => {
                        current_tool_args.clear();
                        tool_calls.push(ToolCall {
                            id: id.clone(),
                            name: name.clone(),
                            arguments: String::new(),
                        });
                        let _ = tx.send(StreamEvent::ToolCallStart { id, name }).await;
                    }
                    ContentBlockStart::Unknown => {}
                },
                AnthropicStreamEvent::ContentBlockDelta { index, delta } => match delta {
                    ContentBlockDelta::TextDelta { text } => {
                        if let Some(crate::types::ContentBlock::Text { text: t }) = content_blocks.get_mut(&index) {
                            t.push_str(&text);
                        }
                        if tx.send(StreamEvent::TextDelta(text)).await.is_err() {
                            return Ok(());
                        }
                    }
                    ContentBlockDelta::ThinkingDelta { thinking, .. } => {
                        if let Some(crate::types::ContentBlock::Thinking { thinking: t, .. }) = content_blocks.get_mut(&index) {
                            t.push_str(&thinking);
                        }
                        if tx.send(StreamEvent::ReasoningDelta(thinking)).await.is_err() {
                            return Ok(());
                        }
                    }
                    ContentBlockDelta::InputJsonDelta { partial_json } => {
                        current_tool_args.push_str(&partial_json);
                        if let Some(tc) = tool_calls.last_mut() {
                            tc.arguments.push_str(&partial_json);
                            let id = tc.id.clone();
                            let _ = tx
                                .send(StreamEvent::ToolCallDelta {
                                    id,
                                    arguments: partial_json,
                                })
                                .await;
                        }
                    }
                    ContentBlockDelta::Unknown => {}
                },
                AnthropicStreamEvent::ContentBlockStop { index: _ } => {
                    if let Some(tc) = tool_calls.last() {
                        if !tc.arguments.is_empty() || !current_tool_args.is_empty() {
                            let _ = tx
                                .send(StreamEvent::ToolCallEnd {
                                    id: tc.id.clone(),
                                })
                                .await;
                        }
                    }
                }
                AnthropicStreamEvent::MessageDelta { delta: _, usage } => {
                    if let Some(u) = usage {
                        final_usage.completion_tokens = u.output_tokens;
                        if u.input_tokens > 0 {
                            final_usage.prompt_tokens = u.input_tokens;
                        }
                        final_usage.total_tokens =
                            final_usage.prompt_tokens + final_usage.completion_tokens;
                        
                        let _ = tx.send(StreamEvent::UsageUpdate(Usage {
                            prompt_tokens: final_usage.prompt_tokens,
                            completion_tokens: final_usage.completion_tokens,
                            total_tokens: final_usage.total_tokens,
                        })).await;
                    }
                }
                AnthropicStreamEvent::MessageStop {} => {}
                AnthropicStreamEvent::Ping {} => {}
                AnthropicStreamEvent::Error { error } => {
                    let msg = format!("Anthropic stream error: {}", error.message);
                    error!("{}", msg);
                    let _ = tx.send(StreamEvent::Error(msg)).await;
                }
                AnthropicStreamEvent::Unknown => {}
            }
        }

        // Finalize final message using accumulated blocks (Claude-style)
        let final_content_blocks: Vec<crate::types::ContentBlock> = content_blocks.into_values().collect();
        let mut full_text = String::new();
        let mut full_reasoning = String::new();
        
        for block in &final_content_blocks {
            match block {
                crate::types::ContentBlock::Text { text } => full_text.push_str(text),
                crate::types::ContentBlock::Thinking { thinking, .. } => full_reasoning.push_str(thinking),
            }
        }

        let final_message = Message {
            role: Role::Assistant,
            content: if full_text.is_empty() { None } else { Some(full_text) },
            reasoning: if full_reasoning.is_empty() { None } else { Some(full_reasoning) },
            content_blocks: final_content_blocks,
            tool_calls,
            tool_call_id: None,
            images: Vec::new(),
        };

        let response = ChatResponse {
            message: final_message,
            usage: final_usage,
            model,
        };

        let _ = tx.send(StreamEvent::Done(response)).await;

        Ok(())
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
