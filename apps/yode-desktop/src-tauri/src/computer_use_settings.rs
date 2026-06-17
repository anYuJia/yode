use std::collections::HashSet;
use std::path::Path;

use anyhow::Result;

use crate::desktop_settings_store::{desktop_string_list_setting, desktop_string_setting};
use crate::protocol::ComputerUseSettings;

pub(super) fn computer_use_settings_from_desktop_settings(
    settings: &serde_json::Map<String, serde_json::Value>,
) -> Result<ComputerUseSettings> {
    Ok(normalize_computer_use_settings(ComputerUseSettings {
        any_app_status: desktop_string_setting(settings, "yode-computer-use-anyapp", "uninstalled"),
        chrome_status: desktop_string_setting(settings, "yode-computer-use-chrome", "uninstalled"),
        allowed_apps: desktop_string_list_setting(settings, "yode-computer-use-allowed-apps"),
    }))
}

pub(super) fn normalize_computer_use_settings(
    mut settings: ComputerUseSettings,
) -> ComputerUseSettings {
    settings.any_app_status = normalize_install_status(&settings.any_app_status);
    settings.chrome_status = normalize_install_status(&settings.chrome_status);
    settings.allowed_apps = normalize_app_list(settings.allowed_apps);
    settings
}

pub(super) fn validate_computer_use_settings(settings: &ComputerUseSettings) -> Result<()> {
    for status in [&settings.any_app_status, &settings.chrome_status] {
        if !matches!(
            normalize_install_status(status).as_str(),
            "installed" | "uninstalled" | "installing"
        ) {
            anyhow::bail!("无效的计算机使用状态：{}", status);
        }
    }
    for app in &settings.allowed_apps {
        if normalize_app_name(app).is_none() {
            anyhow::bail!("无效应用名称：{}", app);
        }
    }
    Ok(())
}

pub(super) fn application_display_name(path: &Path) -> String {
    path.file_stem()
        .or_else(|| path.file_name())
        .and_then(|value| value.to_str())
        .and_then(normalize_app_name)
        .unwrap_or_default()
}

fn normalize_install_status(status: &str) -> String {
    match status.trim() {
        "installed" => "installed".to_string(),
        "installing" => "installing".to_string(),
        _ => "uninstalled".to_string(),
    }
}

fn normalize_app_list(values: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut result = Vec::new();
    for value in values {
        if let Some(app) = normalize_app_name(&value) {
            let key = app.to_ascii_lowercase();
            if seen.insert(key) {
                result.push(app);
            }
        }
    }
    result
}

fn normalize_app_name(value: &str) -> Option<String> {
    let name = value.trim().trim_end_matches(".app").trim();
    if name.is_empty() || name.len() > 80 || name.chars().any(|ch| ch == '\0') {
        return None;
    }
    Some(name.to_string())
}

#[cfg(test)]
mod tests {
    use crate::protocol::ComputerUseSettings;

    use super::*;

    #[test]
    fn computer_use_settings_normalize_statuses_and_apps() {
        let settings = normalize_computer_use_settings(ComputerUseSettings {
            any_app_status: "installed".to_string(),
            chrome_status: "unknown".to_string(),
            allowed_apps: vec![
                " Safari.app ".to_string(),
                "safari".to_string(),
                "Cursor".to_string(),
                "".to_string(),
            ],
        });

        assert_eq!(settings.any_app_status, "installed");
        assert_eq!(settings.chrome_status, "uninstalled");
        assert_eq!(settings.allowed_apps, vec!["Safari", "Cursor"]);
    }

    #[test]
    fn computer_use_settings_validation_rejects_null_app_names() {
        let settings = ComputerUseSettings {
            any_app_status: "installed".to_string(),
            chrome_status: "installed".to_string(),
            allowed_apps: vec!["Bad\0App".to_string()],
        };

        assert!(validate_computer_use_settings(&settings).is_err());
    }
}
