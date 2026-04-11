use serde::{Deserialize, Serialize};

// ── OpenAI API request types ────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub(super) struct OpenAiRequest {
    pub(super) model: String,
    pub(super) messages: Vec<OpenAiMessage>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(super) tools: Vec<OpenAiTool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) max_tokens: Option<u32>,
    pub(super) stream: bool,
    /// Request usage stats in the final streaming chunk.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) stream_options: Option<StreamOptions>,
}

#[derive(Debug, Serialize)]
pub(super) struct StreamOptions {
    pub(super) include_usage: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct OpenAiMessage {
    pub(super) role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) content: Option<String>,
    #[serde(
        alias = "thought",
        alias = "reasoning",
        skip_serializing_if = "Option::is_none"
    )]
    pub(super) reasoning_content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) tool_calls: Option<Vec<OpenAiToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) tool_call_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub(super) struct OpenAiToolCall {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) id: Option<String>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub(super) call_type: Option<String>,
    pub(super) function: OpenAiFunction,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) index: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub(super) struct OpenAiFunction {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) arguments: Option<String>,
}

#[derive(Debug, Serialize)]
pub(super) struct OpenAiTool {
    #[serde(rename = "type")]
    pub(super) tool_type: String,
    pub(super) function: OpenAiToolFunction,
}

#[derive(Debug, Serialize)]
pub(super) struct OpenAiToolFunction {
    pub(super) name: String,
    pub(super) description: String,
    pub(super) parameters: serde_json::Value,
}

// ── OpenAI API response types ───────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub(super) struct OpenAiResponse {
    pub(super) choices: Vec<OpenAiChoice>,
    #[serde(default)]
    pub(super) usage: Option<OpenAiUsage>,
    pub(super) model: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct OpenAiChoice {
    pub(super) message: OpenAiMessage,
    #[serde(default)]
    pub(super) finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct OpenAiUsage {
    #[serde(default)]
    pub(super) prompt_tokens: u32,
    #[serde(default)]
    pub(super) completion_tokens: u32,
    #[serde(default)]
    pub(super) total_tokens: u32,
    #[serde(default)]
    pub(super) prompt_tokens_details: Option<OpenAiPromptTokensDetails>,
}

#[derive(Debug, Deserialize)]
pub(super) struct OpenAiPromptTokensDetails {
    #[serde(default)]
    pub(super) cached_tokens: u32,
}

#[derive(Debug, Deserialize)]
pub(super) struct OpenAiModelsResponse {
    pub(super) data: Vec<OpenAiModel>,
}

#[derive(Debug, Deserialize)]
pub(super) struct OpenAiModel {
    pub(super) id: String,
}

// ── Stream chunk types ──────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub(super) struct OpenAiStreamChunk {
    pub(super) choices: Vec<OpenAiStreamChoice>,
    #[serde(default)]
    pub(super) usage: Option<OpenAiUsage>,
    pub(super) model: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct OpenAiStreamChoice {
    pub(super) delta: OpenAiStreamDelta,
    #[serde(default)]
    pub(super) finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct OpenAiStreamDelta {
    #[serde(default)]
    #[allow(dead_code)]
    pub(super) role: Option<String>,
    #[serde(default)]
    pub(super) content: Option<String>,
    #[serde(alias = "thought", alias = "reasoning", default)]
    pub(super) reasoning_content: Option<String>,
    #[serde(default)]
    pub(super) tool_calls: Option<Vec<OpenAiToolCall>>,
}

// ── OpenAI API error types ──────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub(super) struct OpenAiErrorResponse {
    pub(super) error: OpenAiErrorDetail,
}

#[derive(Debug, Deserialize)]
pub(super) struct OpenAiErrorDetail {
    pub(super) message: String,
    #[serde(default)]
    pub(super) code: Option<String>,
}
