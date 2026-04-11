use std::collections::HashMap;

use tokio::sync::mpsc;
use tracing::{debug, warn};

use crate::providers::streaming_shared::{
    append_tool_call_delta, emit_done_event, emit_tool_call_end, emit_tool_call_start,
    map_stop_reason,
};
use crate::types::{Message, StopReason, StreamEvent, ToolCall, Usage};

use super::types::OpenAiStreamChunk;

pub(super) struct OpenAiStreamState {
    pub(super) full_content: String,
    pub(super) full_reasoning: String,
    pub(super) accumulated_tool_calls: HashMap<u32, ToolCall>,
    pub(super) active_tool_indices: HashMap<u32, bool>,
    pub(super) model: String,
    pub(super) final_usage: Usage,
    pub(super) stop_reason: Option<StopReason>,
    pub(super) saw_done_sentinel: bool,
    pub(super) saw_finish_reason: bool,
    pub(super) finalize_reason: &'static str,
    pub(super) chunk_count: u64,
}

impl OpenAiStreamState {
    pub(super) fn new(model: String) -> Self {
        Self {
            full_content: String::new(),
            full_reasoning: String::new(),
            accumulated_tool_calls: HashMap::new(),
            active_tool_indices: HashMap::new(),
            model,
            final_usage: Usage::default(),
            stop_reason: None,
            saw_done_sentinel: false,
            saw_finish_reason: false,
            finalize_reason: "stream_eof",
            chunk_count: 0,
        }
    }
}

pub(super) async fn handle_stream_chunk(
    state: &mut OpenAiStreamState,
    chunk: OpenAiStreamChunk,
    tx: &mpsc::Sender<StreamEvent>,
) -> bool {
    state.chunk_count += 1;
    if let Some(model) = &chunk.model {
        state.model = model.clone();
    }
    if let Some(usage) = &chunk.usage {
        let prompt = if usage.prompt_tokens == 0 && usage.total_tokens > usage.completion_tokens {
            usage.total_tokens - usage.completion_tokens
        } else {
            usage.prompt_tokens
        };
        state.final_usage = Usage {
            prompt_tokens: prompt,
            completion_tokens: usage.completion_tokens,
            total_tokens: usage.total_tokens,
            cache_write_tokens: 0,
            cache_read_tokens: usage
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
                state.full_reasoning.push_str(reasoning);
                if tx
                    .send(StreamEvent::ReasoningDelta(reasoning.clone()))
                    .await
                    .is_err()
                {
                    return true;
                }
            }
        }

        if let Some(content) = &delta.content {
            if !content.is_empty() {
                state.full_content.push_str(content);
                if tx.send(StreamEvent::TextDelta(content.clone())).await.is_err() {
                    return true;
                }
            }
        }

        if let Some(tool_calls) = &delta.tool_calls {
            for tool_call in tool_calls {
                let index = tool_call.index.unwrap_or(0);
                if let Some(id) = &tool_call.id {
                    let name = tool_call.function.name.clone().unwrap_or_default();
                    state.accumulated_tool_calls.insert(
                        index,
                        ToolCall {
                            id: id.clone(),
                            name: name.clone(),
                            arguments: String::new(),
                        },
                    );
                    state.active_tool_indices.insert(index, true);
                    emit_tool_call_start(tx, id.clone(), name).await;
                }
                if let Some(arguments) = &tool_call.function.arguments {
                    if !arguments.is_empty() {
                        if let Some(accumulated) = state.accumulated_tool_calls.get_mut(&index) {
                            if !append_tool_call_delta(tx, accumulated, arguments).await {
                                return true;
                            }
                        }
                    }
                }
            }
        }

        if let Some(reason) = &choice.finish_reason {
            state.saw_finish_reason = true;
            state.finalize_reason = "finish_reason";
            debug!("Received finish_reason: {}", reason);
            state.stop_reason = Some(map_stop_reason(reason));
            for (&index, active) in &state.active_tool_indices {
                if *active {
                    if let Some(tool_call) = state.accumulated_tool_calls.get(&index) {
                        emit_tool_call_end(tx, tool_call.id.clone()).await;
                    }
                }
            }
            state.active_tool_indices.clear();
            return true;
        }
    }

    false
}

pub(super) async fn finalize_stream(
    mut state: OpenAiStreamState,
    tx: &mpsc::Sender<StreamEvent>,
) {
    if !state.saw_done_sentinel && !state.saw_finish_reason {
        warn!(
            "OpenAI stream ended without [DONE] or finish_reason; finalizing from partial state (reason={}, chunks={})",
            state.finalize_reason,
            state.chunk_count
        );
    }

    for (&index, active) in &state.active_tool_indices {
        if *active {
            if let Some(tool_call) = state.accumulated_tool_calls.get(&index) {
                emit_tool_call_end(tx, tool_call.id.clone()).await;
            }
        }
    }
    state.active_tool_indices.clear();

    let mut tool_calls_sorted: Vec<(u32, ToolCall)> = state.accumulated_tool_calls.into_iter().collect();
    tool_calls_sorted.sort_by_key(|(index, _)| *index);
    let tool_calls = tool_calls_sorted
        .into_iter()
        .map(|(_, tool_call)| tool_call)
        .collect();
    let message = Message::assistant_with_reasoning_and_tools(
        (!state.full_content.is_empty()).then_some(state.full_content),
        (!state.full_reasoning.is_empty()).then_some(state.full_reasoning),
        tool_calls,
    );
    emit_done_event(tx, message, state.final_usage, state.model, state.stop_reason).await;
}
