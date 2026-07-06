use crate::{protocol, runtime};

#[tauri::command]
pub async fn git_settings_get(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
) -> Result<protocol::GitSettings, String> {
    runtime
        .git_settings_get()
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn git_settings_apply(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
    settings: protocol::GitSettings,
) -> Result<protocol::GitSettings, String> {
    runtime
        .git_settings_apply(settings)
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn git_current_branch(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
    workspace_path: Option<String>,
) -> Result<Option<String>, String> {
    runtime
        .git_current_branch(workspace_path)
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn worktrees_list(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
) -> Result<Vec<protocol::DesktopWorktree>, String> {
    runtime
        .worktrees_list()
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn worktrees_prune_idle(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
) -> Result<protocol::DesktopActionResult, String> {
    runtime
        .worktrees_prune_idle()
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn worktree_delete(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
    path: String,
) -> Result<protocol::DesktopActionResult, String> {
    runtime
        .worktree_delete(path)
        .await
        .map_err(|err| err.to_string())
}
