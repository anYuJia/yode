use crate::{protocol, runtime};

#[tauri::command]
pub fn mcp_servers_state(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
) -> Result<protocol::DesktopMcpState, String> {
    runtime.mcp_servers_state().map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn mcp_servers_save(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
    servers: Vec<protocol::DesktopMcpServer>,
) -> Result<protocol::DesktopMcpState, String> {
    runtime
        .mcp_servers_save(servers)
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub fn mcp_server_test(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
    server: protocol::DesktopMcpServer,
) -> Result<protocol::DesktopMcpServerStatus, String> {
    runtime
        .mcp_server_test(server)
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn mcp_servers_reload(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
) -> Result<protocol::DesktopMcpState, String> {
    runtime
        .mcp_servers_reload()
        .await
        .map_err(|err| err.to_string())
}
