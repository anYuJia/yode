mod request_conversion;
mod streaming;
mod streaming_support;
mod types;

use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use tokio::sync::mpsc;
use tracing::debug;

use self::request_conversion::anthropic_usage_to_usage;
use self::types::{
    AnthropicErrorResponse, AnthropicRequest, AnthropicResponse, AnthropicThinkingConfig,
    ContentBlock,
};
use crate::providers::error_shared::format_api_error;
use crate::providers::http_client::provider_http_client;
use crate::providers::retry::send_with_retry;
use crate::providers::streaming_shared::map_stop_reason;
use crate::providers::write_debug_artifact;

use crate::provider::LlmProvider;
use crate::registry::KNOWN_PROVIDERS;
use crate::types::{ChatRequest, ChatResponse, Message, ModelInfo, StreamEvent, ToolCall};

// ── Provider implementation ─────────────────────────────────────────────────
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
            client: provider_http_client("anthropic"),
        }
    }

    fn messages_url(&self) -> String {
        format!("{}/v1/messages", self.base_url.trim_end_matches('/'))
    }

    fn build_request(&self, request: &ChatRequest, stream: bool) -> AnthropicRequest {
        let (system, messages) = Self::convert_messages(
            &request.messages,
            request.provider_hints.anthropic.as_ref(),
            &request.provider_hints.restore_system_blocks,
        );
        let tools = Self::convert_tools(&request.tools, request.provider_hints.anthropic.as_ref());
        let thinking = Some(anthropic_thinking_config());

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
        write_debug_artifact(
            &self.name,
            "anthropic-chat-request",
            serde_json::json!({
                "url": self.messages_url(),
                "body": &body,
            }),
        );

        let resp = send_with_retry(
            || {
                self.client
                    .post(self.messages_url())
                    .header("x-api-key", &self.api_key)
                    .header("anthropic-version", "2023-06-01")
                    .header("content-type", "application/json")
                    .json(&body)
            },
            "Failed to send Anthropic chat request",
        )
        .await?;

        let status = resp.status();
        if !status.is_success() {
            let error_text = resp.text().await.unwrap_or_default();
            let parsed = serde_json::from_str::<AnthropicErrorResponse>(&error_text)
                .ok()
                .map(|err_resp| err_resp.error.message);
            return Err(format_api_error("Anthropic", status, parsed, &error_text));
        }

        let response_text = resp
            .text()
            .await
            .context("Failed to read Anthropic response")?;
        write_debug_artifact(
            &self.name,
            "anthropic-chat-response",
            serde_json::json!({
                "status": status.as_u16(),
                "body": &response_text,
            }),
        );
        let api_resp: AnthropicResponse = serde_json::from_str(&response_text)
            .context("Failed to parse Anthropic response")?;

        let mut text_content = String::new();
        let mut reasoning_content = String::new();
        let mut tool_calls = Vec::new();
        let mut content_blocks = Vec::new();

        for block in &api_resp.content {
            match block {
                ContentBlock::Text { text, .. } => {
                    text_content.push_str(text);
                    content_blocks.push(crate::types::ContentBlock::Text { text: text.clone() });
                }
                ContentBlock::Thinking {
                    thinking,
                    signature,
                    ..
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
        let models = KNOWN_PROVIDERS
            .iter()
            .find(|provider| provider.name == "anthropic")
            .map(|provider| provider.default_models)
            .unwrap_or(&[]);

        Ok(models
            .iter()
            .map(|model| ModelInfo {
                id: (*model).to_string(),
                name: anthropic_model_display_name(model),
                provider: self.name.clone(),
            })
            .collect())
    }
}

fn anthropic_model_display_name(model: &str) -> String {
    if model.contains("opus") {
        "Claude Opus 4".to_string()
    } else if model.contains("sonnet") {
        "Claude Sonnet 4".to_string()
    } else if model.contains("haiku") {
        "Claude Haiku 4".to_string()
    } else {
        model.to_string()
    }
}

fn anthropic_thinking_config() -> AnthropicThinkingConfig {
    AnthropicThinkingConfig {
        thinking_type: "enabled".to_string(),
        budget_tokens: anthropic_thinking_budget_tokens(),
    }
}

fn anthropic_thinking_budget_tokens() -> u32 {
    parse_anthropic_thinking_budget_tokens(
        std::env::var("YODE_ANTHROPIC_THINKING_BUDGET_TOKENS")
            .ok()
            .as_deref(),
    )
}

fn parse_anthropic_thinking_budget_tokens(raw: Option<&str>) -> u32 {
    raw.and_then(|value| value.parse::<u32>().ok())
        .filter(|value| *value >= 1024)
        .unwrap_or(1024)
}

#[cfg(test)]
mod tests {
    use super::AnthropicProvider;
    use crate::provider::LlmProvider;
    use crate::registry::KNOWN_PROVIDERS;

    #[tokio::test]
    async fn anthropic_static_models_include_claude_4() {
        let provider = AnthropicProvider::new("anthropic", "key", "https://example.test");
        let models = provider.list_models().await.unwrap();
        assert!(models
            .iter()
            .any(|model| model.id == "claude-sonnet-4-20250514"));
        assert!(models
            .iter()
            .any(|model| model.id == "claude-opus-4-20250514"));
    }

    #[tokio::test]
    async fn anthropic_static_models_match_provider_catalog() {
        let provider = AnthropicProvider::new("anthropic", "key", "https://example.test");
        let models = provider.list_models().await.unwrap();
        let listed_ids = models
            .iter()
            .map(|model| model.id.as_str())
            .collect::<Vec<_>>();
        let catalog_ids = KNOWN_PROVIDERS
            .iter()
            .find(|provider| provider.name == "anthropic")
            .unwrap()
            .default_models;

        assert_eq!(listed_ids, catalog_ids);
    }

    #[test]
    fn thinking_budget_parser_uses_safe_minimum_default() {
        assert_eq!(super::parse_anthropic_thinking_budget_tokens(None), 1024);
        assert_eq!(
            super::parse_anthropic_thinking_budget_tokens(Some("not-a-number")),
            1024
        );
        assert_eq!(
            super::parse_anthropic_thinking_budget_tokens(Some("512")),
            1024
        );
        assert_eq!(
            super::parse_anthropic_thinking_budget_tokens(Some("4096")),
            4096
        );
    }
}
