mod conversion;
mod streaming;
mod streaming_support;
mod types;

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use tokio::sync::mpsc;
use tracing::{debug, trace};

use crate::providers::streaming_shared::map_stop_reason;
use self::conversion::{
    message_to_openai, openai_message_to_internal, openai_usage_to_usage, tool_to_openai,
};
use self::types::{
    OpenAiErrorResponse, OpenAiMessage, OpenAiModelsResponse, OpenAiRequest, OpenAiResponse,
    OpenAiTool, StreamOptions,
};

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
            client: Client::builder()
                .user_agent(format!("Yode/{}", env!("CARGO_PKG_VERSION")))
                .build()
                .expect("Failed to build HTTP client"),
        }
    }

    fn chat_url(&self) -> String {
        format!("{}/chat/completions", self.base_url.trim_end_matches('/'))
    }

    fn models_url(&self) -> String {
        format!("{}/models", self.base_url.trim_end_matches('/'))
    }

    fn build_request(&self, request: &ChatRequest, stream: bool) -> OpenAiRequest {
        let tools: Vec<OpenAiTool> = request.tools.iter().map(tool_to_openai).collect();
        let messages: Vec<OpenAiMessage> = request.messages.iter().map(message_to_openai).collect();

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

        let resp = self
            .client
            .post(self.chat_url())
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .context("Failed to send chat request")?;

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

        let api_resp: OpenAiResponse =
            resp.json().await.context("Failed to parse chat response")?;

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

        let resp = self
            .client
            .get(self.models_url())
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .await
            .context("Failed to fetch models")?;

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
