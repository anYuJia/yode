mod conversion;
mod streaming;
mod streaming_support;
mod types;

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use tokio::sync::mpsc;
use tracing::{debug, trace};

use self::conversion::{
    message_to_openai, openai_content_text, openai_message_to_internal, openai_usage_to_usage,
    tool_to_openai,
};
use self::types::{
    OpenAiErrorResponse, OpenAiMessage, OpenAiModelsResponse, OpenAiRequest, OpenAiResponse,
    OpenAiTool, StreamOptions,
};
use crate::providers::error_shared::format_api_error;
use crate::providers::http_client::provider_http_client;
use crate::providers::retry::send_with_retry;
use crate::providers::streaming_shared::map_stop_reason;

use crate::provider::LlmProvider;
use crate::types::{ChatRequest, ChatResponse, ModelInfo, StreamEvent};

// ── Provider implementation ─────────────────────────────────────────────────

pub struct OpenAiProvider {
    name: String,
    api_key: String,
    base_url: String,
    client: Client,
}

impl OpenAiProvider {
    pub fn new(
        name: impl Into<String>,
        api_key: impl Into<String>,
        base_url: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            api_key: api_key.into(),
            base_url: base_url.into(),
            client: provider_http_client("openai"),
        }
    }

    fn chat_url(&self) -> String {
        format!("{}/chat/completions", self.endpoint_base_url())
    }

    fn models_url(&self) -> String {
        format!("{}/models", self.endpoint_base_url())
    }

    fn endpoint_base_url(&self) -> String {
        let trimmed = self.base_url.trim_end_matches('/');
        match reqwest::Url::parse(trimmed) {
            Ok(url) if url.path() == "/" => format!("{}/v1", trimmed),
            _ => trimmed.to_string(),
        }
    }

    fn build_request(&self, request: &ChatRequest, stream: bool) -> OpenAiRequest {
        let tools: Vec<OpenAiTool> = request.tools.iter().map(tool_to_openai).collect();
        let mut messages: Vec<OpenAiMessage> =
            request.messages.iter().map(message_to_openai).collect();
        if !request.provider_hints.restore_system_blocks.is_empty() {
            let insert_at = messages
                .iter()
                .position(|message| {
                    message.role == "system"
                        && message
                            .content
                            .as_ref()
                            .and_then(|content| openai_content_text(&Some(content.clone())))
                            .unwrap_or_default()
                            .starts_with("[Context summary]")
                })
                .map(|index| index + 1)
                .or_else(|| messages.iter().position(|message| message.role != "system"))
                .unwrap_or(messages.len());
            for (offset, block) in request
                .provider_hints
                .restore_system_blocks
                .iter()
                .cloned()
                .enumerate()
            {
                messages.insert(
                    insert_at + offset,
                    OpenAiMessage {
                        role: "system".to_string(),
                        content: Some(serde_json::json!(format!(
                            "[Post-compact restore: {}]\n{}",
                            block.kind, block.content
                        ))),
                        reasoning_content: None,
                        tool_calls: None,
                        tool_call_id: None,
                    },
                );
            }
        }

        OpenAiRequest {
            model: request.model.clone(),
            messages,
            tools,
            temperature: request.temperature,
            max_tokens: request.max_tokens,
            stream,
            stream_options: stream.then_some(StreamOptions {
                include_usage: true,
            }),
        }
    }
}

#[async_trait]
impl LlmProvider for OpenAiProvider {
    fn name(&self) -> &str {
        &self.name
    }

    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse> {
        let body = self.build_request(&request, false);

        debug!("Sending chat request to {}", self.chat_url());
        trace!("Request body: {:?}", body);

        let resp = send_with_retry(
            || {
                self.client
                    .post(self.chat_url())
                    .header("Authorization", format!("Bearer {}", self.api_key))
                    .header("Content-Type", "application/json")
                    .json(&body)
            },
            "Failed to send chat request",
        )
        .await?;

        let status = resp.status();
        if !status.is_success() {
            let error_text = resp.text().await.unwrap_or_default();
            let parsed = serde_json::from_str::<OpenAiErrorResponse>(&error_text)
                .ok()
                .map(|err_resp| {
                    format!(
                        "{} (code: {})",
                        err_resp.error.message,
                        err_resp.error.code.unwrap_or_else(|| "none".to_string())
                    )
                });
            return Err(format_api_error("OpenAI", status, parsed, &error_text));
        }

        let content_type = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_string();
        let response_text = resp.text().await.context("Failed to read chat response")?;
        if content_type.contains("text/html") || response_text.trim_start().starts_with("<!") {
            return Err(anyhow!(
                "模型接口返回了网页内容，不是 OpenAI 兼容 JSON。请检查 base_url 是否指向 API 地址，通常需要以 /v1 结尾。"
            ));
        }

        let api_resp: OpenAiResponse = serde_json::from_str(&response_text).context(
            "模型接口返回内容无法解析为 OpenAI 兼容 JSON，请检查 base_url 和 provider format",
        )?;

        let choice = api_resp
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| anyhow!("OpenAI returned no choices"))?;

        let message = openai_message_to_internal(&choice.message);

        let stop_reason = choice.finish_reason.as_deref().map(map_stop_reason);

        let usage = api_resp
            .usage
            .map(|usage| openai_usage_to_usage(&usage))
            .unwrap_or_default();

        debug!(
            "Chat response received: {} prompt tokens, {} completion tokens",
            usage.prompt_tokens, usage.completion_tokens
        );

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
        debug!("Fetching models from {}", self.models_url());

        let resp = send_with_retry(
            || {
                self.client
                    .get(self.models_url())
                    .header("Authorization", format!("Bearer {}", self.api_key))
            },
            "Failed to fetch models",
        )
        .await?;

        let status = resp.status();
        if !status.is_success() {
            let error_text = resp.text().await.unwrap_or_default();
            if let Ok(err_resp) = serde_json::from_str::<OpenAiErrorResponse>(&error_text) {
                return Err(anyhow!(
                    "OpenAI API error ({}): {} (code: {})",
                    status,
                    err_resp.error.message,
                    err_resp.error.code.unwrap_or_else(|| "none".to_string())
                ));
            }
            return Err(anyhow!("OpenAI API error ({}): {}", status, error_text));
        }

        let models_resp: OpenAiModelsResponse = resp
            .json()
            .await
            .context("Failed to parse models response")?;

        let models = models_resp
            .data
            .into_iter()
            .map(|model| ModelInfo {
                id: model.id.clone(),
                name: model.id,
                provider: self.name.clone(),
            })
            .collect();

        Ok(models)
    }
}

#[cfg(test)]
mod tests {
    use super::{openai_content_text, OpenAiProvider};
    use crate::types::{ChatRequest, Message, ProviderRequestHints};

    #[test]
    fn openai_build_request_injects_restore_blocks_from_provider_hints() {
        let provider = OpenAiProvider::new("openai", "test-key", "https://example.com");
        let request = ChatRequest {
            model: "gpt-4o".to_string(),
            messages: vec![
                Message::system("base system"),
                Message::system("[Context summary] compacted"),
                Message::user("resume"),
            ],
            tools: vec![],
            temperature: Some(0.2),
            max_tokens: Some(512),
            provider_hints: ProviderRequestHints {
                anthropic: None,
                restore_system_blocks: vec![
                    crate::types::RestoreSystemBlockHint {
                        kind: "runtime".to_string(),
                        content: "- Runtime cwd: /tmp".to_string(),
                    },
                    crate::types::RestoreSystemBlockHint {
                        kind: "files".to_string(),
                        content: "- Recent files read: src/main.rs".to_string(),
                    },
                ],
            },
        };

        let built = provider.build_request(&request, false);
        let system_messages = built
            .messages
            .iter()
            .filter(|message| message.role == "system")
            .map(|message| {
                message
                    .content
                    .as_ref()
                    .and_then(|content| openai_content_text(&Some(content.clone())))
                    .unwrap_or_default()
            })
            .collect::<Vec<_>>();

        assert_eq!(system_messages.len(), 4);
        assert_eq!(system_messages[0], "base system");
        assert_eq!(system_messages[1], "[Context summary] compacted");
        assert_eq!(
            system_messages[2],
            "[Post-compact restore: runtime]\n- Runtime cwd: /tmp"
        );
        assert_eq!(
            system_messages[3],
            "[Post-compact restore: files]\n- Recent files read: src/main.rs"
        );
    }
}
