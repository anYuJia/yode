use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Bootstrap {
    pub app_version: &'static str,
    pub workspace_path: String,
    pub provider: String,
    pub model: String,
    pub permission_mode: String,
    pub sessions: Vec<DesktopSession>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopProvider {
    pub id: String,
    pub name: String,
    pub format: String,
    pub enabled: bool,
    pub api_key: String,
    pub base_url: String,
    pub models: Vec<String>,
    pub gradient: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopSession {
    pub id: String,
    pub title: String,
    pub project: Option<String>,
    pub project_root: Option<String>,
    pub provider: String,
    pub model: String,
    pub updated_at: String,
    pub active: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopMessage {
    pub id: i64,
    pub role: String,
    pub content: Option<String>,
    pub reasoning: Option<String>,
    pub tool_calls_json: Option<String>,
    pub tool_call_id: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateSessionRequest {
    pub project_root: Option<String>,
    pub title: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendMessageRequest {
    pub session_id: Option<String>,
    pub content: String,
    pub project_root: Option<String>,
    pub standalone: Option<bool>,
    pub title: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnAccepted {
    pub session_id: String,
    pub turn_id: String,
    pub session: DesktopSession,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopEvent {
    pub session_id: String,
    pub turn_id: String,
    pub seq: u64,
    pub kind: String,
    pub timestamp: String,
    pub payload: Value,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeState {
    pub active_session_id: Option<String>,
    pub status: String,
    pub permission_mode: String,
    pub context_percent: u8,
    pub tool_calls: String,
}
