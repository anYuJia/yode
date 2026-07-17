use std::process::Command;

use anyhow::{Context, Result};
use serde_json::json;

use super::DesktopRuntime;
use crate::computer_use_settings::{
    application_display_name, computer_use_settings_from_desktop_settings,
    normalize_computer_use_settings, validate_computer_use_settings,
};
use crate::desktop_settings_store::{read_desktop_settings_async, write_desktop_settings_async};
use crate::protocol::{ComputerUseSettings, DesktopActionResult};

impl DesktopRuntime {
    pub fn computer_use_open_accessibility(&self) -> Result<DesktopActionResult> {
        #[cfg(target_os = "macos")]
        {
            match Command::new("open")
                .arg(
                    "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility",
                )
                .status()
            {
                Ok(status) if status.success() => {}
                Ok(status) => tracing::warn!(
                    status = ?status,
                    "Opening macOS accessibility settings exited unsuccessfully"
                ),
                Err(err) => tracing::warn!(
                    error = %err,
                    "Failed to open macOS accessibility settings"
                ),
            }
        }
        Ok(DesktopActionResult {
            ok: true,
            message: "已打开系统辅助功能权限设置，请为 Yode 授权。".to_string(),
            path: None,
        })
    }

    pub fn computer_use_open_chrome(&self) -> Result<DesktopActionResult> {
        #[cfg(target_os = "macos")]
        let status = Command::new("open")
            .args(["-a", "Google Chrome"])
            .status()
            .context("无法打开 Google Chrome")?;

        #[cfg(not(target_os = "macos"))]
        let status = Command::new("google-chrome")
            .status()
            .or_else(|_| Command::new("chrome").status())
            .context("无法打开 Google Chrome")?;

        Ok(DesktopActionResult {
            ok: status.success(),
            message: if status.success() {
                "已打开 Google Chrome，请确认扩展连接状态。".to_string()
            } else {
                "尝试打开 Google Chrome 失败，请确认已安装浏览器。".to_string()
            },
            path: None,
        })
    }

    pub fn computer_use_pick_application(&self) -> Result<DesktopActionResult> {
        let dialog = rfd::FileDialog::new().set_title("选择始终允许的应用");
        #[cfg(target_os = "macos")]
        let dialog = dialog.set_directory("/Applications");
        let Some(path) = dialog.pick_folder() else {
            return Ok(DesktopActionResult {
                ok: false,
                message: "已取消选择应用。".to_string(),
                path: None,
            });
        };
        let app_name = application_display_name(&path);
        if app_name.trim().is_empty() {
            anyhow::bail!("无法识别应用名称。");
        }
        Ok(DesktopActionResult {
            ok: true,
            message: app_name,
            path: Some(path.display().to_string()),
        })
    }

    pub async fn computer_use_settings_get(&self) -> Result<ComputerUseSettings> {
        computer_use_settings_from_desktop_settings(&read_desktop_settings_async().await?)
    }

    pub async fn computer_use_settings_apply(
        &self,
        settings: ComputerUseSettings,
    ) -> Result<ComputerUseSettings> {
        validate_computer_use_settings(&settings)?;
        let normalized = normalize_computer_use_settings(settings);
        let mut desktop_settings = read_desktop_settings_async().await?;
        desktop_settings.insert(
            "yode-computer-use-anyapp".to_string(),
            json!(normalized.any_app_status),
        );
        desktop_settings.insert(
            "yode-computer-use-chrome".to_string(),
            json!(normalized.chrome_status),
        );
        desktop_settings.insert(
            "yode-computer-use-allowed-apps".to_string(),
            json!(normalized.allowed_apps),
        );
        write_desktop_settings_async(&desktop_settings).await?;
        Ok(normalized)
    }
}
