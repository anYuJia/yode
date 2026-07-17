use anyhow::Result;
use serde_json::json;

use super::DesktopRuntime;
use crate::desktop_settings_store::{read_desktop_settings_async, write_desktop_settings_async};
use crate::hook_settings::{
    hooks_settings_from_desktop_settings, normalize_hooks_settings, validate_hooks_settings,
};
use crate::protocol::HooksSettings;

impl DesktopRuntime {
    pub async fn hooks_settings_get(&self) -> Result<HooksSettings> {
        hooks_settings_from_desktop_settings(&read_desktop_settings_async().await?)
    }

    pub async fn hooks_settings_apply(&self, settings: HooksSettings) -> Result<HooksSettings> {
        let normalized = normalize_hooks_settings(settings);
        validate_hooks_settings(&normalized)?;
        let mut desktop_settings = read_desktop_settings_async().await?;
        desktop_settings.insert("yode-hooks-enabled".to_string(), json!(normalized.enabled));
        desktop_settings.insert("yode-hooks-list".to_string(), json!(normalized.hooks));
        write_desktop_settings_async(&desktop_settings).await?;
        Ok(normalized)
    }
}
