use anyhow::{anyhow, Result};
use eventsource_stream::Eventsource;
use futures::StreamExt;
use tokio::sync::mpsc;
use tracing::warn;

use crate::providers::error_shared::{format_api_error, read_error_body};
use crate::providers::streaming_shared::emit_done_event;

use super::conversion::{
    assistant_message, gemini_usage_to_usage, map_gemini_finish_reason, send_tool_call_events,
};
use super::types::{GeminiError, GeminiPart, GeminiResponse};

use crate::types::{StreamEvent, ToolCall, Usage};

pub(super) async fn stream_response(
    resp: reqwest::Response,
    model: String,
    provider_name: String,
    tx: mpsc::Sender<StreamEvent>,
) -> Result<()> {
    let status = resp.status();
    if !status.is_success() {
        let text = read_error_body("Gemini", status, resp).await;
        let message = match serde_json::from_str::<GeminiError>(&text) {
            Ok(err) => {
                format_api_error("Gemini", status, Some(err.error.message), &text).to_string()
            }
            Err(_) => format_api_error("Gemini", status, None, &text).to_string(),
        };
        return Err(anyhow!(message));
    }

    let mut event_stream = resp.bytes_stream().eventsource();
    let mut full_text = String::new();
    let mut all_tool_calls = Vec::new();
    let mut final_usage = Usage::default();
    let mut tool_call_counter = 0u32;
    let mut stop_reason = None;
    let mut chunk_count = 0u32;
    let mut debug_events = Vec::new();

    while let Some(event_result) = event_stream.next().await {
        let event = match event_result {
            Ok(event) => event,
            Err(err) => {
                // SSE 错误不能吞掉：截断流必须向上失败，不能当作成功。
                return Err(anyhow::anyhow!("Gemini SSE stream error: {}", err));
            }
        };

        if crate::providers::debug_requests_enabled() {
            debug_events.push(serde_json::json!({
                "event": event.event,
                "data": &event.data,
            }));
        }
        let chunk: GeminiResponse = match serde_json::from_str(&event.data) {
            Ok(chunk) => chunk,
            Err(err) => {
                warn!("Failed to parse Gemini chunk: {}", err);
                continue;
            }
        };
        chunk_count = chunk_count.saturating_add(1);

        if let Some(usage) = &chunk.usage_metadata {
            final_usage = gemini_usage_to_usage(usage);
        }

        if let Some(candidates) = &chunk.candidates {
            if let Some(candidate) = candidates.first() {
                if let Some(content) = &candidate.content {
                    for part in &content.parts {
                        match part {
                            GeminiPart::Text { text } => {
                                full_text.push_str(text);
                                let _ = tx.send(StreamEvent::TextDelta(text.clone())).await;
                            }
                            GeminiPart::FunctionCall { function_call } => {
                                tool_call_counter += 1;
                                let tool_call = ToolCall {
                                    id: format!("gemini_tc_{}", tool_call_counter),
                                    name: function_call.name.clone(),
                                    arguments: function_call.args.to_string(),
                                };
                                send_tool_call_events(&tx, &tool_call).await;
                                all_tool_calls.push(tool_call);
                            }
                            GeminiPart::FunctionResponse { .. } => {}
                            GeminiPart::InlineData { .. } => {}
                        }
                    }
                }
                if let Some(reason) = candidate.finish_reason.as_deref() {
                    stop_reason = Some(map_gemini_finish_reason(reason));
                }
            }
        }
    }

    // 没有任何 finish_reason 说明流被截断（连接中断、服务端异常退出等）：
    // 绝不能把它当作成功，更不能再执行部分工具调用。
    if stop_reason.is_none() {
        return Err(anyhow::anyhow!(
            "Gemini 流在收到 finish_reason 之前中断（已接收 {} 个 chunk）；丢弃部分输出。",
            chunk_count
        ));
    }
    crate::providers::write_debug_artifact(
        &provider_name,
        "gemini-stream-events",
        serde_json::json!({
            "events": debug_events,
        }),
    )
    .await;
    let message = assistant_message(full_text, all_tool_calls);
    emit_done_event(&tx, message, final_usage, model, stop_reason).await;
    Ok(())
}
