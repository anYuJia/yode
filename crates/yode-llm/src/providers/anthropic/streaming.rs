use anyhow::{anyhow, Context, Result};
use eventsource_stream::Eventsource;
use futures::StreamExt;
use tokio::sync::mpsc;
use tracing::{debug, error, warn};

use crate::types::{ChatRequest, StreamEvent};

use super::streaming_support::{finalize_stream, handle_stream_event, AnthropicStreamState};
use super::types::{
    AnthropicErrorResponse, AnthropicRequest, AnthropicStreamEvent, AnthropicThinkingConfig,
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
        let mut state = AnthropicStreamState::new(request.model.clone());

        while let Some(event_result) = event_stream.next().await {
            let event = match event_result {
                Ok(ev) => ev,
                Err(e) => {
                    let msg = format!("SSE stream error: {}", e);
                    error!("{}", msg);
                    let _ = tx.send(StreamEvent::Error(msg)).await;
                    state.finalize_reason = "sse_error";
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

            if handle_stream_event(&mut state, stream_event, &data, &tx).await? {
                break;
            }
        }

        finalize_stream(state, &tx).await
    }
}
