use std::path::PathBuf;
use std::process::Command;

use anyhow::{Context, Result};
use serde_json::json;
use tauri::AppHandle;

use super::{
    settings_system::{apply_menu_bar_setting, open_with_destination, stop_sleep_guard},
    DesktopRuntime,
};
use crate::browser_settings::{
    apply_browser_settings_env, browser_settings_from_desktop_settings, normalize_browser_settings,
    validate_browser_settings,
};
use crate::computer_use_settings::{
    application_display_name, computer_use_settings_from_desktop_settings,
    normalize_computer_use_settings, validate_computer_use_settings,
};
use crate::desktop_settings_store::{read_desktop_settings_async, write_desktop_settings_async};
use crate::git_settings::{
    apply_git_settings_env, git_settings_from_desktop_settings, normalize_git_settings,
    validate_git_settings,
};
use crate::hook_settings::{
    hooks_settings_from_desktop_settings, normalize_hooks_settings, validate_hooks_settings,
};
use crate::protocol::{
    BrowserSettings, ComputerUseSettings, DesktopActionResult, DesktopSettingSetRequest,
    DesktopSettingValue, GeneralSettings, GitSettings, HooksSettings, OpenTargetRequest,
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

    pub async fn git_settings_get(&self) -> Result<GitSettings> {
        git_settings_from_desktop_settings(&read_desktop_settings_async().await?)
    }

    pub async fn git_settings_apply(&self, settings: GitSettings) -> Result<GitSettings> {
        let normalized = normalize_git_settings(settings);
        validate_git_settings(&normalized)?;
        let mut desktop_settings = read_desktop_settings_async().await?;
        desktop_settings.insert(
            "yode-git-branch-prefix".to_string(),
            json!(normalized.branch_prefix),
        );
        desktop_settings.insert(
            "yode-git-merge-method".to_string(),
            json!(normalized.merge_method),
        );
        desktop_settings.insert(
            "yode-git-show-pr-icons".to_string(),
            json!(normalized.show_pr_icons),
        );
        desktop_settings.insert(
            "yode-git-always-force-push".to_string(),
            json!(normalized.always_force_push),
        );
        desktop_settings.insert(
            "yode-git-create-draft-prs".to_string(),
            json!(normalized.create_draft_prs),
        );
        desktop_settings.insert(
            "yode-git-auto-delete-worktrees".to_string(),
            json!(normalized.auto_delete_worktrees),
        );
        desktop_settings.insert(
            "yode-git-auto-delete-limit".to_string(),
            json!(normalized.auto_delete_limit),
        );
        desktop_settings.insert(
            "yode-git-commit-instructions".to_string(),
            json!(normalized.commit_instructions),
        );
        desktop_settings.insert(
            "yode-git-pr-instructions".to_string(),
            json!(normalized.pr_instructions),
        );
        write_desktop_settings_async(&desktop_settings).await?;
        apply_git_settings_env(&normalized);
        Ok(normalized)
    }

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
