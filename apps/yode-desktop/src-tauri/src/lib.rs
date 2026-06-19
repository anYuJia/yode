mod browser_settings;
mod computer_use_settings;
mod desktop_settings_store;
mod git_settings;
mod hook_settings;
mod license_notices;
mod protocol;
mod runtime;
mod session_helpers;
mod session_import;
mod worktree;

use tauri::Manager;

#[tauri::command]
fn app_get_bootstrap(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
) -> Result<protocol::Bootstrap, String> {
    runtime.bootstrap().map_err(|err| err.to_string())
}

#[tauri::command]
fn sessions_list(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
) -> Result<Vec<protocol::DesktopSession>, String> {
    runtime.sessions_list().map_err(|err| err.to_string())
}

#[tauri::command]
fn sessions_create(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
    request: protocol::CreateSessionRequest,
) -> Result<protocol::DesktopSession, String> {
    runtime
        .sessions_create(request)
        .map_err(|err| err.to_string())
}

#[tauri::command]
fn sessions_messages(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
    session_id: String,
) -> Result<Vec<protocol::DesktopMessage>, String> {
    runtime
        .sessions_messages(session_id)
        .map_err(|err| err.to_string())
}

#[tauri::command]
fn sessions_clear_messages(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
    session_id: String,
) -> Result<(), String> {
    runtime
        .sessions_clear_messages(session_id)
        .map_err(|err| err.to_string())
}

#[tauri::command]
fn sessions_rename(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
    session_id: String,
    title: String,
) -> Result<protocol::DesktopSession, String> {
    runtime
        .sessions_rename(session_id, title)
        .map_err(|err| err.to_string())
}

#[tauri::command]
async fn sessions_export_markdown(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
    session_id: String,
) -> Result<protocol::SessionExportResult, String> {
    runtime
        .sessions_export_markdown(session_id)
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
fn sessions_compact_local(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
    session_id: String,
) -> Result<protocol::SessionCompactResult, String> {
    runtime
        .sessions_compact_local(session_id)
        .map_err(|err| err.to_string())
}

#[tauri::command]
async fn sessions_compact_engine(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
    session_id: String,
) -> Result<protocol::SessionCompactResult, String> {
    runtime
        .sessions_compact_engine(session_id)
        .await
        .map_err(|err| err.to_string())
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
fn edit_diff_artifact_read(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
    path: String,
) -> Result<String, String> {
    runtime
        .edit_diff_artifact_read(path)
        .map_err(|err| err.to_string())
}

#[tauri::command]
fn turn_send_message(
    app: tauri::AppHandle,
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
    request: protocol::SendMessageRequest,
) -> Result<protocol::TurnAccepted, String> {
    runtime
        .turn_send_message(app, request)
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
fn import_ai_sessions(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
) -> Result<protocol::ImportAiSessionsResult, String> {
    runtime.import_ai_sessions().map_err(|err| err.to_string())
}

#[tauri::command]
fn license_notices(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
) -> Result<Vec<protocol::LicenseNotice>, String> {
    runtime.license_notices().map_err(|err| err.to_string())
}

#[tauri::command]
fn configuration_state_get(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
) -> Result<protocol::ConfigurationState, String> {
    runtime.configuration_state().map_err(|err| err.to_string())
}

#[tauri::command]
fn configuration_update(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
    request: protocol::ConfigurationUpdateRequest,
) -> Result<protocol::ConfigurationState, String> {
    runtime
        .configuration_update(request)
        .map_err(|err| err.to_string())
}

#[tauri::command]
fn configuration_open_file(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
    scope: String,
) -> Result<(), String> {
    runtime
        .open_configuration_file(scope)
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
fn desktop_setting_get(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
    key: String,
) -> Result<protocol::DesktopSettingValue, String> {
    runtime
        .desktop_setting_get(key)
        .map_err(|err| err.to_string())
}

#[tauri::command]
fn desktop_setting_set(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
    request: protocol::DesktopSettingSetRequest,
) -> Result<protocol::DesktopSettingValue, String> {
    runtime
        .desktop_setting_set(request)
        .map_err(|err| err.to_string())
}

#[tauri::command]
fn personalization_state_get(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
) -> Result<protocol::PersonalizationState, String> {
    runtime
        .personalization_state()
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
fn mcp_servers_save(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
    servers: Vec<protocol::DesktopMcpServer>,
) -> Result<protocol::DesktopMcpState, String> {
    runtime
        .mcp_servers_save(servers)
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
fn mcp_servers_reload(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
) -> Result<protocol::DesktopMcpState, String> {
    runtime.mcp_servers_reload().map_err(|err| err.to_string())
}

#[tauri::command]
async fn browser_clear_data(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
) -> Result<protocol::DesktopActionResult, String> {
    runtime
        .browser_clear_data()
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
fn browser_settings_get(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
) -> Result<protocol::BrowserSettings, String> {
    runtime
        .browser_settings_get()
        .map_err(|err| err.to_string())
}

#[tauri::command]
fn browser_settings_apply(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
    settings: protocol::BrowserSettings,
) -> Result<protocol::BrowserSettings, String> {
    runtime
        .browser_settings_apply(settings)
        .map_err(|err| err.to_string())
}

#[tauri::command]
fn hooks_settings_get(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
) -> Result<protocol::HooksSettings, String> {
    runtime.hooks_settings_get().map_err(|err| err.to_string())
}

#[tauri::command]
fn hooks_settings_apply(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
    settings: protocol::HooksSettings,
) -> Result<protocol::HooksSettings, String> {
    runtime
        .hooks_settings_apply(settings)
        .map_err(|err| err.to_string())
}

#[tauri::command]
fn git_settings_get(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
) -> Result<protocol::GitSettings, String> {
    runtime.git_settings_get().map_err(|err| err.to_string())
}

#[tauri::command]
fn git_settings_apply(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
    settings: protocol::GitSettings,
) -> Result<protocol::GitSettings, String> {
    runtime
        .git_settings_apply(settings)
        .map_err(|err| err.to_string())
}

#[tauri::command]
fn git_current_branch(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
    workspace_path: Option<String>,
) -> Result<Option<String>, String> {
    runtime
        .git_current_branch(workspace_path)
        .map_err(|err| err.to_string())
}

#[tauri::command]
fn worktrees_list(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
) -> Result<Vec<protocol::DesktopWorktree>, String> {
    runtime.worktrees_list().map_err(|err| err.to_string())
}

#[tauri::command]
fn worktrees_prune_idle(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
) -> Result<protocol::DesktopActionResult, String> {
    runtime
        .worktrees_prune_idle()
        .map_err(|err| err.to_string())
}

#[tauri::command]
fn worktree_delete(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
    path: String,
) -> Result<protocol::DesktopActionResult, String> {
    runtime.worktree_delete(path).map_err(|err| err.to_string())
}

#[tauri::command]
fn computer_use_open_accessibility(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
) -> Result<protocol::DesktopActionResult, String> {
    runtime
        .computer_use_open_accessibility()
        .map_err(|err| err.to_string())
}

#[tauri::command]
fn computer_use_open_chrome(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
) -> Result<protocol::DesktopActionResult, String> {
    runtime
        .computer_use_open_chrome()
        .map_err(|err| err.to_string())
}

#[tauri::command]
fn computer_use_pick_application(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
) -> Result<protocol::DesktopActionResult, String> {
    runtime
        .computer_use_pick_application()
        .map_err(|err| err.to_string())
}

#[tauri::command]
fn computer_use_settings_get(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
) -> Result<protocol::ComputerUseSettings, String> {
    runtime
        .computer_use_settings_get()
        .map_err(|err| err.to_string())
}

#[tauri::command]
fn computer_use_settings_apply(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
    settings: protocol::ComputerUseSettings,
) -> Result<protocol::ComputerUseSettings, String> {
    runtime
        .computer_use_settings_apply(settings)
        .map_err(|err| err.to_string())
}

#[tauri::command]
fn terminal_run(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
    request: protocol::TerminalRunRequest,
) -> Result<protocol::TerminalRunResponse, String> {
    runtime.terminal_run(request).map_err(|err| err.to_string())
}

#[tauri::command]
fn terminal_open(
    app: tauri::AppHandle,
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
    request: protocol::TerminalOpenRequest,
) -> Result<protocol::TerminalOpenResponse, String> {
    runtime
        .terminal_open(app, request)
        .map_err(|err| err.to_string())
}

#[tauri::command]
fn terminal_write(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
    request: protocol::TerminalWriteRequest,
) -> Result<(), String> {
    runtime
        .terminal_write(request)
        .map_err(|err| err.to_string())
}

#[tauri::command]
fn terminal_resize(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
    request: protocol::TerminalResizeRequest,
) -> Result<(), String> {
    runtime
        .terminal_resize(request)
        .map_err(|err| err.to_string())
}

#[tauri::command]
fn terminal_close(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
    session_id: String,
) -> Result<(), String> {
    runtime
        .terminal_close(session_id)
        .map_err(|err| err.to_string())
}

#[tauri::command]
fn sessions_delete(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
    session_id: String,
) -> Result<(), String> {
    runtime
        .sessions_delete(session_id)
        .map_err(|err| err.to_string())
}

#[tauri::command]
fn sessions_update_llm(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
    session_id: String,
    provider: String,
    model: String,
) -> Result<(), String> {
    runtime
        .sessions_update_llm(session_id, provider, model)
        .map_err(|err| err.to_string())
}

#[tauri::command]
fn config_get_providers(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
) -> Result<Vec<protocol::DesktopProvider>, String> {
    runtime
        .config_get_providers()
        .map_err(|err: anyhow::Error| err.to_string())
}

#[tauri::command]
fn config_save_providers(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
    providers: Vec<protocol::DesktopProvider>,
) -> Result<(), String> {
    runtime
        .config_save_providers(providers)
        .map_err(|err: anyhow::Error| err.to_string())
}

#[tauri::command]
fn config_get_default_llm(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
) -> Result<protocol::DefaultLlm, String> {
    runtime
        .config_get_default_llm()
        .map_err(|err: anyhow::Error| err.to_string())
}

#[tauri::command]
fn config_set_default_llm(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
    provider: String,
    model: String,
) -> Result<protocol::DefaultLlm, String> {
    runtime
        .config_set_default_llm(provider, model)
        .map_err(|err: anyhow::Error| err.to_string())
}

#[tauri::command]
async fn config_test_provider(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
    provider: protocol::DesktopProvider,
) -> Result<(), String> {
    runtime
        .config_test_provider(provider)
        .await
        .map_err(|err: anyhow::Error| err.to_string())
}

pub fn run() {
    let runtime = runtime::DesktopRuntime::new().expect("failed to initialize desktop runtime");

    tauri::Builder::default()
        .manage(runtime)
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let keep_in_menu_bar = window
                    .app_handle()
                    .state::<runtime::DesktopRuntime>()
                    .menu_bar_enabled()
                    .unwrap_or(false);
                if keep_in_menu_bar {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            app_get_bootstrap,
            sessions_list,
            sessions_create,
            sessions_messages,
            sessions_clear_messages,
            sessions_rename,
            sessions_export_markdown,
            sessions_compact_local,
            sessions_compact_engine,
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
            browser_clear_data,
            browser_settings_get,
            browser_settings_apply,
            hooks_settings_get,
            hooks_settings_apply,
            git_settings_get,
            git_settings_apply,
            git_current_branch,
            worktrees_list,
            worktrees_prune_idle,
            worktree_delete,
            computer_use_open_accessibility,
            computer_use_open_chrome,
            computer_use_pick_application,
            computer_use_settings_get,
            computer_use_settings_apply,
            terminal_run,
            terminal_open,
            terminal_write,
            terminal_resize,
            terminal_close,
            sessions_delete,
            sessions_update_llm,
            config_get_providers,
            config_save_providers,
            config_get_default_llm,
            config_set_default_llm,
            config_test_provider
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Yode desktop app");
}
