mod provider;
mod session;
mod settings;
mod terminal;
mod worktree;

use crate::{protocol, runtime};

pub fn invoke_handler() -> impl Fn(tauri::ipc::Invoke<tauri::Wry>) -> bool + Send + Sync + 'static {
    tauri::generate_handler![
        app_get_bootstrap,
        session::sessions_list,
        session::sessions_create,
        session::sessions_messages,
        session::sessions_clear_messages,
        session::sessions_rename,
        session::sessions_export_markdown,
        session::sessions_compact_local,
        session::sessions_compact_engine,
        project_folder_pick,
        runtime_state_get,
        edit_diff_artifact_read,
        turn_send_message,
        permission_respond,
        ask_user_respond,
        turn_cancel,
        permission_mode_set,
        general_settings_apply,
        open_target,
        import_ai_sessions,
        license_notices,
        configuration_state_get,
        configuration_update,
        configuration_open_file,
        workspace_diagnose,
        workspace_reinstall,
        desktop_setting_get,
        desktop_setting_set,
        personalization_state_get,
        personalization_reset_memories,
        mcp_servers_state,
        mcp_servers_save,
        mcp_server_test,
        mcp_servers_reload,
        settings::browser_clear_data,
        settings::browser_settings_get,
        settings::browser_settings_apply,
        settings::hooks_settings_get,
        settings::hooks_settings_apply,
        worktree::git_settings_get,
        worktree::git_settings_apply,
        worktree::git_current_branch,
        worktree::worktrees_list,
        worktree::worktrees_prune_idle,
        worktree::worktree_delete,
        settings::computer_use_open_accessibility,
        settings::computer_use_open_chrome,
        settings::computer_use_pick_application,
        settings::computer_use_settings_get,
        settings::computer_use_settings_apply,
        terminal::terminal_run,
        terminal::terminal_open,
        terminal::terminal_write,
        terminal::terminal_resize,
        terminal::terminal_close,
        session::sessions_delete,
        session::sessions_update_llm,
        provider::config_get_providers,
        provider::config_save_providers,
        provider::config_get_default_llm,
        provider::config_set_default_llm,
        provider::config_test_provider
    ]
}

#[tauri::command]
fn app_get_bootstrap(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
) -> Result<protocol::Bootstrap, String> {
    runtime.bootstrap().map_err(|err| err.to_string())
}
#[tauri::command]
fn project_folder_pick() -> Option<String> {
    rfd::FileDialog::new()
        .pick_folder()
        .map(|path| path.display().to_string())
}
#[tauri::command]
fn runtime_state_get(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
) -> Result<protocol::RuntimeState, String> {
    runtime.runtime_state().map_err(|err| err.to_string())
}
#[tauri::command]
async fn edit_diff_artifact_read(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
    path: String,
) -> Result<String, String> {
    runtime
        .edit_diff_artifact_read(path)
        .await
        .map_err(|err| err.to_string())
}
#[tauri::command]
async fn turn_send_message(
    app: tauri::AppHandle,
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
    request: protocol::SendMessageRequest,
) -> Result<protocol::TurnAccepted, String> {
    runtime
        .turn_send_message(app, request)
        .await
        .map_err(|err| err.to_string())
}
#[tauri::command]
fn permission_respond(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
    session_id: String,
    turn_id: String,
    allow: bool,
    always_allow: Option<bool>,
) -> Result<(), String> {
    runtime
        .permission_respond(session_id, turn_id, allow, always_allow.unwrap_or(false))
        .map_err(|err| err.to_string())
}
#[tauri::command]
fn ask_user_respond(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
    session_id: String,
    turn_id: String,
    answer: String,
) -> Result<(), String> {
    runtime
        .ask_user_respond(session_id, turn_id, answer)
        .map_err(|err| err.to_string())
}
#[tauri::command]
fn turn_cancel(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
    session_id: String,
    turn_id: String,
) -> Result<(), String> {
    runtime
        .turn_cancel(session_id, turn_id)
        .map_err(|err| err.to_string())
}
#[tauri::command]
fn permission_mode_set(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
    mode: String,
) -> Result<(), String> {
    runtime
        .permission_mode_set(mode)
        .map_err(|err| err.to_string())
}
#[tauri::command]
fn general_settings_apply(
    app: tauri::AppHandle,
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
    settings: protocol::GeneralSettings,
) -> Result<protocol::GeneralSettings, String> {
    runtime
        .general_settings_apply(&app, settings)
        .map_err(|err| err.to_string())
}
#[tauri::command]
fn open_target(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
    request: protocol::OpenTargetRequest,
) -> Result<(), String> {
    runtime.open_target(request).map_err(|err| err.to_string())
}
#[tauri::command]
async fn import_ai_sessions(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
) -> Result<protocol::ImportAiSessionsResult, String> {
    runtime
        .import_ai_sessions()
        .await
        .map_err(|err| err.to_string())
}
#[tauri::command]
async fn license_notices(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
) -> Result<Vec<protocol::LicenseNotice>, String> {
    runtime
        .license_notices()
        .await
        .map_err(|err| err.to_string())
}
#[tauri::command]
async fn configuration_state_get(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
) -> Result<protocol::ConfigurationState, String> {
    runtime
        .configuration_state()
        .await
        .map_err(|err| err.to_string())
}
#[tauri::command]
async fn configuration_update(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
    request: protocol::ConfigurationUpdateRequest,
) -> Result<protocol::ConfigurationState, String> {
    runtime
        .configuration_update(request)
        .await
        .map_err(|err| err.to_string())
}
#[tauri::command]
async fn configuration_open_file(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
    scope: String,
) -> Result<(), String> {
    runtime
        .open_configuration_file(scope)
        .await
        .map_err(|err| err.to_string())
}
#[tauri::command]
async fn workspace_diagnose(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
) -> Result<protocol::WorkspaceDiagnosticsResult, String> {
    runtime
        .diagnose_workspace()
        .await
        .map_err(|err| err.to_string())
}
#[tauri::command]
async fn workspace_reinstall(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
) -> Result<protocol::WorkspaceDiagnosticsResult, String> {
    runtime
        .reinstall_workspace()
        .await
        .map_err(|err| err.to_string())
}
#[tauri::command]
async fn desktop_setting_get(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
    key: String,
) -> Result<protocol::DesktopSettingValue, String> {
    runtime
        .desktop_setting_get(key)
        .await
        .map_err(|err| err.to_string())
}
#[tauri::command]
async fn desktop_setting_set(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
    request: protocol::DesktopSettingSetRequest,
) -> Result<protocol::DesktopSettingValue, String> {
    runtime
        .desktop_setting_set(request)
        .await
        .map_err(|err| err.to_string())
}
#[tauri::command]
async fn personalization_state_get(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
) -> Result<protocol::PersonalizationState, String> {
    runtime
        .personalization_state()
        .await
        .map_err(|err| err.to_string())
}
#[tauri::command]
async fn personalization_reset_memories(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
) -> Result<protocol::DesktopActionResult, String> {
    runtime
        .personalization_reset_memories()
        .await
        .map_err(|err| err.to_string())
}
#[tauri::command]
fn mcp_servers_state(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
) -> Result<protocol::DesktopMcpState, String> {
    runtime.mcp_servers_state().map_err(|err| err.to_string())
}
#[tauri::command]
async fn mcp_servers_save(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
    servers: Vec<protocol::DesktopMcpServer>,
) -> Result<protocol::DesktopMcpState, String> {
    runtime
        .mcp_servers_save(servers)
        .await
        .map_err(|err| err.to_string())
}
#[tauri::command]
fn mcp_server_test(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
    server: protocol::DesktopMcpServer,
) -> Result<protocol::DesktopMcpServerStatus, String> {
    runtime
        .mcp_server_test(server)
        .map_err(|err| err.to_string())
}
#[tauri::command]
async fn mcp_servers_reload(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
) -> Result<protocol::DesktopMcpState, String> {
    runtime
        .mcp_servers_reload()
        .await
        .map_err(|err| err.to_string())
}
