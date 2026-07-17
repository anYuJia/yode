use std::path::PathBuf;

use anyhow::Result;
use tauri::AppHandle;

use super::{
    settings_system::{apply_menu_bar_setting, open_with_destination, stop_sleep_guard},
    DesktopRuntime,
};
use crate::desktop_settings_store::{read_desktop_settings_async, write_desktop_settings_async};
use crate::protocol::{
    DesktopSettingSetRequest, DesktopSettingValue, GeneralSettings, OpenTargetRequest,
};

impl DesktopRuntime {
    pub fn menu_bar_enabled(&self) -> Result<bool> {
        Ok(self
            .general_settings
            .lock()
            .map_err(|_| anyhow::anyhow!("general settings lock poisoned"))?
            .show_in_menu_bar)
    }

    pub fn general_settings_apply(
        &self,
        app: &AppHandle,
        settings: GeneralSettings,
    ) -> Result<GeneralSettings> {
        let effective_mode = permission_mode_from_general_settings(&settings);
        {
            let mut active_mode = self
                .permission_mode
                .lock()
                .map_err(|_| anyhow::anyhow!("permission mode lock poisoned"))?;
            *active_mode = effective_mode.to_string();
        }
        {
            let mut current = self
                .general_settings
                .lock()
                .map_err(|_| anyhow::anyhow!("general settings lock poisoned"))?;
            *current = settings.clone();
        }
        apply_menu_bar_setting(app, settings.show_in_menu_bar)?;
        if !settings.prevent_sleep {
            stop_sleep_guard(&self.sleep_guard);
        }
        Ok(settings)
    }

    pub fn open_target(&self, request: OpenTargetRequest) -> Result<()> {
        let settings = self
            .general_settings
            .lock()
            .map_err(|_| anyhow::anyhow!("general settings lock poisoned"))?
            .clone();
        let target = request
            .target
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(settings.open_destination);
        let path = request
            .path
            .filter(|value| !value.trim().is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| self.workspace_path.clone());
        open_with_destination(&target, &path)
    }

    pub async fn desktop_setting_get(&self, key: String) -> Result<DesktopSettingValue> {
        let settings = read_desktop_settings_async().await?;
        Ok(DesktopSettingValue {
            value: settings.get(&key).cloned(),
            key,
        })
    }

    pub async fn desktop_setting_set(
        &self,
        request: DesktopSettingSetRequest,
    ) -> Result<DesktopSettingValue> {
        let mut settings = read_desktop_settings_async().await?;
        settings.insert(request.key.clone(), request.value.clone());
        write_desktop_settings_async(&settings).await?;
        Ok(DesktopSettingValue {
            key: request.key,
            value: Some(request.value),
        })
    }
}

pub(super) fn default_general_settings() -> GeneralSettings {
    GeneralSettings {
        work_mode: "coding".to_string(),
        default_file_permission: true,
        auto_review: true,
        full_access: true,
        open_destination: "VS Code".to_string(),
        show_in_menu_bar: true,
        bottom_panel: true,
        terminal_location: "bottom".to_string(),
        prevent_sleep: false,
        code_review_policy: "inline".to_string(),
        suggested_prompts: true,
        context_usage: false,
        follow_up_behavior: "queue".to_string(),
        require_opt_enter: false,
        completion_notification: "Only when unfocused".to_string(),
        permission_notification: true,
        question_notification: true,
    }
}

fn permission_mode_from_general_settings(
    settings: &GeneralSettings,
) -> yode_core::permission::PermissionMode {
    if settings.full_access {
        yode_core::permission::PermissionMode::Bypass
    } else if settings.default_file_permission {
        yode_core::permission::PermissionMode::AcceptEdits
    } else {
        yode_core::permission::PermissionMode::Default
    }
}
