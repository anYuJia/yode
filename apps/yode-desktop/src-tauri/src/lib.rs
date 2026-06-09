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

pub fn run() {
    let runtime = runtime::DesktopRuntime::new().expect("failed to initialize desktop runtime");

    tauri::Builder::default()
        .manage(runtime)
        .invoke_handler(tauri::generate_handler![
            app_get_bootstrap,
            sessions_list,
            sessions_create,
            runtime_state_get,
            turn_send_message
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Yode desktop app");
}
