use crate::{protocol, runtime};

#[tauri::command]
pub async fn browser_clear_data(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
) -> Result<protocol::DesktopActionResult, String> {
    runtime
        .browser_clear_data()
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn browser_settings_get(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
) -> Result<protocol::BrowserSettings, String> {
    runtime
        .browser_settings_get()
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn browser_settings_apply(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
    settings: protocol::BrowserSettings,
) -> Result<protocol::BrowserSettings, String> {
    runtime
        .browser_settings_apply(settings)
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn hooks_settings_get(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
) -> Result<protocol::HooksSettings, String> {
    runtime
        .hooks_settings_get()
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn hooks_settings_apply(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
    settings: protocol::HooksSettings,
) -> Result<protocol::HooksSettings, String> {
    runtime
        .hooks_settings_apply(settings)
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub fn computer_use_open_accessibility(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
) -> Result<protocol::DesktopActionResult, String> {
    runtime
        .computer_use_open_accessibility()
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub fn computer_use_open_chrome(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
) -> Result<protocol::DesktopActionResult, String> {
    runtime
        .computer_use_open_chrome()
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub fn computer_use_pick_application(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
) -> Result<protocol::DesktopActionResult, String> {
    runtime
        .computer_use_pick_application()
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn computer_use_settings_get(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
) -> Result<protocol::ComputerUseSettings, String> {
    runtime
        .computer_use_settings_get()
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn computer_use_settings_apply(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
    settings: protocol::ComputerUseSettings,
) -> Result<protocol::ComputerUseSettings, String> {
    runtime
        .computer_use_settings_apply(settings)
        .await
        .map_err(|err| err.to_string())
}
