use anyhow::{anyhow, Context, Result};
use eventsource_stream::Eventsource;
use futures::StreamExt;
use tokio::sync::mpsc;
use tracing::{debug, trace, warn};

use crate::providers::streaming_shared::{emit_stream_error, emit_usage_update};
use crate::types::{ChatRequest, StreamEvent, Usage};

use super::conversion::{message_to_openai, tool_to_openai};
use super::streaming_support::{finalize_stream, handle_stream_chunk, OpenAiStreamState};
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
                emit_stream_error(&tx, msg.clone()).await;
                return Err(anyhow!(msg));
            }
            let msg = format!("OpenAI API error ({}): {}", status, error_text);
            emit_stream_error(&tx, msg.clone()).await;
            return Err(anyhow!(msg));
        }

        let mut event_stream = resp.bytes_stream().eventsource();
        let mut state = OpenAiStreamState::new(request.model.clone());

        while let Some(event_result) = event_stream.next().await {
            let event = match event_result {
                Ok(ev) => ev,
                Err(err) => {
                    let msg = format!("SSE stream error: {}", err);
                    emit_stream_error(&tx, msg).await;
                    state.finalize_reason = "sse_error";
                    break;
                }
            };

            let data = event.data;
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

        finalize_stream(state, &tx).await;
        Ok(())
    }
}

fn stream_usage(usage: &super::types::OpenAiUsage) -> Usage {
    let prompt_tokens = if usage.prompt_tokens == 0 && usage.total_tokens > usage.completion_tokens {
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
    }
}
