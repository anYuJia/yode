use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Bootstrap {
    pub app_version: &'static str,
    pub workspace_path: String,
    /// 后端计算的唯一工作区信任状态（仓库外存储绑定 path+hash+remote）。
    pub workspace_trusted: bool,
    pub provider: String,
    pub model: String,
    pub permission_mode: String,
    /// 后端计算后的唯一有效权限模式。`permission_mode` 暂留给旧前端兼容，
    /// 新代码必须读取该字段，不能从本地设置推导。
    pub effective_permission_mode: String,
    pub sessions: Vec<DesktopSession>,
    pub runs: Vec<SessionRunState>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopProvider {
    pub id: String,
    pub name: String,
    pub format: String,
    pub enabled: bool,
    /// 永不为空返回：WebView 不接触真实密钥。前端留空表示“保持原密钥”。
    pub api_key: String,
    /// 该 provider 是否已配置密钥（用于前端显示掩码/占位）。
    #[serde(default)]
    pub has_api_key: bool,
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
    /// 会话内消息顺序（sort_order）。分页/向上翻页的游标；旧版响应无此字段。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sort_order: Option<i64>,
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
    /// 统一事件协议版本：老字段继续输出，新字段只增不改；前端对缺失值保持兼容。
    pub schema_version: u32,
    pub session_id: String,
    pub turn_id: String,
    pub seq: u64,
    pub kind: String,
    pub timestamp: String,
    pub payload: Value,
}

/// 线上线型与 yode-runtime 统一事件信封的唯一转换点：
/// 事件构造只允许经 `DesktopEventEnvelope::new`（强类型 kind），
/// 这里仅做字段平铺，不再允许在运行时散落构造裸事件。
impl From<yode_runtime::DesktopEventEnvelope> for DesktopEvent {
    fn from(envelope: yode_runtime::DesktopEventEnvelope) -> Self {
        Self {
            schema_version: envelope.schema_version,
            session_id: envelope.session_id,
            turn_id: envelope.turn_id,
            seq: envelope.seq,
            kind: envelope.kind.as_str().to_string(),
            timestamp: envelope.timestamp,
            payload: envelope.payload,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeState {
    pub active_session_id: Option<String>,
    pub status: String,
    pub permission_mode: String,
    pub effective_permission_mode: String,
    pub context_percent: u8,
    pub tool_calls: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionModeState {
    pub effective_permission_mode: String,
    pub scope: String,
    pub persisted: bool,
    pub bypass_active: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionRunState {
    pub session_id: String,
    pub turn_id: String,
    pub status: String,
    pub updated_at: String,
    pub detail: Option<String>,
    /// turn journal 持久化字段：开始/结束时间、已落盘事件 seq、错误码、取消请求标记。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<String>,
    #[serde(default)]
    pub last_seq: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    #[serde(default)]
    pub cancellation_requested: bool,
}

/// 持久化 turn 事件（payload 已在落盘前脱敏，可安全回放给前端）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnEvent {
    pub session_id: String,
    pub turn_id: String,
    pub seq: i64,
    pub kind: String,
    pub timestamp: String,
    pub payload: Value,
}

/// 会话消息分页结果：按 sort_order 降序返回最近窗口，has_more 指示是否还有更早消息。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionMessagesPage {
    pub messages: Vec<DesktopMessage>,
    pub has_more: bool,
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
    pub effective_permission_mode: String,
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

/// 桌面设置文件加载状态。`loaded: false` 表示文件无效 JSON、根节点不是对象
/// 或不可读；此时设置页必须明确提示“设置文件未加载”，并提供重试/恢复操作。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopSettingsStatus {
    pub loaded: bool,
    pub path: String,
    pub error: Option<String>,
    pub backup_path: Option<String>,
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
pub struct SessionExportResult {
    pub path: String,
    pub message_count: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionCompactResult {
    pub before_count: usize,
    pub after_count: usize,
    pub removed_count: usize,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonalizationState {
    pub personality: String,
    pub custom_instructions: String,
    pub enable_memories: bool,
    pub skip_tool_chats: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserSettings {
    pub enabled: bool,
    pub annotation_screenshots: String,
    pub approval_policy: String,
    pub blocked_domains: Vec<String>,
    pub allowed_domains: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComputerUseSettings {
    pub any_app_status: String,
    pub chrome_status: String,
    pub allowed_apps: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopHookEntry {
    pub name: String,
    pub events: Vec<String>,
    pub command: String,
    #[serde(alias = "timeout_secs")]
    pub timeout_secs: u64,
    #[serde(alias = "can_block")]
    pub can_block: bool,
    pub disabled: bool,
    #[serde(
        default,
        alias = "tool_filter",
        skip_serializing_if = "Option::is_none"
    )]
    pub tool_filter: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HooksSettings {
    pub enabled: bool,
    pub hooks: Vec<DesktopHookEntry>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitSettings {
    pub branch_prefix: String,
    pub merge_method: String,
    pub show_pr_icons: bool,
    pub always_force_push: bool,
    pub create_draft_prs: bool,
    pub auto_delete_worktrees: bool,
    pub auto_delete_limit: u32,
    pub commit_instructions: String,
    pub pr_instructions: String,
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

/// 仅供 WebView 展示的 MCP 环境变量元数据。环境变量值可能包含访问令牌，绝不能
/// 通过 Tauri 响应返回给前端。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopMcpEnv {
    pub key: String,
    pub has_value: bool,
    pub source: String,
}

/// WebView 保存 MCP 配置时提交的一次性环境变量修改。未提供 `value` 表示保留后端
/// 已有值；只有 `clear: true` 才会删除已有值。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopMcpEnvInput {
    pub value: Option<String>,
    #[serde(default)]
    pub clear: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopMcpServerInput {
    pub name: String,
    pub transport: String,
    pub command: Option<String>,
    pub args: Vec<String>,
    pub url: Option<String>,
    #[serde(default)]
    pub env: std::collections::HashMap<String, DesktopMcpEnvInput>,
    pub disabled: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopMcpServer {
    pub name: String,
    pub transport: String,
    pub command: Option<String>,
    pub args: Vec<String>,
    pub url: Option<String>,
    pub env: Vec<DesktopMcpEnv>,
    pub disabled: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopMcpServerStatus {
    pub name: String,
    pub state: String,
    pub detail: String,
    pub tool_count: usize,
    pub resource_count: usize,
    pub template_count: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopMcpState {
    pub config_path: String,
    pub servers: Vec<DesktopMcpServer>,
    pub statuses: Vec<DesktopMcpServerStatus>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCheckResult {
    pub version: String,
    pub release_url: String,
    pub published_at: String,
}
