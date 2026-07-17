use crate::{protocol, runtime};

#[tauri::command]
pub async fn check_for_updates(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
) -> Result<Option<protocol::UpdateCheckResult>, String> {
    match runtime.check_for_updates().await {
        Ok(Some(update)) => Ok(Some(protocol::UpdateCheckResult {
            version: update.latest_version,
            release_url: update.download_url,
            published_at: update.published_at,
        })),
        Ok(None) => Ok(None),
        Err(err) => Err(err.to_string()),
    }
}

#[tauri::command]
pub async fn download_update(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
) -> Result<String, String> {
    runtime
        .download_update()
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn has_pending_update(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
) -> Result<bool, String> {
    Ok(runtime.has_pending_update().await)
}

#[tauri::command]
pub async fn apply_downloaded_update(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
) -> Result<bool, String> {
    runtime
        .apply_downloaded_update()
        .await
        .map_err(|err| err.to_string())
}
