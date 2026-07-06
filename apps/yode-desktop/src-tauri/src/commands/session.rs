use crate::{protocol, runtime};

#[tauri::command]
pub fn sessions_list(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
) -> Result<Vec<protocol::DesktopSession>, String> {
    runtime.sessions_list().map_err(|err| err.to_string())
}

#[tauri::command]
pub fn sessions_create(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
    request: protocol::CreateSessionRequest,
) -> Result<protocol::DesktopSession, String> {
    runtime
        .sessions_create(request)
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub fn sessions_messages(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
    session_id: String,
) -> Result<Vec<protocol::DesktopMessage>, String> {
    runtime
        .sessions_messages(session_id)
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub fn sessions_clear_messages(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
    session_id: String,
) -> Result<(), String> {
    runtime
        .sessions_clear_messages(session_id)
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub fn sessions_rename(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
    session_id: String,
    title: String,
) -> Result<protocol::DesktopSession, String> {
    runtime
        .sessions_rename(session_id, title)
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn sessions_export_markdown(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
    session_id: String,
) -> Result<protocol::SessionExportResult, String> {
    runtime
        .sessions_export_markdown(session_id)
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub fn sessions_compact_local(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
    session_id: String,
) -> Result<protocol::SessionCompactResult, String> {
    runtime
        .sessions_compact_local(session_id)
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn sessions_compact_engine(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
    session_id: String,
) -> Result<protocol::SessionCompactResult, String> {
    runtime
        .sessions_compact_engine(session_id)
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub fn sessions_delete(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
    session_id: String,
) -> Result<(), String> {
    runtime
        .sessions_delete(session_id)
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub fn sessions_update_llm(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
    session_id: String,
    provider: String,
    model: String,
) -> Result<(), String> {
    runtime
        .sessions_update_llm(session_id, provider, model)
        .map_err(|err| err.to_string())
}
