use crate::{protocol, runtime};

#[tauri::command]
pub fn config_get_providers(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
) -> Result<Vec<protocol::DesktopProvider>, String> {
    runtime
        .config_get_providers()
        .map_err(|err: anyhow::Error| err.to_string())
}

#[tauri::command]
pub fn config_save_providers(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
    providers: Vec<protocol::DesktopProvider>,
) -> Result<(), String> {
    runtime
        .config_save_providers(providers)
        .map_err(|err: anyhow::Error| err.to_string())
}

#[tauri::command]
pub fn config_get_default_llm(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
) -> Result<protocol::DefaultLlm, String> {
    runtime
        .config_get_default_llm()
        .map_err(|err: anyhow::Error| err.to_string())
}

#[tauri::command]
pub fn config_set_default_llm(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
    provider: String,
    model: String,
) -> Result<protocol::DefaultLlm, String> {
    runtime
        .config_set_default_llm(provider, model)
        .map_err(|err: anyhow::Error| err.to_string())
}

#[tauri::command]
pub async fn config_test_provider(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
    provider: protocol::DesktopProvider,
) -> Result<(), String> {
    runtime
        .config_test_provider(provider)
        .await
        .map_err(|err: anyhow::Error| err.to_string())
}
