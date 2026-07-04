use std::collections::HashMap;

use tokio::sync::mpsc;
use tracing::{debug, warn};

use crate::providers::streaming_shared::{
    append_tool_call_delta, emit_done_event, emit_tool_call_end, emit_tool_call_start,
    map_stop_reason,
};
use crate::types::{Message, StopReason, StreamEvent, ToolCall, Usage};

use super::types::OpenAiStreamChunk;

const UNKNOWN_OPENAI_TOOL_NAME: &str = "unknown_tool";

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
            cache_deleted_tokens: 0,
        };
    }

    for choice in &chunk.choices {
        let delta = &choice.delta;

        if let Some(reasoning) = &delta.reasoning_content {
            if !reasoning.is_empty() {
                let Some(reasoning_delta) = streaming_delta(&state.full_reasoning, reasoning)
                else {
                    continue;
                };
                state.full_reasoning.push_str(&reasoning_delta);
                if tx
                    .send(StreamEvent::ReasoningDelta(reasoning_delta))
                    .await
                    .is_err()
                {
                    return true;
                }
            }
        }

        if let Some(content) = &delta.content {
            if !content.is_empty() {
                let Some(content_delta) = streaming_delta(&state.full_content, content) else {
                    continue;
                };
                state.full_content.push_str(&content_delta);
                if tx
                    .send(StreamEvent::TextDelta(content_delta))
                    .await
                    .is_err()
                {
                    return true;
                }
            }
        }

        if let Some(tool_calls) = &delta.tool_calls {
            for tool_call in tool_calls {
                let index = tool_call.index.unwrap_or(0);
                if let Some(id) = &tool_call.id {
                    let name = tool_call
                        .function
                        .name
                        .clone()
                        .unwrap_or_else(|| UNKNOWN_OPENAI_TOOL_NAME.to_string());
                    let accumulated =
                        state
                            .accumulated_tool_calls
                            .entry(index)
                            .or_insert_with(|| ToolCall {
                                id: id.clone(),
                                name: name.clone(),
                                arguments: String::new(),
                            });
                    accumulated.id = id.clone();
                    accumulated.name = name.clone();
                    state.active_tool_indices.insert(index, true);
                    emit_tool_call_start(tx, id.clone(), name).await;
                }
                if let Some(arguments) = &tool_call.function.arguments {
                    if !arguments.is_empty() {
                        let accumulated =
                            state
                                .accumulated_tool_calls
                                .entry(index)
                                .or_insert_with(|| ToolCall {
                                    id: format!("openai_tc_{index}"),
                                    name: tool_call
                                        .function
                                        .name
                                        .clone()
                                        .unwrap_or_else(|| UNKNOWN_OPENAI_TOOL_NAME.to_string()),
                                    arguments: String::new(),
                                });
                        if !append_tool_call_delta(tx, accumulated, arguments).await {
                            return true;
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

fn streaming_delta(current: &str, incoming: &str) -> Option<String> {
    if incoming.is_empty() || current.ends_with(incoming) {
        return None;
    }
    if current.is_empty() {
        return Some(incoming.to_string());
    }
    if let Some(delta) = incoming.strip_prefix(current) {
        return Some(delta.to_string());
    }

    let max_overlap = current.len().min(incoming.len());
    for overlap in (1..=max_overlap).rev() {
        if !current.is_char_boundary(current.len() - overlap) || !incoming.is_char_boundary(overlap)
        {
            continue;
        }
        if current.ends_with(&incoming[..overlap]) {
            return Some(incoming[overlap..].to_string());
        }
    }

    Some(incoming.to_string())
}

pub(super) async fn finalize_stream(mut state: OpenAiStreamState, tx: &mpsc::Sender<StreamEvent>) {
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

    let mut tool_calls_sorted: Vec<(u32, ToolCall)> =
        state.accumulated_tool_calls.into_iter().collect();
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
    emit_done_event(
        tx,
        message,
        state.final_usage,
        state.model,
        state.stop_reason,
    )
    .await;
}

#[cfg(test)]
mod tests {
    use tokio::sync::mpsc;

    use super::{finalize_stream, handle_stream_chunk, OpenAiStreamState};
    use crate::providers::openai::types::{
        OpenAiFunction, OpenAiStreamChoice, OpenAiStreamChunk, OpenAiStreamDelta, OpenAiToolCall,
    };
    use crate::types::StreamEvent;

    #[tokio::test]
    async fn tool_arguments_are_kept_when_arguments_delta_arrives_before_id() {
        let (tx, mut rx) = mpsc::channel(8);
        let mut state = OpenAiStreamState::new("gpt-test".to_string());

        let should_stop = handle_stream_chunk(
            &mut state,
            OpenAiStreamChunk {
                choices: vec![OpenAiStreamChoice {
                    delta: OpenAiStreamDelta {
                        _role: None,
                        content: None,
                        reasoning_content: None,
                        tool_calls: Some(vec![OpenAiToolCall {
                            id: None,
                            call_type: None,
                            function: OpenAiFunction {
                                name: None,
                                arguments: Some("{\"cmd\"".to_string()),
                            },
                            index: Some(0),
                        }]),
                    },
                    finish_reason: None,
                }],
                usage: None,
                model: None,
            },
            &tx,
        )
        .await;

        assert!(!should_stop);
        assert!(matches!(
            rx.recv().await,
            Some(StreamEvent::ToolCallDelta { arguments, .. }) if arguments == "{\"cmd\""
        ));

        let should_stop = handle_stream_chunk(
            &mut state,
            OpenAiStreamChunk {
                choices: vec![OpenAiStreamChoice {
                    delta: OpenAiStreamDelta {
                        _role: None,
                        content: None,
                        reasoning_content: None,
                        tool_calls: Some(vec![OpenAiToolCall {
                            id: Some("call-1".to_string()),
                            call_type: Some("function".to_string()),
                            function: OpenAiFunction {
                                name: Some("exec_command".to_string()),
                                arguments: Some(":\"git status\"}".to_string()),
                            },
                            index: Some(0),
                        }]),
                    },
                    finish_reason: Some("tool_calls".to_string()),
                }],
                usage: None,
                model: Some("gpt-test".to_string()),
            },
            &tx,
        )
        .await;

        assert!(should_stop);
        assert!(matches!(
            rx.recv().await,
            Some(StreamEvent::ToolCallStart { id, name }) if id == "call-1" && name == "exec_command"
        ));
        assert!(matches!(
            rx.recv().await,
            Some(StreamEvent::ToolCallDelta { id, arguments }) if id == "call-1" && arguments == ":\"git status\"}"
        ));
        assert!(matches!(
            rx.recv().await,
            Some(StreamEvent::ToolCallEnd { id }) if id == "call-1"
        ));

        finalize_stream(state, &tx).await;
        let done = rx.recv().await;
        match done {
            Some(StreamEvent::Done(response)) => {
                assert_eq!(response.message.tool_calls.len(), 1);
                assert_eq!(response.message.tool_calls[0].id, "call-1");
                assert_eq!(response.message.tool_calls[0].name, "exec_command");
                assert_eq!(
                    response.message.tool_calls[0].arguments,
                    "{\"cmd\":\"git status\"}"
                );
            }
            other => panic!("expected done event, got {other:?}"),
        }
    }
}
