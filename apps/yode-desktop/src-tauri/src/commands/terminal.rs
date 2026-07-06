use crate::{protocol, runtime};

#[tauri::command]
pub async fn terminal_run(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
    request: protocol::TerminalRunRequest,
) -> Result<protocol::TerminalRunResponse, String> {
    runtime
        .terminal_run(request)
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub fn terminal_open(
    app: tauri::AppHandle,
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
    request: protocol::TerminalOpenRequest,
) -> Result<protocol::TerminalOpenResponse, String> {
    runtime
        .terminal_open(app, request)
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub fn terminal_write(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
    request: protocol::TerminalWriteRequest,
) -> Result<(), String> {
    runtime
        .terminal_write(request)
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub fn terminal_resize(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
    request: protocol::TerminalResizeRequest,
) -> Result<(), String> {
    runtime
        .terminal_resize(request)
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub fn terminal_close(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
    session_id: String,
) -> Result<(), String> {
    runtime
        .terminal_close(session_id)
        .map_err(|err| err.to_string())
}
