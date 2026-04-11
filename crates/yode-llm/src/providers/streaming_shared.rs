use tokio::sync::mpsc;

use crate::types::{stream_done, Message, StopReason, StreamEvent, ToolCall, Usage};

pub(crate) fn map_stop_reason(reason: &str) -> StopReason {
    match reason {
        "stop" | "end_turn" => StopReason::EndTurn,
        "tool_calls" | "tool_use" => StopReason::ToolUse,
        "length" | "max_tokens" => StopReason::MaxTokens,
        "stop_sequence" => StopReason::StopSequence,
        "content_filter" => StopReason::ContentFilter,
        other => StopReason::Other(other.to_string()),
    }
}

pub(crate) async fn emit_stream_error(tx: &mpsc::Sender<StreamEvent>, message: impl Into<String>) {
    let _ = tx.send(StreamEvent::Error(message.into())).await;
}

pub(crate) async fn emit_usage_update(tx: &mpsc::Sender<StreamEvent>, usage: &Usage) {
    let _ = tx.send(StreamEvent::UsageUpdate(usage.clone())).await;
}

pub(crate) async fn emit_tool_call_start(
    tx: &mpsc::Sender<StreamEvent>,
    id: String,
    name: String,
) {
    let _ = tx.send(StreamEvent::ToolCallStart { id, name }).await;
}

pub(crate) async fn emit_tool_call_end(tx: &mpsc::Sender<StreamEvent>, id: String) {
    let _ = tx.send(StreamEvent::ToolCallEnd { id }).await;
}

pub(crate) async fn append_tool_call_delta(
    tx: &mpsc::Sender<StreamEvent>,
    tool_call: &mut ToolCall,
    partial_arguments: &str,
) -> bool {
    tool_call.arguments.push_str(partial_arguments);
    tx.send(StreamEvent::ToolCallDelta {
        id: tool_call.id.clone(),
        arguments: partial_arguments.to_string(),
    })
    .await
    .is_ok()
}

pub(crate) async fn emit_done_event(
    tx: &mpsc::Sender<StreamEvent>,
    message: Message,
    usage: Usage,
    model: String,
    stop_reason: Option<StopReason>,
) {
    let _ = tx
        .send(stream_done(message, usage, model, stop_reason))
        .await;
}
