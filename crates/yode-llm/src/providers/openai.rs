use std::collections::HashMap;

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use eventsource_stream::Eventsource;
use futures::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tracing::{debug, error, trace, warn};

use crate::provider::LlmProvider;
use crate::types::{
    ChatRequest, ChatResponse, Message, ModelInfo, Role, StreamEvent, ToolCall, ToolDefinition,
    Usage,
};

// ── OpenAI API request types ────────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct OpenAiRequest {
    model: String,
    messages: Vec<OpenAiMessage>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<OpenAiTool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    stream: bool,
    /// Request usage stats in the final streaming chunk.
    #[serde(skip_serializing_if = "Option::is_none")]
    stream_options: Option<StreamOptions>,
}

#[derive(Debug, Serialize)]
struct StreamOptions {
    include_usage: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct OpenAiMessage {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    #[serde(
        alias = "thought",
        alias = "reasoning",
        skip_serializing_if = "Option::is_none"
    )]
    reasoning_content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<OpenAiToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct OpenAiToolCall {
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<String>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    call_type: Option<String>,
    function: OpenAiFunction,
    #[serde(skip_serializing_if = "Option::is_none")]
    index: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct OpenAiFunction {
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    arguments: Option<String>,
}

#[derive(Debug, Serialize)]
struct OpenAiTool {
    #[serde(rename = "type")]
    tool_type: String,
    function: OpenAiToolFunction,
}

#[derive(Debug, Serialize)]
struct OpenAiToolFunction {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

// ── OpenAI API response types ───────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct OpenAiResponse {
    choices: Vec<OpenAiChoice>,
    #[serde(default)]
    usage: Option<OpenAiUsage>,
    model: String,
}

#[derive(Debug, Deserialize)]
struct OpenAiChoice {
    message: OpenAiMessage,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAiUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

#[derive(Debug, Deserialize)]
struct OpenAiModelsResponse {
    data: Vec<OpenAiModel>,
}

#[derive(Debug, Deserialize)]
struct OpenAiModel {
    id: String,
}

// ── Stream chunk types ──────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct OpenAiStreamChunk {
    choices: Vec<OpenAiStreamChoice>,
    #[serde(default)]
    usage: Option<OpenAiUsage>,
    model: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAiStreamChoice {
    delta: OpenAiStreamDelta,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAiStreamDelta {
    #[serde(default)]
    #[allow(dead_code)]
    role: Option<String>,
    #[serde(default)]
    content: Option<String>,
    #[serde(alias = "thought", alias = "reasoning", default)]
    reasoning_content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<OpenAiToolCall>>,
}

// ── OpenAI API error types ──────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct OpenAiErrorResponse {
    error: OpenAiErrorDetail,
}

#[derive(Debug, Deserialize)]
struct OpenAiErrorDetail {
    message: String,
    #[serde(default)]
    code: Option<String>,
}

// ── Conversion helpers ──────────────────────────────────────────────────────

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

fn message_to_openai(msg: &Message) -> OpenAiMessage {
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

fn openai_message_to_internal(msg: &OpenAiMessage) -> Message {
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

fn tool_to_openai(tool: &ToolDefinition) -> OpenAiTool {
    OpenAiTool {
        tool_type: "function".to_string(),
        function: OpenAiToolFunction {
            name: tool.name.clone(),
            description: tool.description.clone(),
            parameters: tool.parameters.clone(),
        },
    }
}

// ── Provider implementation ─────────────────────────────────────────────────

pub struct OpenAiProvider {
    name: String,
    api_key: String,
    base_url: String,
    client: Client,
}

impl OpenAiProvider {
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

    fn chat_url(&self) -> String {
        format!("{}/chat/completions", self.base_url.trim_end_matches('/'))
    }

    fn models_url(&self) -> String {
        format!("{}/models", self.base_url.trim_end_matches('/'))
    }
}

#[async_trait]
impl LlmProvider for OpenAiProvider {
    fn name(&self) -> &str {
        &self.name
    }

    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse> {
        let tools: Vec<OpenAiTool> = request.tools.iter().map(tool_to_openai).collect();
        let messages: Vec<OpenAiMessage> = request.messages.iter().map(message_to_openai).collect();

        let body = OpenAiRequest {
            model: request.model.clone(),
            messages,
            tools,
            temperature: request.temperature,
            max_tokens: request.max_tokens,
            stream: false,
            stream_options: None,
        };

        debug!("Sending chat request to {}", self.chat_url());
        trace!("Request body: {:?}", body);

        let resp = self
            .client
            .post(&self.chat_url())
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .context("Failed to send chat request")?;

        let status = resp.status();
        if !status.is_success() {
            let error_text = resp.text().await.unwrap_or_default();
            if let Ok(err_resp) = serde_json::from_str::<OpenAiErrorResponse>(&error_text) {
                return Err(anyhow!(
                    "OpenAI API error ({}): {} (code: {})",
                    status,
                    err_resp.error.message,
                    err_resp.error.code.unwrap_or_else(|| "none".to_string())
                ));
            }
            return Err(anyhow!("OpenAI API error ({}): {}", status, error_text));
        }

        let api_resp: OpenAiResponse =
            resp.json().await.context("Failed to parse chat response")?;

        let choice = api_resp
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| anyhow!("OpenAI returned no choices"))?;

        let message = openai_message_to_internal(&choice.message);

        let stop_reason = match choice.finish_reason.as_deref() {
            Some("stop") => Some(crate::types::StopReason::EndTurn),
            Some("tool_calls") => Some(crate::types::StopReason::ToolUse),
            Some("length") => Some(crate::types::StopReason::MaxTokens),
            Some("content_filter") => Some(crate::types::StopReason::ContentFilter),
            Some(other) => Some(crate::types::StopReason::Other(other.to_string())),
            None => None,
        };

        let usage = api_resp
            .usage
            .map(|u| Usage {
                prompt_tokens: u.prompt_tokens,
                completion_tokens: u.completion_tokens,
                total_tokens: u.total_tokens,
            })
            .unwrap_or_default();

        debug!(
            "Chat response received: {} prompt tokens, {} completion tokens",
            usage.prompt_tokens, usage.completion_tokens
        );

        Ok(ChatResponse {
            message,
            usage,
            model: api_resp.model,
            stop_reason,
        })
    }

    async fn chat_stream(&self, request: ChatRequest, tx: mpsc::Sender<StreamEvent>) -> Result<()> {
        let tools: Vec<OpenAiTool> = request.tools.iter().map(tool_to_openai).collect();
        let messages: Vec<OpenAiMessage> = request.messages.iter().map(message_to_openai).collect();

        let body = OpenAiRequest {
            model: request.model.clone(),
            messages,
            tools,
            temperature: request.temperature,
            max_tokens: request.max_tokens,
            stream: true,
            stream_options: Some(StreamOptions {
                include_usage: true,
            }),
        };

        debug!("Sending streaming chat request to {}", self.chat_url());

        let resp = self
            .client
            .post(&self.chat_url())
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .context("Failed to send streaming chat request")?;

        let status = resp.status();
        if !status.is_success() {
            let error_text = resp.text().await.unwrap_or_default();
            if let Ok(err_resp) = serde_json::from_str::<OpenAiErrorResponse>(&error_text) {
                let msg = format!(
                    "OpenAI API error ({}): {} (code: {})",
                    status,
                    err_resp.error.message,
                    err_resp.error.code.unwrap_or_else(|| "none".to_string())
                );
                let _ = tx.send(StreamEvent::Error(msg.clone())).await;
                return Err(anyhow!(msg));
            }
            let msg = format!("OpenAI API error ({}): {}", status, error_text);
            let _ = tx.send(StreamEvent::Error(msg.clone())).await;
            return Err(anyhow!(msg));
        }

        let mut event_stream = resp.bytes_stream().eventsource();

        // Accumulated state for building the final ChatResponse
        let mut full_content = String::new();
        let mut full_reasoning = String::new();
        let mut accumulated_tool_calls: HashMap<u32, ToolCall> = HashMap::new();
        let mut active_tool_indices: HashMap<u32, bool> = HashMap::new();
        let mut model = request.model.clone();
        let mut final_usage = Usage::default();
        let mut stop_reason = None;
        let mut saw_done_sentinel = false;
        let mut saw_finish_reason = false;
        let mut finalize_reason = "stream_eof";
        let mut chunk_count: u64 = 0;

        'stream_loop: while let Some(event_result) = event_stream.next().await {
            let event = match event_result {
                Ok(ev) => ev,
                Err(e) => {
                    let msg = format!("SSE stream error: {}", e);
                    error!("{}", msg);
                    let _ = tx.send(StreamEvent::Error(msg)).await;
                    // Stream error — finalize with what we have and break
                    finalize_reason = "sse_error";
                    break;
                }
            };

            let data = event.data;
            chunk_count += 1;

            // Check for the stream termination sentinel
            if data.trim() == "[DONE]" {
                debug!("Stream completed with [DONE]");
                saw_done_sentinel = true;
                finalize_reason = "done_sentinel";
                break;
            }

            let chunk: OpenAiStreamChunk = match serde_json::from_str(&data) {
                Ok(c) => c,
                Err(e) => {
                    warn!("Failed to parse stream chunk: {} (data: {})", e, data);
                    continue;
                }
            };

            // Log chunk for debugging (trace level)
            trace!(
                "Received chunk: choices={}, has_usage={}",
                chunk.choices.len(),
                chunk.usage.is_some()
            );

            if let Some(m) = &chunk.model {
                model = m.clone();
            }

            if let Some(u) = &chunk.usage {
                let prompt = if u.prompt_tokens == 0 && u.total_tokens > u.completion_tokens {
                    u.total_tokens - u.completion_tokens // infer from total
                } else {
                    u.prompt_tokens
                };
                final_usage = Usage {
                    prompt_tokens: prompt,
                    completion_tokens: u.completion_tokens,
                    total_tokens: u.total_tokens,
                };
            }

            for choice in &chunk.choices {
                let delta = &choice.delta;

                // Handle reasoning content (e.g. DeepSeek-R1)
                if let Some(reasoning) = &delta.reasoning_content {
                    if !reasoning.is_empty() {
                        full_reasoning.push_str(reasoning);
                        if tx
                            .send(StreamEvent::ReasoningDelta(reasoning.clone()))
                            .await
                            .is_err()
                        {
                            debug!("Stream receiver dropped, stopping");
                            return Ok(());
                        }
                    }
                }

                // Handle text content
                if let Some(content) = &delta.content {
                    if !content.is_empty() {
                        full_content.push_str(content);
                        if tx
                            .send(StreamEvent::TextDelta(content.clone()))
                            .await
                            .is_err()
                        {
                            debug!("Stream receiver dropped, stopping");
                            return Ok(());
                        }
                    }
                }

                // Handle tool calls
                if let Some(tool_calls) = &delta.tool_calls {
                    for tc in tool_calls {
                        let index = tc.index.unwrap_or(0);

                        // If we see an id, this is a new tool call starting
                        if let Some(id) = &tc.id {
                            let name = tc.function.name.clone().unwrap_or_default();

                            accumulated_tool_calls.insert(
                                index,
                                ToolCall {
                                    id: id.clone(),
                                    name: name.clone(),
                                    arguments: String::new(),
                                },
                            );
                            active_tool_indices.insert(index, true);

                            if tx
                                .send(StreamEvent::ToolCallStart {
                                    id: id.clone(),
                                    name,
                                })
                                .await
                                .is_err()
                            {
                                debug!("Stream receiver dropped, stopping");
                                return Ok(());
                            }
                        }

                        // Accumulate arguments
                        if let Some(args) = &tc.function.arguments {
                            if !args.is_empty() {
                                if let Some(tool_call) = accumulated_tool_calls.get_mut(&index) {
                                    tool_call.arguments.push_str(args);

                                    let id = tool_call.id.clone();
                                    if tx
                                        .send(StreamEvent::ToolCallDelta {
                                            id,
                                            arguments: args.clone(),
                                        })
                                        .await
                                        .is_err()
                                    {
                                        debug!("Stream receiver dropped, stopping");
                                        return Ok(());
                                    }
                                }
                            }
                        }
                    }
                }

                // If there's a finish_reason, end any active tool calls and exit loop
                if let Some(reason) = &choice.finish_reason {
                    saw_finish_reason = true;
                    finalize_reason = "finish_reason";
                    debug!("Received finish_reason: {}", reason);

                    stop_reason = match reason.as_str() {
                        "stop" => Some(crate::types::StopReason::EndTurn),
                        "tool_calls" => Some(crate::types::StopReason::ToolUse),
                        "length" => Some(crate::types::StopReason::MaxTokens),
                        "content_filter" => Some(crate::types::StopReason::ContentFilter),
                        _ => Some(crate::types::StopReason::Other(reason.clone())),
                    };

                    for (&index, active) in &active_tool_indices {
                        if *active {
                            if let Some(tc) = accumulated_tool_calls.get(&index) {
                                if tx
                                    .send(StreamEvent::ToolCallEnd { id: tc.id.clone() })
                                    .await
                                    .is_err()
                                {
                                    debug!("Stream receiver dropped, stopping");
                                    return Ok(());
                                }
                            }
                        }
                    }
                    active_tool_indices.clear();
                    // API signaled completion via finish_reason - break the stream loop to avoid waiting for [DONE]
                    // This handles APIs like DashScope/Aliyun that don't send the [DONE] sentinel
                    break 'stream_loop;
                }
            }
        }

        if !saw_done_sentinel && !saw_finish_reason {
            warn!(
                "OpenAI stream ended without [DONE] or finish_reason; finalizing from partial state (reason={}, chunks={})",
                finalize_reason,
                chunk_count
            );
        }

        // Ensure any still-active tool calls are ended before finalization.
        for (&index, active) in &active_tool_indices {
            if *active {
                if let Some(tc) = accumulated_tool_calls.get(&index) {
                    let _ = tx
                        .send(StreamEvent::ToolCallEnd { id: tc.id.clone() })
                        .await;
                }
            }
        }
        active_tool_indices.clear();

        // Build the final tool_calls list sorted by index
        let mut tool_calls_sorted: Vec<(u32, ToolCall)> =
            accumulated_tool_calls.into_iter().collect();
        tool_calls_sorted.sort_by_key(|(idx, _)| *idx);
        let final_tool_calls: Vec<ToolCall> =
            tool_calls_sorted.into_iter().map(|(_, tc)| tc).collect();

        let content = if full_content.is_empty() {
            None
        } else {
            Some(full_content)
        };

        let reasoning = if full_reasoning.is_empty() {
            None
        } else {
            Some(full_reasoning)
        };

        let mut blocks = Vec::new();
        if let Some(ref r) = reasoning {
            blocks.push(crate::types::ContentBlock::Thinking {
                thinking: r.clone(),
                signature: None,
            });
        }
        if let Some(ref t) = content {
            blocks.push(crate::types::ContentBlock::Text { text: t.clone() });
        }

        let final_message = Message {
            role: Role::Assistant,
            content,
            reasoning,
            content_blocks: blocks,
            tool_calls: final_tool_calls,
            tool_call_id: None,
            images: Vec::new(),
        }
        .normalized();

        let response = ChatResponse {
            message: final_message,
            usage: final_usage,
            model,
            stop_reason,
        };

        let _ = tx.send(StreamEvent::Done(response)).await;
        debug!(
            "OpenAI stream finalized (reason={}, saw_done_sentinel={}, saw_finish_reason={}, chunks={})",
            finalize_reason,
            saw_done_sentinel,
            saw_finish_reason,
            chunk_count
        );

        Ok(())
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        debug!("Fetching models from {}", self.models_url());

        let resp = self
            .client
            .get(&self.models_url())
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .await
            .context("Failed to fetch models")?;

        let status = resp.status();
        if !status.is_success() {
            let error_text = resp.text().await.unwrap_or_default();
            if let Ok(err_resp) = serde_json::from_str::<OpenAiErrorResponse>(&error_text) {
                return Err(anyhow!(
                    "OpenAI API error ({}): {} (code: {})",
                    status,
                    err_resp.error.message,
                    err_resp.error.code.unwrap_or_else(|| "none".to_string())
                ));
            }
            return Err(anyhow!("OpenAI API error ({}): {}", status, error_text));
        }

        let models_resp: OpenAiModelsResponse = resp
            .json()
            .await
            .context("Failed to parse models response")?;

        let models = models_resp
            .data
            .into_iter()
            .map(|m| ModelInfo {
                id: m.id.clone(),
                name: m.id,
                provider: self.name.clone(),
            })
            .collect();

        Ok(models)
    }
}
