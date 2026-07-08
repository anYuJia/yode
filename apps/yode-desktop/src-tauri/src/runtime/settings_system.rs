use std::path::Path;
use std::process::{Child, Command};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use tauri::{AppHandle, Manager};

pub(super) fn start_sleep_guard(sleep_guard: &Arc<Mutex<Option<Child>>>) {
    let Ok(mut guard) = sleep_guard.lock() else {
        return;
    };
    if guard.is_some() {
        return;
    }
    #[cfg(target_os = "macos")]
    let child = Command::new("caffeinate").args(["-dimsu"]).spawn();
    #[cfg(not(target_os = "macos"))]
    let child = Command::new("sh").args(["-c", "sleep 2147483647"]).spawn();
    if let Ok(child) = child {
        *guard = Some(child);
    }
}

pub(super) fn stop_sleep_guard(sleep_guard: &Arc<Mutex<Option<Child>>>) {
    let Ok(mut guard) = sleep_guard.lock() else {
        return;
    };
    if let Some(mut child) = guard.take() {
        if let Err(err) = child.kill() {
            tracing::warn!(error = %err, "Failed to stop sleep guard process");
        }
        if let Err(err) = child.wait() {
            tracing::warn!(error = %err, "Failed to wait for sleep guard process");
        }
    }
}

pub(super) fn open_with_destination(destination: &str, path: &Path) -> Result<()> {
    let dest = destination.to_lowercase();
    if dest.contains("cursor") {
        return open_editor(path, "Cursor", "cursor");
    }
    if dest.contains("terminal") {
        return open_terminal_app(path);
    }
    open_editor(path, "Visual Studio Code", "code")
}

fn open_editor(path: &Path, mac_app: &str, cli: &str) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        let status = Command::new("open")
            .args(["-a", mac_app])
            .arg(path)
            .status();
        if status.is_ok_and(|status| status.success()) {
            return Ok(());
        }
    }
    Command::new(cli)
        .arg(path)
        .spawn()
        .with_context(|| format!("无法启动 {}", mac_app))?;
    Ok(())
}

fn open_terminal_app(path: &Path) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .args(["-a", "Terminal"])
            .arg(path)
            .spawn()
            .context("无法启动 Terminal")?;
        Ok(())
    }
    #[cfg(target_os = "windows")]
    {
        Command::new("cmd")
            .args(["/C", "start", "cmd"])
            .current_dir(path)
            .spawn()
            .context("无法启动系统终端")?;
        return Ok(());
    }
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    {
        Command::new("x-terminal-emulator")
            .current_dir(path)
            .spawn()
            .context("无法启动系统终端")?;
        Ok(())
    }
}

pub(super) fn apply_menu_bar_setting(app: &AppHandle, enabled: bool) -> Result<()> {
    if let Some(tray) = app.tray_by_id("main") {
        tray.set_visible(enabled)?;
        return Ok(());
    }
    if !enabled {
        return Ok(());
    }

    #[allow(unused_imports)]
    use tauri::{
        menu::{Menu, MenuItem},
        tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    };

    let show = MenuItem::with_id(app, "show", "显示 Yode", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &quit])?;
    let _tray = TrayIconBuilder::with_id("main")
        .tooltip("Yode")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show" => {
                show_main_window(app);
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if matches!(
                event,
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                }
            ) {
                let app = tray.app_handle();
                show_main_window(app);
            }
        })
        .build(app)?;
    Ok(())
}

fn show_main_window(app: &AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        tracing::warn!("Main window was unavailable while showing Yode");
        return;
    };
    if let Err(err) = window.show() {
        tracing::warn!(error = %err, "Failed to show main Yode window");
    }
    if let Err(err) = window.set_focus() {
        tracing::warn!(error = %err, "Failed to focus main Yode window");
    }
}
