mod browser_settings;
mod computer_use_settings;
mod desktop_settings_store;
mod git_settings;
mod hook_settings;
mod license_notices;
mod protocol;
mod runtime;
mod session_helpers;
mod session_import;
mod worktree;

mod commands;

use tauri::Manager;

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let runtime = tauri::async_runtime::block_on(runtime::DesktopRuntime::new())
                .expect("failed to initialize desktop runtime");
            app.manage(runtime);
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let keep_in_menu_bar = window
                    .app_handle()
                    .state::<runtime::DesktopRuntime>()
                    .menu_bar_enabled()
                    .unwrap_or(false);
                if keep_in_menu_bar {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(commands::invoke_handler())
        .run(tauri::generate_context!())
        .expect("failed to run Yode desktop app");
}
