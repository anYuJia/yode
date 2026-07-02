use anyhow::{anyhow, Result};
use eventsource_stream::Eventsource;
use futures::StreamExt;
use tokio::sync::mpsc;
use tracing::{debug, trace, warn};

use crate::providers::error_shared::format_api_error;
use crate::providers::retry::send_with_retry;
use crate::providers::streaming_shared::emit_usage_update;
use crate::providers::write_debug_artifact;
use crate::types::{ChatRequest, StreamEvent, Usage};

use super::conversion::{message_to_openai, tool_to_openai};
use super::streaming_support::{finalize_stream, handle_stream_chunk, OpenAiStreamState};
use super::types::{
    OpenAiErrorResponse, OpenAiMessage, OpenAiRequest, OpenAiStreamChunk, OpenAiTool, StreamOptions,
};
use super::OpenAiProvider;

impl OpenAiProvider {
    pub(super) async fn send_chat_stream_request(
        &self,
        request: ChatRequest,
        tx: mpsc::Sender<StreamEvent>,
    ) -> Result<()> {
        let tools: Vec<OpenAiTool> = request.tools.iter().map(tool_to_openai).collect();
        let messages: Vec<OpenAiMessage> = request
            .messages
            .iter()
            .map(message_to_openai)
            .map(|message| {
                if self.compatibility.include_reasoning_content {
                    message
                } else {
                    message.without_reasoning()
                }
            })
            .collect();

        let body = OpenAiRequest {
            model: request.model.clone(),
            messages,
            tools,
            temperature: request.temperature,
            max_tokens: request.max_tokens,
            stream: true,
            stream_options: self
                .compatibility
                .include_stream_options
                .then_some(StreamOptions {
                    include_usage: true,
                }),
        };

        debug!("Sending streaming chat request to {}", self.chat_url());
        write_debug_artifact(
            &self.name,
            "openai-stream-request",
            serde_json::json!({
                "url": self.chat_url(),
                "body": &body,
            }),
        )
        .await;

        let resp = send_with_retry(
            || {
                self.client
                    .post(self.chat_url())
                    .header("Authorization", format!("Bearer {}", self.api_key))
                    .header("Content-Type", "application/json")
                    .json(&body)
            },
            "Failed to send streaming chat request",
        )
        .await?;

        let status = resp.status();
        if !status.is_success() {
            let error_text = resp.text().await.unwrap_or_default();
            let err = format_api_error(
                "OpenAI",
                status,
                serde_json::from_str::<OpenAiErrorResponse>(&error_text)
                    .ok()
                    .map(|err_resp| {
                        format!(
                            "{} (code: {})",
                            err_resp.error.message,
                            err_resp.error.code.unwrap_or_else(|| "none".to_string())
                        )
                    }),
                &error_text,
            );
            return Err(err);
        }

        let content_type = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_string();
        if content_type.contains("text/html") {
            return Err(anyhow!(
                "模型接口返回了网页内容，不是 OpenAI 兼容流式响应。请检查 base_url 是否指向 API 地址，通常需要以 /v1 结尾。"
            ));
        }

        let mut event_stream = resp.bytes_stream().eventsource();
        let mut state = OpenAiStreamState::new(request.model.clone());
        let mut debug_events = Vec::new();

        while let Some(event_result) = event_stream.next().await {
            let event = match event_result {
                Ok(ev) => ev,
                Err(err) => {
                    let msg = format!("SSE stream error: {}", err);
                    return Err(anyhow!(msg));
                }
            };

            let data = event.data;
            if crate::providers::debug_requests_enabled() {
                debug_events.push(serde_json::json!({
                    "event": event.event,
                    "data": &data,
                }));
            }
            if data.trim() == "[DONE]" {
                debug!("Stream completed with [DONE]");
                state.saw_done_sentinel = true;
                state.finalize_reason = "done_sentinel";
                break;
            }

            let chunk: OpenAiStreamChunk = match serde_json::from_str(&data) {
                Ok(chunk) => chunk,
                Err(err) => {
                    warn!("Failed to parse stream chunk: {} (data: {})", err, data);
                    continue;
                }
            };

            trace!(
                "Received chunk: choices={}, has_usage={}",
                chunk.choices.len(),
                chunk.usage.is_some()
            );

            if let Some(usage) = &chunk.usage {
                emit_usage_update(&tx, &stream_usage(usage)).await;
            }

            if handle_stream_chunk(&mut state, chunk, &tx).await {
                debug!("Stream receiver dropped or finish_reason reached, stopping");
                break;
            }
        }

        write_debug_artifact(
            &self.name,
            "openai-stream-events",
            serde_json::json!({
                "events": debug_events,
            }),
        )
        .await;
        finalize_stream(state, &tx).await;
        Ok(())
    }
}

fn stream_usage(usage: &super::types::OpenAiUsage) -> Usage {
    let prompt_tokens = if usage.prompt_tokens == 0 && usage.total_tokens > usage.completion_tokens
    {
        usage.total_tokens - usage.completion_tokens
    } else {
        usage.prompt_tokens
    };

    Usage {
        prompt_tokens,
        completion_tokens: usage.completion_tokens,
        total_tokens: usage.total_tokens,
        cache_write_tokens: 0,
        cache_read_tokens: usage
            .prompt_tokens_details
            .as_ref()
            .map(|details| details.cached_tokens)
            .unwrap_or(0),
        cache_deleted_tokens: 0,
    }
}
