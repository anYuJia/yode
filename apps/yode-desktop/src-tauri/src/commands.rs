mod configuration;
mod mcp;
mod provider;
mod session;
mod settings;
mod terminal;
mod turn;
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
        turn::turn_send_message,
        turn::permission_respond,
        turn::ask_user_respond,
        turn::turn_cancel,
        turn::permission_mode_set,
        general_settings_apply,
        open_target,
        import_ai_sessions,
        license_notices,
        configuration::configuration_state_get,
        configuration::configuration_update,
        configuration::configuration_open_file,
        configuration::workspace_diagnose,
        configuration::workspace_reinstall,
        configuration::desktop_setting_get,
        configuration::desktop_setting_set,
        configuration::personalization_state_get,
        configuration::personalization_reset_memories,
        mcp::mcp_servers_state,
        mcp::mcp_servers_save,
        mcp::mcp_server_test,
        mcp::mcp_servers_reload,
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
