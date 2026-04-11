use anyhow::{anyhow, Result};
use eventsource_stream::Eventsource;
use futures::StreamExt;
use tokio::sync::mpsc;
use tracing::warn;

use super::conversion::{
    assistant_message, done_event, gemini_usage_to_usage, send_tool_call_events,
};
use super::types::{GeminiError, GeminiPart, GeminiResponse};

use crate::types::{StreamEvent, ToolCall, Usage};

pub(super) async fn stream_response(
    resp: reqwest::Response,
    model: String,
    tx: mpsc::Sender<StreamEvent>,
) -> Result<()> {
    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        let message = match serde_json::from_str::<GeminiError>(&text) {
            Ok(err) => format!("Gemini API error ({}): {}", status, err.error.message),
            Err(_) => format!("Gemini API error ({}): {}", status, text),
        };
        let _ = tx.send(StreamEvent::Error(message.clone())).await;
        return Err(anyhow!(message));
    }

    let mut event_stream = resp.bytes_stream().eventsource();
    let mut full_text = String::new();
    let mut all_tool_calls = Vec::new();
    let mut final_usage = Usage::default();
    let mut tool_call_counter = 0u32;

    while let Some(event_result) = event_stream.next().await {
        let event = match event_result {
            Ok(event) => event,
            Err(err) => {
                warn!("Gemini SSE error: {}", err);
                continue;
            }
        };

        let chunk: GeminiResponse = match serde_json::from_str(&event.data) {
            Ok(chunk) => chunk,
            Err(err) => {
                warn!("Failed to parse Gemini chunk: {}", err);
                continue;
            }
        };

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
                                    arguments: serde_json::to_string(&function_call.args)
                                        .unwrap_or_default(),
                                };
                                send_tool_call_events(&tx, &tool_call).await;
                                all_tool_calls.push(tool_call);
                            }
                            GeminiPart::FunctionResponse { .. } => {}
                        }
                    }
                }
            }
        }
    }

    let message = assistant_message(full_text, all_tool_calls);
    let _ = tx.send(done_event(message, final_usage, model)).await;
    Ok(())
}
