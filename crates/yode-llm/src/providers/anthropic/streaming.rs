use super::request_conversion::anthropic_usage_to_usage;
use anyhow::{anyhow, Context, Result};
use eventsource_stream::Eventsource;
use futures::StreamExt;
use tokio::sync::mpsc;
use tracing::{debug, error, warn};

use crate::types::{ChatRequest, ChatResponse, Message, Role, StreamEvent, ToolCall, Usage};

use super::types::{
    AnthropicErrorResponse, AnthropicRequest, AnthropicStreamEvent, AnthropicThinkingConfig,
    ContentBlockDelta, ContentBlockStart,
};
use super::AnthropicProvider;

impl AnthropicProvider {
    pub(super) async fn send_chat_stream_request(
        &self,
        request: ChatRequest,
        tx: mpsc::Sender<StreamEvent>,
    ) -> Result<()> {
        let (system, messages) = Self::convert_messages(&request.messages);
        let tools = Self::convert_tools(&request.tools);
        let max_tokens = request.max_tokens.unwrap_or(4096);

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
            stream: true,
        };

        debug!(
            "Sending Anthropic streaming request to {}",
            self.messages_url()
        );

        let body_json = serde_json::to_string(&body).context("Failed to serialize request")?;

        let resp = self
            .client
            .post(self.messages_url())
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
                let msg = format!(
                    "Anthropic API error ({}): {}",
                    status, err_resp.error.message
                );
                let _ = tx.send(StreamEvent::Error(msg.clone())).await;
                return Err(anyhow!(msg));
            }
            let msg = format!("Anthropic API error ({}): {}", status, error_text);
            let _ = tx.send(StreamEvent::Error(msg.clone())).await;
            return Err(anyhow!(msg));
        }

        let mut event_stream = resp.bytes_stream().eventsource();
        let mut content_blocks: std::collections::BTreeMap<u32, crate::types::ContentBlock> =
            std::collections::BTreeMap::new();
        let mut tool_calls: Vec<ToolCall> = Vec::new();
        let mut tool_ids_by_index: std::collections::HashMap<u32, String> =
            std::collections::HashMap::new();
        let mut model = request.model.clone();
        let mut final_usage = Usage::default();
        let mut stop_reason = None;
        let mut saw_message_stop = false;
        let mut finalize_reason = "stream_eof";
        let mut event_count: u64 = 0;
        let mut first_block_text = String::new();
        let mut first_block_is_thinking = false;

        while let Some(event_result) = event_stream.next().await {
            let event = match event_result {
                Ok(ev) => ev,
                Err(e) => {
                    let msg = format!("SSE stream error: {}", e);
                    error!("{}", msg);
                    let _ = tx.send(StreamEvent::Error(msg)).await;
                    finalize_reason = "sse_error";
                    break;
                }
            };

            let data = event.data;

            let stream_event: AnthropicStreamEvent = match serde_json::from_str(&data) {
                Ok(e) => e,
                Err(e) => {
                    warn!(
                        "Failed to parse Anthropic stream event: {} (data: {})",
                        e, data
                    );
                    continue;
                }
            };

            event_count += 1;
            tracing::trace!("Received SSE event: {}", stream_event.event_type());

            match stream_event {
                AnthropicStreamEvent::MessageStart { message } => {
                    model = message.model;
                    if let Some(u) = message.usage {
                        final_usage = anthropic_usage_to_usage(&u);
                        let _ = tx.send(StreamEvent::UsageUpdate(final_usage.clone())).await;
                    }
                }
                AnthropicStreamEvent::ContentBlockStart {
                    index,
                    content_block,
                } => match content_block {
                    ContentBlockStart::Text { text } => {
                        if index == 0 {
                            first_block_text = text.clone();
                        }

                        content_blocks.insert(
                            index,
                            crate::types::ContentBlock::Text { text: text.clone() },
                        );
                        if !text.is_empty() {
                            let _ = tx.send(StreamEvent::TextDelta(text)).await;
                        }
                    }
                    ContentBlockStart::Thinking {
                        thinking,
                        signature,
                    } => {
                        first_block_is_thinking = true;
                        content_blocks.insert(
                            index,
                            crate::types::ContentBlock::Thinking {
                                thinking: thinking.clone(),
                                signature: signature.clone(),
                            },
                        );
                        if !thinking.is_empty() {
                            let _ = tx.send(StreamEvent::ReasoningDelta(thinking)).await;
                        }
                    }
                    ContentBlockStart::ToolUse { id, name } => {
                        tool_ids_by_index.insert(index, id.clone());
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
                        if index == 0 {
                            first_block_text.push_str(&text);
                        }

                        if index == 0 && !first_block_is_thinking {
                            let trimmed = first_block_text.trim();
                            let is_thinking = trimmed.starts_with("用户")
                                || trimmed.starts_with("我应该")
                                || trimmed.starts_with("Thinking")
                                || trimmed.starts_with("Let me");

                            if is_thinking {
                                first_block_is_thinking = true;
                                content_blocks.insert(
                                    index,
                                    crate::types::ContentBlock::Thinking {
                                        thinking: first_block_text.clone(),
                                        signature: None,
                                    },
                                );
                                if tx.send(StreamEvent::ReasoningDelta(text)).await.is_err() {
                                    return Ok(());
                                }
                                continue;
                            }
                        }

                        if first_block_is_thinking && index == 0 {
                            if let Some(crate::types::ContentBlock::Thinking {
                                thinking: t, ..
                            }) = content_blocks.get_mut(&index)
                            {
                                t.push_str(&text);
                            }
                            if tx.send(StreamEvent::ReasoningDelta(text)).await.is_err() {
                                return Ok(());
                            }
                        } else {
                            if let Some(crate::types::ContentBlock::Text { text: t }) =
                                content_blocks.get_mut(&index)
                            {
                                t.push_str(&text);
                            }
                            if tx.send(StreamEvent::TextDelta(text)).await.is_err() {
                                return Ok(());
                            }
                        }
                    }
                    ContentBlockDelta::ThinkingDelta {
                        thinking,
                        signature,
                    } => {
                        first_block_is_thinking = true;
                        if let Some(crate::types::ContentBlock::Thinking {
                            thinking: t,
                            signature: s,
                        }) = content_blocks.get_mut(&index)
                        {
                            t.push_str(&thinking);
                            if signature.is_some() {
                                *s = signature.clone();
                            }
                        }
                        if tx
                            .send(StreamEvent::ReasoningDelta(thinking))
                            .await
                            .is_err()
                        {
                            return Ok(());
                        }
                    }
                    ContentBlockDelta::InputJsonDelta { partial_json } => {
                        if let Some(tool_id) = tool_ids_by_index.get(&index) {
                            if let Some(tc) = tool_calls.iter_mut().find(|t| t.id == *tool_id) {
                                tc.arguments.push_str(&partial_json);
                                let _ = tx
                                    .send(StreamEvent::ToolCallDelta {
                                        id: tc.id.clone(),
                                        arguments: partial_json,
                                    })
                                    .await;
                            }
                        } else if let Some(tc) = tool_calls.last_mut() {
                            tc.arguments.push_str(&partial_json);
                            let _ = tx
                                .send(StreamEvent::ToolCallDelta {
                                    id: tc.id.clone(),
                                    arguments: partial_json,
                                })
                                .await;
                        }
                    }
                    ContentBlockDelta::Unknown => {}
                },
                AnthropicStreamEvent::ContentBlockStop { index } => {
                    if let Some(tool_id) = tool_ids_by_index.remove(&index) {
                        let _ = tx.send(StreamEvent::ToolCallEnd { id: tool_id }).await;
                    }
                }
                AnthropicStreamEvent::MessageDelta { delta, usage } => {
                    if let Some(reason) = delta.stop_reason {
                        stop_reason = match reason.as_str() {
                            "end_turn" => Some(crate::types::StopReason::EndTurn),
                            "tool_use" => Some(crate::types::StopReason::ToolUse),
                            "max_tokens" => Some(crate::types::StopReason::MaxTokens),
                            "stop_sequence" => Some(crate::types::StopReason::StopSequence),
                            _ => Some(crate::types::StopReason::Other(reason)),
                        };
                    }

                    if let Some(u) = usage {
                        final_usage = anthropic_usage_to_usage(&u);
                        let _ = tx.send(StreamEvent::UsageUpdate(final_usage.clone())).await;
                    }
                }
                AnthropicStreamEvent::MessageStop {} => {
                    debug!("Received message_stop - finalizing stream");
                    saw_message_stop = true;
                    finalize_reason = "message_stop";
                    break;
                }
                AnthropicStreamEvent::Ping {} => {}
                AnthropicStreamEvent::Error { error } => {
                    let msg = format!("Anthropic stream error: {}", error.message);
                    error!("{}", msg);
                    let _ = tx.send(StreamEvent::Error(msg)).await;
                    finalize_reason = "stream_error_event";
                    break;
                }
                AnthropicStreamEvent::Unknown => {
                    tracing::debug!("Received unknown SSE event type from API - data: {}", data);
                }
            }
        }

        if !saw_message_stop {
            warn!(
                "Anthropic stream ended without message_stop; finalizing from partial state (reason={}, events={})",
                finalize_reason,
                event_count
            );
        }

        let dangling_tool_count = tool_ids_by_index.len();
        for (_, tool_id) in tool_ids_by_index.drain() {
            let _ = tx.send(StreamEvent::ToolCallEnd { id: tool_id }).await;
        }
        if dangling_tool_count > 0 {
            debug!(
                "Closed {} dangling Anthropic tool blocks during finalization",
                dangling_tool_count
            );
        }

        let final_content_blocks: Vec<crate::types::ContentBlock> =
            content_blocks.into_values().collect();
        let mut full_text = String::new();
        let mut full_reasoning = String::new();

        for block in &final_content_blocks {
            match block {
                crate::types::ContentBlock::Text { text } => full_text.push_str(text),
                crate::types::ContentBlock::Thinking { thinking, .. } => {
                    full_reasoning.push_str(thinking)
                }
            }
        }

        let final_message = Message {
            role: Role::Assistant,
            content: if full_text.is_empty() {
                None
            } else {
                Some(full_text)
            },
            reasoning: if full_reasoning.is_empty() {
                None
            } else {
                Some(full_reasoning)
            },
            content_blocks: final_content_blocks,
            tool_calls,
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
            "Sent StreamEvent::Done - stream complete (reason={}, saw_message_stop={}, events={})",
            finalize_reason, saw_message_stop, event_count
        );

        Ok(())
    }
}
