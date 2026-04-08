use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tracing::{debug, warn};

use crate::provider::LlmProvider;
use crate::types::{
    ChatRequest, ChatResponse, Message, ModelInfo, Role, StreamEvent, ToolCall, ToolDefinition,
    Usage,
};

// ── Gemini API types ────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GeminiRequest {
    contents: Vec<GeminiContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system_instruction: Option<GeminiContent>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<GeminiToolDeclaration>,
    #[serde(skip_serializing_if = "Option::is_none")]
    generation_config: Option<GenerationConfig>,
}

#[derive(Debug, Serialize, Deserialize)]
struct GeminiContent {
    #[serde(skip_serializing_if = "Option::is_none")]
    role: Option<String>,
    parts: Vec<GeminiPart>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum GeminiPart {
    Text {
        text: String,
    },
    FunctionCall {
        #[serde(rename = "functionCall")]
        function_call: GeminiFunctionCall,
    },
    FunctionResponse {
        #[serde(rename = "functionResponse")]
        function_response: GeminiFunctionResponse,
    },
}

#[derive(Debug, Serialize, Deserialize)]
struct GeminiFunctionCall {
    name: String,
    args: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
struct GeminiFunctionResponse {
    name: String,
    response: serde_json::Value,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GenerationConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_output_tokens: Option<u32>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GeminiToolDeclaration {
    function_declarations: Vec<GeminiFunctionDecl>,
}

#[derive(Debug, Serialize)]
struct GeminiFunctionDecl {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

// ── Response types ──────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GeminiResponse {
    candidates: Option<Vec<GeminiCandidate>>,
    usage_metadata: Option<GeminiUsage>,
    #[serde(default)]
    #[allow(dead_code)]
    model_version: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GeminiCandidate {
    content: Option<GeminiContent>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GeminiUsage {
    #[serde(default)]
    prompt_token_count: u32,
    #[serde(default)]
    candidates_token_count: u32,
    #[serde(default)]
    total_token_count: u32,
}

#[derive(Debug, Deserialize)]
struct GeminiError {
    error: GeminiErrorDetail,
}

#[derive(Debug, Deserialize)]
struct GeminiErrorDetail {
    message: String,
    #[serde(default)]
    #[allow(dead_code)]
    code: Option<i32>,
}

// ── Stream types ────────────────────────────────────────────────────────────

// Gemini streaming returns line-delimited JSON objects

// ── Provider ────────────────────────────────────────────────────────────────

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
}

fn convert_messages(messages: &[Message]) -> (Option<GeminiContent>, Vec<GeminiContent>) {
    let mut system = None;
    let mut contents = Vec::new();

    for msg in messages {
        match msg.role {
            Role::System => {
                if let Some(ref text) = msg.content {
                    system = Some(GeminiContent {
                        role: None,
                        parts: vec![GeminiPart::Text { text: text.clone() }],
                    });
                }
            }
            Role::User => {
                if let Some(ref text) = msg.content {
                    contents.push(GeminiContent {
                        role: Some("user".into()),
                        parts: vec![GeminiPart::Text { text: text.clone() }],
                    });
                }
            }
            Role::Assistant => {
                let mut parts = Vec::new();
                if let Some(ref text) = msg.content {
                    if !text.is_empty() {
                        parts.push(GeminiPart::Text { text: text.clone() });
                    }
                }
                for tc in &msg.tool_calls {
                    let args: serde_json::Value = serde_json::from_str(&tc.arguments)
                        .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
                    parts.push(GeminiPart::FunctionCall {
                        function_call: GeminiFunctionCall {
                            name: tc.name.clone(),
                            args,
                        },
                    });
                }
                if !parts.is_empty() {
                    contents.push(GeminiContent {
                        role: Some("model".into()),
                        parts,
                    });
                }
            }
            Role::Tool => {
                if let Some(ref text) = msg.content {
                    // Need the tool name — extract from tool_call_id pattern or use "function"
                    let name = msg
                        .tool_call_id
                        .as_deref()
                        .unwrap_or("function")
                        .to_string();
                    contents.push(GeminiContent {
                        role: Some("user".into()),
                        parts: vec![GeminiPart::FunctionResponse {
                            function_response: GeminiFunctionResponse {
                                name,
                                response: serde_json::json!({ "result": text }),
                            },
                        }],
                    });
                }
            }
        }
    }

    (system, contents)
}

fn convert_tools(tools: &[ToolDefinition]) -> Vec<GeminiToolDeclaration> {
    if tools.is_empty() {
        return vec![];
    }
    vec![GeminiToolDeclaration {
        function_declarations: tools
            .iter()
            .map(|t| GeminiFunctionDecl {
                name: t.name.clone(),
                description: t.description.clone(),
                parameters: t.parameters.clone(),
            })
            .collect(),
    }]
}

fn parse_response(resp: &GeminiResponse, _model: &str) -> (Message, Usage) {
    let mut text = String::new();
    let mut tool_calls = Vec::new();
    let mut tc_counter = 0u32;

    if let Some(ref candidates) = resp.candidates {
        if let Some(candidate) = candidates.first() {
            if let Some(ref content) = candidate.content {
                for part in &content.parts {
                    match part {
                        GeminiPart::Text { text: t } => text.push_str(t),
                        GeminiPart::FunctionCall { function_call } => {
                            tc_counter += 1;
                            tool_calls.push(ToolCall {
                                id: format!("gemini_tc_{}", tc_counter),
                                name: function_call.name.clone(),
                                arguments: serde_json::to_string(&function_call.args)
                                    .unwrap_or_default(),
                            });
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    let usage = resp
        .usage_metadata
        .as_ref()
        .map(|u| Usage {
            prompt_tokens: u.prompt_token_count,
            completion_tokens: u.candidates_token_count,
            total_tokens: u.total_token_count,
        })
        .unwrap_or_default();

    let message = Message {
        role: Role::Assistant,
        content: if text.is_empty() {
            None
        } else {
            Some(text.clone())
        },
        content_blocks: if text.is_empty() {
            vec![]
        } else {
            vec![crate::types::ContentBlock::Text { text }]
        },
        reasoning: None,
        tool_calls,
        tool_call_id: None,
        images: Vec::new(),
    }
    .normalized();

    (message, usage)
}

#[async_trait]
impl LlmProvider for GeminiProvider {
    fn name(&self) -> &str {
        &self.name
    }

    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse> {
        let (system, contents) = convert_messages(&request.messages);
        let tools = convert_tools(&request.tools);

        let body = GeminiRequest {
            contents,
            system_instruction: system,
            tools,
            generation_config: Some(GenerationConfig {
                temperature: request.temperature,
                max_output_tokens: request.max_tokens,
            }),
        };

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
            if let Ok(err) = serde_json::from_str::<GeminiError>(&text) {
                return Err(anyhow!(
                    "Gemini API error ({}): {}",
                    status,
                    err.error.message
                ));
            }
            return Err(anyhow!("Gemini API error ({}): {}", status, text));
        }

        let api_resp: GeminiResponse = resp
            .json()
            .await
            .context("Failed to parse Gemini response")?;
        let (message, usage) = parse_response(&api_resp, &request.model);

        Ok(ChatResponse {
            message,
            usage,
            model: request.model,
            stop_reason: None, // Gemini API 目前不显式返回 stop_reason
        })
    }

    async fn chat_stream(&self, request: ChatRequest, tx: mpsc::Sender<StreamEvent>) -> Result<()> {
        let (system, contents) = convert_messages(&request.messages);
        let tools = convert_tools(&request.tools);

        let body = GeminiRequest {
            contents,
            system_instruction: system,
            tools,
            generation_config: Some(GenerationConfig {
                temperature: request.temperature,
                max_output_tokens: request.max_tokens,
            }),
        };

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

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            let msg = format!("Gemini API error ({}): {}", status, text);
            let _ = tx.send(StreamEvent::Error(msg.clone())).await;
            return Err(anyhow!(msg));
        }

        // Gemini SSE stream
        use eventsource_stream::Eventsource;
        use futures::StreamExt;

        let mut event_stream = resp.bytes_stream().eventsource();
        let mut full_text = String::new();
        let mut all_tool_calls = Vec::new();
        let mut final_usage = Usage::default();
        let mut tc_counter = 0u32;

        while let Some(event_result) = event_stream.next().await {
            let event = match event_result {
                Ok(ev) => ev,
                Err(e) => {
                    warn!("Gemini SSE error: {}", e);
                    continue;
                }
            };

            let chunk: GeminiResponse = match serde_json::from_str(&event.data) {
                Ok(c) => c,
                Err(e) => {
                    warn!("Failed to parse Gemini chunk: {}", e);
                    continue;
                }
            };

            if let Some(ref u) = chunk.usage_metadata {
                final_usage = Usage {
                    prompt_tokens: u.prompt_token_count,
                    completion_tokens: u.candidates_token_count,
                    total_tokens: u.total_token_count,
                };
            }

            if let Some(ref candidates) = chunk.candidates {
                if let Some(candidate) = candidates.first() {
                    if let Some(ref content) = candidate.content {
                        for part in &content.parts {
                            match part {
                                GeminiPart::Text { text } => {
                                    full_text.push_str(text);
                                    let _ = tx.send(StreamEvent::TextDelta(text.clone())).await;
                                }
                                GeminiPart::FunctionCall { function_call } => {
                                    tc_counter += 1;
                                    let id = format!("gemini_tc_{}", tc_counter);
                                    let args = serde_json::to_string(&function_call.args)
                                        .unwrap_or_default();

                                    let _ = tx
                                        .send(StreamEvent::ToolCallStart {
                                            id: id.clone(),
                                            name: function_call.name.clone(),
                                        })
                                        .await;
                                    let _ = tx
                                        .send(StreamEvent::ToolCallDelta {
                                            id: id.clone(),
                                            arguments: args.clone(),
                                        })
                                        .await;
                                    let _ =
                                        tx.send(StreamEvent::ToolCallEnd { id: id.clone() }).await;

                                    all_tool_calls.push(ToolCall {
                                        id,
                                        name: function_call.name.clone(),
                                        arguments: args,
                                    });
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
        }

        let message = Message {
            role: Role::Assistant,
            content: if full_text.is_empty() {
                None
            } else {
                Some(full_text.clone())
            },
            content_blocks: if full_text.is_empty() {
                vec![]
            } else {
                vec![crate::types::ContentBlock::Text { text: full_text }]
            },
            reasoning: None,
            tool_calls: all_tool_calls,
            tool_call_id: None,
            images: Vec::new(),
        }
        .normalized();

        let _ = tx
            .send(StreamEvent::Done(ChatResponse {
                message,
                usage: final_usage,
                model: request.model,
                stop_reason: None,
            }))
            .await;

        Ok(())
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
            .filter(|m| m.name.contains("gemini"))
            .map(|m| {
                let id = m
                    .name
                    .strip_prefix("models/")
                    .unwrap_or(&m.name)
                    .to_string();
                ModelInfo {
                    name: m.display_name.unwrap_or_else(|| id.clone()),
                    id,
                    provider: "google".into(),
                }
            })
            .collect())
    }
}
