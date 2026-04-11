use std::collections::{BTreeMap, HashMap};

use anyhow::Result;
use tokio::sync::mpsc;
use tracing::{debug, error, warn};

use crate::providers::streaming_shared::{
    append_tool_call_delta, emit_done_event, emit_stream_error, emit_tool_call_end,
    emit_tool_call_start, emit_usage_update, map_stop_reason,
};
use crate::types::{Message, StopReason, StreamEvent, ToolCall, Usage};

use super::request_conversion::anthropic_usage_to_usage;
use super::types::{AnthropicStreamEvent, ContentBlockDelta, ContentBlockStart};

pub(super) struct AnthropicStreamState {
    pub(super) content_blocks: BTreeMap<u32, crate::types::ContentBlock>,
    pub(super) tool_calls: Vec<ToolCall>,
    pub(super) tool_ids_by_index: HashMap<u32, String>,
    pub(super) model: String,
    pub(super) final_usage: Usage,
    pub(super) stop_reason: Option<StopReason>,
    pub(super) saw_message_stop: bool,
    pub(super) finalize_reason: &'static str,
    pub(super) event_count: u64,
    pub(super) first_block_text: String,
    pub(super) first_block_is_thinking: bool,
}

impl AnthropicStreamState {
    pub(super) fn new(model: String) -> Self {
        Self {
            content_blocks: BTreeMap::new(),
            tool_calls: Vec::new(),
            tool_ids_by_index: HashMap::new(),
            model,
            final_usage: Usage::default(),
            stop_reason: None,
            saw_message_stop: false,
            finalize_reason: "stream_eof",
            event_count: 0,
            first_block_text: String::new(),
            first_block_is_thinking: false,
        }
    }
}

pub(super) async fn handle_stream_event(
    state: &mut AnthropicStreamState,
    stream_event: AnthropicStreamEvent,
    data: &str,
    tx: &mpsc::Sender<StreamEvent>,
) -> Result<bool> {
    state.event_count += 1;
    tracing::trace!("Received SSE event: {}", stream_event.event_type());

    match stream_event {
        AnthropicStreamEvent::MessageStart { message } => {
            state.model = message.model;
            if let Some(usage) = message.usage {
                state.final_usage = anthropic_usage_to_usage(&usage);
                emit_usage_update(tx, &state.final_usage).await;
            }
        }
        AnthropicStreamEvent::ContentBlockStart {
            index,
            content_block,
        } => match content_block {
            ContentBlockStart::Text { text } => {
                if index == 0 {
                    state.first_block_text = text.clone();
                }

                state.content_blocks.insert(
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
                state.first_block_is_thinking = true;
                state.content_blocks.insert(
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
                state.tool_ids_by_index.insert(index, id.clone());
                state.tool_calls.push(ToolCall {
                    id: id.clone(),
                    name: name.clone(),
                    arguments: String::new(),
                });
                emit_tool_call_start(tx, id, name).await;
            }
            ContentBlockStart::Unknown => {}
        },
        AnthropicStreamEvent::ContentBlockDelta { index, delta } => match delta {
            ContentBlockDelta::TextDelta { text } => {
                handle_text_delta(state, index, text, tx).await?;
            }
            ContentBlockDelta::ThinkingDelta {
                thinking,
                signature,
            } => {
                state.first_block_is_thinking = true;
                if let Some(crate::types::ContentBlock::Thinking {
                    thinking: current,
                    signature: current_signature,
                }) = state.content_blocks.get_mut(&index)
                {
                    current.push_str(&thinking);
                    if signature.is_some() {
                        *current_signature = signature.clone();
                    }
                }
                if tx
                    .send(StreamEvent::ReasoningDelta(thinking))
                    .await
                    .is_err()
                {
                    return Ok(true);
                }
            }
            ContentBlockDelta::InputJsonDelta { partial_json } => {
                if let Some(tool_id) = state.tool_ids_by_index.get(&index) {
                    if let Some(tool_call) =
                        state.tool_calls.iter_mut().find(|call| call.id == *tool_id)
                    {
                        let _ = append_tool_call_delta(tx, tool_call, &partial_json).await;
                    }
                } else if let Some(tool_call) = state.tool_calls.last_mut() {
                    let _ = append_tool_call_delta(tx, tool_call, &partial_json).await;
                }
            }
            ContentBlockDelta::Unknown => {}
        },
        AnthropicStreamEvent::ContentBlockStop { index } => {
            if let Some(tool_id) = state.tool_ids_by_index.remove(&index) {
                emit_tool_call_end(tx, tool_id).await;
            }
        }
        AnthropicStreamEvent::MessageDelta { delta, usage } => {
            if let Some(reason) = delta.stop_reason {
                state.stop_reason = Some(map_stop_reason(&reason));
            }

            if let Some(usage) = usage {
                state.final_usage = anthropic_usage_to_usage(&usage);
                emit_usage_update(tx, &state.final_usage).await;
            }
        }
        AnthropicStreamEvent::MessageStop {} => {
            debug!("Received message_stop - finalizing stream");
            state.saw_message_stop = true;
            state.finalize_reason = "message_stop";
            return Ok(true);
        }
        AnthropicStreamEvent::Ping {} => {}
        AnthropicStreamEvent::Error { error: err } => {
            let msg = format!("Anthropic stream error: {}", err.message);
            error!("{}", msg);
            emit_stream_error(tx, msg).await;
            state.finalize_reason = "stream_error_event";
            return Ok(true);
        }
        AnthropicStreamEvent::Unknown => {
            tracing::debug!("Received unknown SSE event type from API - data: {}", data);
        }
    }

    Ok(false)
}

async fn handle_text_delta(
    state: &mut AnthropicStreamState,
    index: u32,
    text: String,
    tx: &mpsc::Sender<StreamEvent>,
) -> Result<()> {
    if index == 0 {
        state.first_block_text.push_str(&text);
    }

    if index == 0 && !state.first_block_is_thinking {
        let trimmed = state.first_block_text.trim();
        let is_thinking = trimmed.starts_with("用户")
            || trimmed.starts_with("我应该")
            || trimmed.starts_with("Thinking")
            || trimmed.starts_with("Let me");

        if is_thinking {
            state.first_block_is_thinking = true;
            state.content_blocks.insert(
                index,
                crate::types::ContentBlock::Thinking {
                    thinking: state.first_block_text.clone(),
                    signature: None,
                },
            );
            let _ = tx.send(StreamEvent::ReasoningDelta(text)).await;
            return Ok(());
        }
    }

    if state.first_block_is_thinking && index == 0 {
        if let Some(crate::types::ContentBlock::Thinking { thinking, .. }) =
            state.content_blocks.get_mut(&index)
        {
            thinking.push_str(&text);
        }
        let _ = tx.send(StreamEvent::ReasoningDelta(text)).await;
    } else {
        if let Some(crate::types::ContentBlock::Text { text: current }) =
            state.content_blocks.get_mut(&index)
        {
            current.push_str(&text);
        }
        let _ = tx.send(StreamEvent::TextDelta(text)).await;
    }
    Ok(())
}

pub(super) async fn finalize_stream(
    mut state: AnthropicStreamState,
    tx: &mpsc::Sender<StreamEvent>,
) -> Result<()> {
    if !state.saw_message_stop {
        warn!(
            "Anthropic stream ended without message_stop; finalizing from partial state (reason={}, events={})",
            state.finalize_reason,
            state.event_count
        );
    }

    let dangling_tool_count = state.tool_ids_by_index.len();
    for (_, tool_id) in state.tool_ids_by_index.drain() {
        emit_tool_call_end(tx, tool_id).await;
    }
    if dangling_tool_count > 0 {
        debug!(
            "Closed {} dangling Anthropic tool blocks during finalization",
            dangling_tool_count
        );
    }

    let final_content_blocks: Vec<crate::types::ContentBlock> =
        state.content_blocks.into_values().collect();
    let final_message = Message::assistant_from_blocks(final_content_blocks, state.tool_calls);

    emit_done_event(tx, final_message, state.final_usage, state.model, state.stop_reason).await;
    debug!(
        "Sent StreamEvent::Done - stream complete (reason={}, saw_message_stop={}, events={})",
        state.finalize_reason, state.saw_message_stop, state.event_count
    );

    Ok(())
}
