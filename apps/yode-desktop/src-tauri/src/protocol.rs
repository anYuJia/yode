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
pub struct DefaultLlm {
    pub provider: String,
    pub model: String,
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
    pub metadata: Option<Value>,
    pub images: Vec<DesktopImageOutput>,
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
    #[serde(default)]
    pub images: Vec<DesktopImageInput>,
    pub project_root: Option<String>,
    pub standalone: Option<bool>,
    pub title: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalRunRequest {
    pub session_id: String,
    pub command: String,
    pub cwd: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalRunResponse {
    pub output: String,
    pub cwd: String,
    pub exit_code: i32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalOpenRequest {
    pub session_id: String,
    pub cwd: Option<String>,
    pub cols: Option<u16>,
    pub rows: Option<u16>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalOpenResponse {
    pub session_id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalWriteRequest {
    pub session_id: String,
    pub data: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalResizeRequest {
    pub session_id: String,
    pub cols: u16,
    pub rows: u16,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalOutputEvent {
    pub session_id: String,
    pub data: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalExitEvent {
    pub session_id: String,
    pub exit_code: Option<i32>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopImageInput {
    pub base64: String,
    pub media_type: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopImageOutput {
    pub base64: String,
    pub media_type: String,
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

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GeneralSettings {
    pub work_mode: String,
    pub default_file_permission: bool,
    pub auto_review: bool,
    pub full_access: bool,
    pub open_destination: String,
    pub show_in_menu_bar: bool,
    pub bottom_panel: bool,
    pub terminal_location: String,
    pub prevent_sleep: bool,
    pub code_review_policy: String,
    pub suggested_prompts: bool,
    pub context_usage: bool,
    pub follow_up_behavior: String,
    pub require_opt_enter: bool,
    pub completion_notification: String,
    pub permission_notification: bool,
    pub question_notification: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenTargetRequest {
    pub target: Option<String>,
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportAiSessionsResult {
    pub imported: usize,
    pub skipped: usize,
    pub sessions: Vec<DesktopSession>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LicenseNotice {
    pub name: String,
    pub version: Option<String>,
    pub license: Option<String>,
    pub source: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigurationState {
    pub scope: String,
    pub approval_policy: String,
    pub sandbox_settings: String,
    pub expose_dependencies: bool,
    pub config_path: String,
    pub project_config_path: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticCheck {
    pub name: String,
    pub status: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceDiagnosticsResult {
    pub report_path: String,
    pub checks: Vec<DiagnosticCheck>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigurationUpdateRequest {
    pub scope: String,
    pub approval_policy: String,
    pub sandbox_settings: String,
    pub expose_dependencies: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopSettingSetRequest {
    pub key: String,
    pub value: Value,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopSettingValue {
    pub key: String,
    pub value: Option<Value>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopActionResult {
    pub ok: bool,
    pub message: String,
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonalizationState {
    pub personality: String,
    pub custom_instructions: String,
    pub enable_memories: bool,
    pub skip_tool_chats: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopWorktree {
    pub id: String,
    pub branch: String,
    pub path: String,
    pub status: String,
    pub size: String,
}
