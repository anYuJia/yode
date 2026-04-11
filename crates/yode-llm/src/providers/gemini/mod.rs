mod conversion;
mod streaming;
mod types;

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use tokio::sync::mpsc;
use tracing::debug;

use crate::providers::error_shared::format_api_error;
use self::conversion::{convert_messages, convert_tools, parse_response};
use self::streaming::stream_response;
use self::types::{GeminiError, GeminiRequest, GeminiResponse, GenerationConfig};

use crate::provider::LlmProvider;
use crate::types::{ChatRequest, ChatResponse, ModelInfo, StreamEvent};

pub struct GeminiProvider {
    name: String,
    api_key: String,
    base_url: String,
    client: Client,
}

impl GeminiProvider {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            name: "google".to_string(),
            api_key: api_key.into(),
            base_url: "https://generativelanguage.googleapis.com/v1beta".to_string(),
            client: Client::builder()
                .user_agent(format!("Yode/{}", env!("CARGO_PKG_VERSION")))
                .build()
                .expect("Failed to build HTTP client"),
        }
    }

    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into();
        self
    }

    fn generate_url(&self, model: &str) -> String {
        format!(
            "{}/models/{}:generateContent?key={}",
            self.base_url.trim_end_matches('/'),
            model,
            self.api_key
        )
    }

    fn stream_url(&self, model: &str) -> String {
        format!(
            "{}/models/{}:streamGenerateContent?alt=sse&key={}",
            self.base_url.trim_end_matches('/'),
            model,
            self.api_key
        )
    }

    fn models_url(&self) -> String {
        format!(
            "{}/models?key={}",
            self.base_url.trim_end_matches('/'),
            self.api_key
        )
    }

    fn build_request(&self, request: &ChatRequest) -> GeminiRequest {
        let (system, contents) = convert_messages(&request.messages);
        let tools = convert_tools(&request.tools);

        GeminiRequest {
            contents,
            system_instruction: system,
            tools,
            generation_config: Some(GenerationConfig {
                temperature: request.temperature,
                max_output_tokens: request.max_tokens,
            }),
        }
    }
}

#[async_trait]
impl LlmProvider for GeminiProvider {
    fn name(&self) -> &str {
        &self.name
    }

    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse> {
        let body = self.build_request(&request);
        let url = self.generate_url(&request.model);
        debug!("Sending Gemini request to {}", url);

        let resp = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .context("Failed to send Gemini request")?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            let parsed = serde_json::from_str::<GeminiError>(&text)
                .ok()
                .map(|err| err.error.message);
            return Err(format_api_error("Gemini", status, parsed, &text));
        }

        let api_resp: GeminiResponse = resp
            .json()
            .await
            .context("Failed to parse Gemini response")?;
        let (message, usage) = parse_response(&api_resp);

        Ok(ChatResponse {
            message,
            usage,
            model: request.model,
            stop_reason: None,
        })
    }

    async fn chat_stream(&self, request: ChatRequest, tx: mpsc::Sender<StreamEvent>) -> Result<()> {
        let body = self.build_request(&request);
        let url = self.stream_url(&request.model);
        debug!("Sending Gemini stream request to {}", url);

        let resp = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .context("Failed to send Gemini stream request")?;

        stream_response(resp, request.model, tx).await
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        #[derive(Deserialize)]
        struct ModelsResp {
            models: Vec<ModelEntry>,
        }

        #[derive(Deserialize)]
        struct ModelEntry {
            name: String,
            #[serde(default, rename = "displayName")]
            display_name: Option<String>,
        }

        let resp = self
            .client
            .get(self.models_url())
            .send()
            .await
            .context("Failed to fetch Gemini models")?;

        if !resp.status().is_success() {
            return Err(anyhow!("Gemini models API error: {}", resp.status()));
        }

        let data: ModelsResp = resp.json().await?;
        Ok(data
            .models
            .into_iter()
            .filter(|model| model.name.contains("gemini"))
            .map(|model| {
                let id = model
                    .name
                    .strip_prefix("models/")
                    .unwrap_or(&model.name)
                    .to_string();
                ModelInfo {
                    name: model.display_name.unwrap_or_else(|| id.clone()),
                    id,
                    provider: "google".into(),
                }
            })
            .collect())
    }
}
