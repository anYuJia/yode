mod request_conversion;
mod streaming;
mod streaming_support;
mod types;

use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use tokio::sync::mpsc;
use tracing::debug;

use crate::providers::error_shared::format_api_error;
use crate::providers::streaming_shared::map_stop_reason;
use self::request_conversion::anthropic_usage_to_usage;
use self::types::{
    AnthropicErrorResponse, AnthropicRequest, AnthropicResponse, AnthropicThinkingConfig,
    ContentBlock,
};

use crate::provider::LlmProvider;
use crate::types::{ChatRequest, ChatResponse, Message, ModelInfo, StreamEvent, ToolCall};

/// ── Provider implementation ─────────────────────────────────────────────────

pub struct AnthropicProvider {
    name: String,
    api_key: String,
    base_url: String,
    client: Client,
}

impl AnthropicProvider {
    pub fn new(
        name: impl Into<String>,
        api_key: impl Into<String>,
        base_url: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            api_key: api_key.into(),
            base_url: base_url.into(),
            client: Client::builder()
                .user_agent(format!("Yode/{}", env!("CARGO_PKG_VERSION")))
                .build()
                .expect("Failed to build HTTP client"),
        }
    }

    fn messages_url(&self) -> String {
        format!("{}/v1/messages", self.base_url.trim_end_matches('/'))
    }

    fn build_request(&self, request: &ChatRequest, stream: bool) -> AnthropicRequest {
        let (system, messages) = Self::convert_messages(&request.messages);
        let tools = Self::convert_tools(&request.tools);
        let thinking = Some(AnthropicThinkingConfig {
            thinking_type: "enabled".to_string(),
            budget_tokens: 1024,
        });

        AnthropicRequest {
            model: request.model.clone(),
            max_tokens: request.max_tokens.unwrap_or(4096),
            messages,
            system,
            tools,
            temperature: if thinking.is_some() {
                None
            } else {
                request.temperature
            },
            thinking,
            stream,
        }
    }
}

#[async_trait]
impl LlmProvider for AnthropicProvider {
    fn name(&self) -> &str {
        &self.name
    }

    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse> {
        let body = self.build_request(&request, false);

        debug!("Sending Anthropic chat request to {}", self.messages_url());

        let resp = self
            .client
            .post(self.messages_url())
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .context("Failed to send Anthropic chat request")?;

        let status = resp.status();
        if !status.is_success() {
            let error_text = resp.text().await.unwrap_or_default();
            let parsed = serde_json::from_str::<AnthropicErrorResponse>(&error_text)
                .ok()
                .map(|err_resp| err_resp.error.message);
            return Err(format_api_error("Anthropic", status, parsed, &error_text));
        }

        let api_resp: AnthropicResponse = resp
            .json()
            .await
            .context("Failed to parse Anthropic response")?;

        let mut text_content = String::new();
        let mut reasoning_content = String::new();
        let mut tool_calls = Vec::new();
        let mut content_blocks = Vec::new();

        for block in &api_resp.content {
            match block {
                ContentBlock::Text { text } => {
                    text_content.push_str(text);
                    content_blocks.push(crate::types::ContentBlock::Text { text: text.clone() });
                }
                ContentBlock::Thinking {
                    thinking,
                    signature,
                } => {
                    reasoning_content.push_str(thinking);
                    content_blocks.push(crate::types::ContentBlock::Thinking {
                        thinking: thinking.clone(),
                        signature: signature.clone(),
                    });
                }
                ContentBlock::ToolUse { id, name, input } => {
                    tool_calls.push(ToolCall {
                        id: id.clone(),
                        name: name.clone(),
                        arguments: serde_json::to_string(input).unwrap_or_default(),
                    });
                }
                _ => {}
            }
        }

        let usage = api_resp
            .usage
            .map(|usage| anthropic_usage_to_usage(&usage))
            .unwrap_or_default();

        let message = if content_blocks.is_empty() {
            Message::assistant_with_reasoning_and_tools(
                if text_content.is_empty() {
                    None
                } else {
                    Some(text_content)
                },
                if reasoning_content.is_empty() {
                    None
                } else {
                    Some(reasoning_content)
                },
                tool_calls,
            )
        } else {
            Message::assistant_from_blocks(content_blocks, tool_calls)
        };

        let stop_reason = api_resp.stop_reason.as_deref().map(map_stop_reason);

        Ok(ChatResponse {
            message,
            usage,
            model: api_resp.model,
            stop_reason,
        })
    }

    async fn chat_stream(&self, request: ChatRequest, tx: mpsc::Sender<StreamEvent>) -> Result<()> {
        self.send_chat_stream_request(request, tx).await
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        Ok(vec![
            ModelInfo {
                id: "claude-3-5-sonnet-20241022".to_string(),
                name: "Claude 3.5 Sonnet".to_string(),
                provider: self.name.clone(),
            },
            ModelInfo {
                id: "claude-3-5-haiku-20241022".to_string(),
                name: "Claude 3.5 Haiku".to_string(),
                provider: self.name.clone(),
            },
        ])
    }
}
