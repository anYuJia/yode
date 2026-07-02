use std::path::PathBuf;

use anyhow::Result;

pub(super) fn desktop_settings_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".yode")
        .join("desktop-settings.json")
}

pub(super) fn read_desktop_settings() -> Result<serde_json::Map<String, serde_json::Value>> {
    let path = desktop_settings_path();
    if !path.exists() {
        return Ok(serde_json::Map::new());
    }
    let raw = std::fs::read_to_string(path)?;
    Ok(serde_json::from_str::<serde_json::Value>(&raw)
        .ok()
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default())
}

pub(super) async fn read_desktop_settings_async(
) -> Result<serde_json::Map<String, serde_json::Value>> {
    let path = desktop_settings_path();
    if !tokio::fs::try_exists(&path).await? {
        return Ok(serde_json::Map::new());
    }
    let raw = tokio::fs::read_to_string(path).await?;
    Ok(serde_json::from_str::<serde_json::Value>(&raw)
        .ok()
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default())
}

pub(super) async fn write_desktop_settings_async(
    settings: &serde_json::Map<String, serde_json::Value>,
) -> Result<()> {
    let path = desktop_settings_path();
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(path, serde_json::to_string_pretty(settings)?).await?;
    Ok(())
}

pub(super) fn desktop_string_setting(
    settings: &serde_json::Map<String, serde_json::Value>,
    key: &str,
    fallback: &str,
) -> String {
    settings
        .get(key)
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(fallback)
        .to_string()
}

pub(super) fn desktop_bool_setting(
    settings: &serde_json::Map<String, serde_json::Value>,
    key: &str,
    fallback: bool,
) -> bool {
    settings
        .get(key)
        .and_then(|value| {
            value
                .as_bool()
                .or_else(|| value.as_str().and_then(|raw| raw.parse::<bool>().ok()))
        })
        .unwrap_or(fallback)
}

pub(super) fn desktop_u32_setting(
    settings: &serde_json::Map<String, serde_json::Value>,
    key: &str,
    fallback: u32,
) -> u32 {
    settings
        .get(key)
        .and_then(|value| {
            value
                .as_u64()
                .and_then(|raw| u32::try_from(raw).ok())
                .or_else(|| value.as_str().and_then(|raw| raw.parse::<u32>().ok()))
        })
        .unwrap_or(fallback)
}

pub(super) fn desktop_string_list_setting(
    settings: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Vec<String> {
    settings
        .get(key)
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_string))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn primitive_setting_helpers_parse_expected_shapes() {
        let mut settings = serde_json::Map::new();
        settings.insert("name".to_string(), json!("Yode"));
        settings.insert("enabled".to_string(), json!("true"));
        settings.insert("limit".to_string(), json!("42"));

        assert_eq!(
            desktop_string_setting(&settings, "name", "fallback"),
            "Yode"
        );
        assert!(desktop_bool_setting(&settings, "enabled", false));
        assert_eq!(desktop_u32_setting(&settings, "limit", 1), 42);
        assert_eq!(
            desktop_string_setting(&settings, "missing", "fallback"),
            "fallback"
        );
    }
}
