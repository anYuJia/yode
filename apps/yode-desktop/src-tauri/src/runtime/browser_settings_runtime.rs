use anyhow::Result;
use serde_json::json;

use super::DesktopRuntime;
use crate::browser_settings::{
    apply_browser_settings_env, browser_settings_from_desktop_settings, normalize_browser_settings,
    validate_browser_settings,
};
use crate::desktop_settings_store::{read_desktop_settings_async, write_desktop_settings_async};
use crate::protocol::{BrowserSettings, DesktopActionResult};

impl DesktopRuntime {
    pub async fn browser_clear_data(&self) -> Result<DesktopActionResult> {
        let mut cleared = Vec::new();
        for path in [
            self.workspace_path.join(".yode").join("browser-cache"),
            dirs::home_dir()
                .unwrap_or_else(|| self.workspace_path.clone())
                .join(".yode")
                .join("browser-data"),
        ] {
            if tokio::fs::try_exists(&path).await? {
                tokio::fs::remove_dir_all(&path).await?;
                cleared.push(path.display().to_string());
            }
            tokio::fs::create_dir_all(&path).await?;
        }
        Ok(DesktopActionResult {
            ok: true,
            message: if cleared.is_empty() {
                "浏览器数据目录已初始化。".to_string()
            } else {
                format!("已清理 {} 个浏览器数据目录。", cleared.len())
            },
            path: Some(self.workspace_path.join(".yode").display().to_string()),
        })
    }

    pub async fn browser_settings_get(&self) -> Result<BrowserSettings> {
        browser_settings_from_desktop_settings(&read_desktop_settings_async().await?)
    }

    pub async fn browser_settings_apply(
        &self,
        settings: BrowserSettings,
    ) -> Result<BrowserSettings> {
        validate_browser_settings(&settings)?;
        let normalized = normalize_browser_settings(settings);
        let mut desktop_settings = read_desktop_settings_async().await?;
        desktop_settings.insert(
            "yode-browser-enabled".to_string(),
            json!(normalized.enabled),
        );
        desktop_settings.insert(
            "yode-browser-annotation-screenshots".to_string(),
            json!(normalized.annotation_screenshots),
        );
        desktop_settings.insert(
            "yode-browser-approval".to_string(),
            json!(normalized.approval_policy),
        );
        desktop_settings.insert(
            "yode-browser-blocked-domains".to_string(),
            json!(normalized.blocked_domains),
        );
        desktop_settings.insert(
            "yode-browser-allowed-domains".to_string(),
            json!(normalized.allowed_domains),
        );
        write_desktop_settings_async(&desktop_settings).await?;
        apply_browser_settings_env(&normalized);
        Ok(normalized)
    }
}
