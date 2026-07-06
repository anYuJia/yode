use crate::{protocol, runtime};

#[tauri::command]
pub async fn configuration_state_get(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
) -> Result<protocol::ConfigurationState, String> {
    runtime
        .configuration_state()
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn configuration_update(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
    request: protocol::ConfigurationUpdateRequest,
) -> Result<protocol::ConfigurationState, String> {
    runtime
        .configuration_update(request)
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn configuration_open_file(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
    scope: String,
) -> Result<(), String> {
    runtime
        .open_configuration_file(scope)
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn workspace_diagnose(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
) -> Result<protocol::WorkspaceDiagnosticsResult, String> {
    runtime
        .diagnose_workspace()
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn workspace_reinstall(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
) -> Result<protocol::WorkspaceDiagnosticsResult, String> {
    runtime
        .reinstall_workspace()
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn desktop_setting_get(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
    key: String,
) -> Result<protocol::DesktopSettingValue, String> {
    runtime
        .desktop_setting_get(key)
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn desktop_setting_set(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
    request: protocol::DesktopSettingSetRequest,
) -> Result<protocol::DesktopSettingValue, String> {
    runtime
        .desktop_setting_set(request)
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn personalization_state_get(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
) -> Result<protocol::PersonalizationState, String> {
    runtime
        .personalization_state()
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn personalization_reset_memories(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
) -> Result<protocol::DesktopActionResult, String> {
    runtime
        .personalization_reset_memories()
        .await
        .map_err(|err| err.to_string())
}
