use crate::{protocol, runtime};

#[tauri::command]
pub fn app_get_bootstrap(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
) -> Result<protocol::Bootstrap, String> {
    runtime.bootstrap().map_err(|err| err.to_string())
}

#[tauri::command]
pub fn project_folder_pick() -> Option<String> {
    rfd::FileDialog::new()
        .pick_folder()
        .map(|path| path.display().to_string())
}

#[tauri::command]
pub fn runtime_state_get(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
) -> Result<protocol::RuntimeState, String> {
    runtime.runtime_state().map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn edit_diff_artifact_read(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
    path: String,
) -> Result<String, String> {
    runtime
        .edit_diff_artifact_read(path)
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub fn general_settings_apply(
    app: tauri::AppHandle,
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
    settings: protocol::GeneralSettings,
) -> Result<protocol::GeneralSettings, String> {
    runtime
        .general_settings_apply(&app, settings)
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub fn open_target(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
    request: protocol::OpenTargetRequest,
) -> Result<(), String> {
    runtime.open_target(request).map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn import_ai_sessions(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
) -> Result<protocol::ImportAiSessionsResult, String> {
    runtime
        .import_ai_sessions()
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn license_notices(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
) -> Result<Vec<protocol::LicenseNotice>, String> {
    runtime
        .license_notices()
        .await
        .map_err(|err| err.to_string())
}
