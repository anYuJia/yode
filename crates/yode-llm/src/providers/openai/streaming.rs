use std::collections::HashMap;

use anyhow::{anyhow, Context, Result};
use eventsource_stream::Eventsource;
use futures::StreamExt;
use tokio::sync::mpsc;
use tracing::{debug, error, trace, warn};

use crate::types::{ChatRequest, Message, StreamEvent, ToolCall, Usage};

use super::conversion::{message_to_openai, tool_to_openai};
use super::types::{
    OpenAiErrorResponse, OpenAiMessage, OpenAiRequest, OpenAiStreamChunk, OpenAiTool,
    StreamOptions,
};
use super::OpenAiProvider;

impl OpenAiProvider {
    pub(super) async fn send_chat_stream_request(
        &self,
        request: ChatRequest,
        tx: mpsc::Sender<StreamEvent>,
    ) -> Result<()> {
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
            .post(self.chat_url())
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
                    finalize_reason = "sse_error";
                    break;
                }
            };

            let data = event.data;
            chunk_count += 1;

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
                    u.total_tokens - u.completion_tokens
                } else {
                    u.prompt_tokens
                };
                final_usage = Usage {
                    prompt_tokens: prompt,
                    completion_tokens: u.completion_tokens,
                    total_tokens: u.total_tokens,
                    cache_write_tokens: 0,
                    cache_read_tokens: u
                        .prompt_tokens_details
                        .as_ref()
                        .map(|details| details.cached_tokens)
                        .unwrap_or(0),
                };
            }

            for choice in &chunk.choices {
                let delta = &choice.delta;

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

                if let Some(tool_calls) = &delta.tool_calls {
                    for tc in tool_calls {
                        let index = tc.index.unwrap_or(0);

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

        let mut tool_calls_sorted: Vec<(u32, ToolCall)> =
            accumulated_tool_calls.into_iter().collect();
        tool_calls_sorted.sort_by_key(|(idx, _)| *idx);
        let final_tool_calls: Vec<ToolCall> =
            tool_calls_sorted.into_iter().map(|(_, tc)| tc).collect();

        let final_message = Message::assistant_with_reasoning_and_tools(
            if full_content.is_empty() {
                None
            } else {
                Some(full_content)
            },
            if full_reasoning.is_empty() {
                None
            } else {
                Some(full_reasoning)
            },
            final_tool_calls,
        );

        let _ = tx
            .send(crate::types::stream_done(
                final_message,
                final_usage,
                model,
                stop_reason,
            ))
            .await;
        debug!(
            "OpenAI stream finalized (reason={}, saw_done_sentinel={}, saw_finish_reason={}, chunks={})",
            finalize_reason,
            saw_done_sentinel,
            saw_finish_reason,
            chunk_count
        );

        Ok(())
    }
}
