use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct GeminiRequest {
    pub(super) contents: Vec<GeminiContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) system_instruction: Option<GeminiContent>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(super) tools: Vec<GeminiToolDeclaration>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) generation_config: Option<GenerationConfig>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct GeminiContent {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) role: Option<String>,
    pub(super) parts: Vec<GeminiPart>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub(super) enum GeminiPart {
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
pub(super) struct GeminiFunctionCall {
    pub(super) name: String,
    pub(super) args: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct GeminiFunctionResponse {
    pub(super) name: String,
    pub(super) response: serde_json::Value,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct GenerationConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) max_output_tokens: Option<u32>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct GeminiToolDeclaration {
    pub(super) function_declarations: Vec<GeminiFunctionDecl>,
}

#[derive(Debug, Serialize)]
pub(super) struct GeminiFunctionDecl {
    pub(super) name: String,
    pub(super) description: String,
    pub(super) parameters: serde_json::Value,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct GeminiResponse {
    pub(super) candidates: Option<Vec<GeminiCandidate>>,
    pub(super) usage_metadata: Option<GeminiUsage>,
    #[serde(default)]
    #[allow(dead_code)]
    pub(super) model_version: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct GeminiCandidate {
    pub(super) content: Option<GeminiContent>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct GeminiUsage {
    #[serde(default)]
    pub(super) prompt_token_count: u32,
    #[serde(default)]
    pub(super) candidates_token_count: u32,
    #[serde(default)]
    pub(super) total_token_count: u32,
    #[serde(default)]
    pub(super) cached_content_token_count: u32,
}

#[derive(Debug, Deserialize)]
pub(super) struct GeminiError {
    pub(super) error: GeminiErrorDetail,
}

#[derive(Debug, Deserialize)]
pub(super) struct GeminiErrorDetail {
    pub(super) message: String,
    #[serde(default)]
    #[allow(dead_code)]
    pub(super) code: Option<i32>,
}
