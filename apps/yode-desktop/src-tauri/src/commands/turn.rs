use crate::{protocol, runtime};

#[tauri::command]
pub async fn turn_send_message(
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
pub fn permission_respond(
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
pub fn ask_user_respond(
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
pub fn turn_cancel(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
    session_id: String,
    turn_id: String,
) -> Result<(), String> {
    runtime
        .turn_cancel(session_id, turn_id)
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub fn permission_mode_set(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
    mode: String,
) -> Result<(), String> {
    runtime
        .permission_mode_set(mode)
        .map_err(|err| err.to_string())
}
