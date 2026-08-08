use std::path::PathBuf;

use anyhow::Result;
use tauri::AppHandle;

use super::{
    settings_system::{apply_menu_bar_setting, open_with_destination, stop_sleep_guard},
    DesktopRuntime,
};
use crate::desktop_settings_store::{read_desktop_settings_async, update_desktop_settings_async};
use crate::protocol::{
    DesktopSettingSetRequest, DesktopSettingValue, DesktopSettingsStatus, GeneralSettings,
    OpenTargetRequest,
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
        mut settings: GeneralSettings,
    ) -> Result<GeneralSettings> {
        // 通用设置不再拥有权限写入口。full_access 只是后端有效模式的只读投影，
        // 防止设置页挂载/保存普通偏好时静默切换权限。
        let effective_mode = self
            .permission_mode
            .lock()
            .map_err(|_| anyhow::anyhow!("permission mode lock poisoned"))?
            .parse::<yode_core::permission::PermissionMode>()
            .unwrap_or(yode_core::permission::PermissionMode::Default);
        settings.full_access = effective_mode == yode_core::permission::PermissionMode::Bypass;
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

    /// 桌面设置文件加载状态：损坏文件在此如实报告，绝不静默回退默认值。
    pub fn desktop_settings_status(&self) -> Result<DesktopSettingsStatus> {
        crate::desktop_settings_store::desktop_settings_status()
    }

    /// 用户显式恢复损坏的桌面设置文件：备份原文件后生成新配置。
    pub fn desktop_settings_restore(&self) -> Result<DesktopSettingsStatus> {
        crate::desktop_settings_store::restore_desktop_settings()
    }

    pub async fn desktop_setting_set(
        &self,
        request: DesktopSettingSetRequest,
    ) -> Result<DesktopSettingValue> {
        let key = request.key.clone();
        let value = request.value.clone();
        update_desktop_settings_async(move |settings| {
            settings.insert(key, value);
            Ok(())
        })
        .await?;
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
        full_access: false,
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
