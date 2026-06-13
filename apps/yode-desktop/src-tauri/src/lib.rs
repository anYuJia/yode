mod protocol;
mod runtime;

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
fn terminal_run(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
    request: protocol::TerminalRunRequest,
) -> Result<protocol::TerminalRunResponse, String> {
    runtime.terminal_run(request).map_err(|err| err.to_string())
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
        .invoke_handler(tauri::generate_handler![
            app_get_bootstrap,
            sessions_list,
            sessions_create,
            sessions_messages,
            project_folder_pick,
            runtime_state_get,
            turn_send_message,
            permission_respond,
            ask_user_respond,
            turn_cancel,
            permission_mode_set,
            terminal_run,
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
