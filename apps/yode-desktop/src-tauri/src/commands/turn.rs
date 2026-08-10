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
    app: tauri::AppHandle,
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
    session_id: String,
    turn_id: String,
) -> Result<(), String> {
    runtime
        .turn_cancel_request(app, session_id, turn_id)
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn permission_mode_set(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
    mode: String,
    bypass_confirmed: Option<bool>,
    scope: Option<String>,
) -> Result<protocol::PermissionModeState, String> {
    runtime
        .permission_mode_set(mode, bypass_confirmed.unwrap_or(false), scope)
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub fn turn_events_since(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
    session_id: String,
    turn_id: String,
    since_seq: i64,
    limit: Option<usize>,
) -> Result<Vec<protocol::TurnEvent>, String> {
    runtime
        .turn_events_since(session_id, turn_id, since_seq, limit)
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub fn turn_recent_events(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
    session_id: String,
    turn_id: String,
    limit: usize,
) -> Result<Vec<protocol::TurnEvent>, String> {
    runtime
        .turn_recent_events(session_id, turn_id, limit)
        .map_err(|err| err.to_string())
}
