use std::collections::HashSet;

use anyhow::Result;

use crate::desktop_settings_store::{
    desktop_bool_setting, desktop_string_list_setting, desktop_string_setting,
};
use crate::protocol::BrowserSettings;

pub(super) fn browser_settings_from_desktop_settings(
    settings: &serde_json::Map<String, serde_json::Value>,
) -> Result<BrowserSettings> {
    Ok(normalize_browser_settings(BrowserSettings {
        enabled: desktop_bool_setting(settings, "yode-browser-enabled", true),
        annotation_screenshots: desktop_string_setting(
            settings,
            "yode-browser-annotation-screenshots",
            "Always include",
        ),
        approval_policy: desktop_string_setting(settings, "yode-browser-approval", "Always ask"),
        blocked_domains: desktop_string_list_setting(settings, "yode-browser-blocked-domains"),
        allowed_domains: desktop_string_list_setting(settings, "yode-browser-allowed-domains"),
    }))
}

pub(super) fn normalize_browser_settings(mut settings: BrowserSettings) -> BrowserSettings {
    settings.annotation_screenshots =
        normalize_browser_choice(&settings.annotation_screenshots, "Always include");
    settings.approval_policy = normalize_browser_choice(&settings.approval_policy, "Always ask");
    settings.blocked_domains = normalize_domain_list(settings.blocked_domains);
    settings.allowed_domains = normalize_domain_list(settings.allowed_domains);
    settings
}

pub(super) fn validate_browser_settings(settings: &BrowserSettings) -> Result<()> {
    for value in [&settings.annotation_screenshots, &settings.approval_policy] {
        if value.trim().is_empty() {
            anyhow::bail!("浏览器策略不能为空。");
        }
    }
    for domain in settings
        .blocked_domains
        .iter()
        .chain(settings.allowed_domains.iter())
    {
        if normalize_domain(domain).is_none() {
            anyhow::bail!("无效域名：{}", domain);
        }
    }
    Ok(())
}

pub(super) fn apply_browser_settings_env(settings: &BrowserSettings) {
    if let Ok(json) = serde_json::to_string(settings) {
        std::env::set_var("YODE_BROWSER_SETTINGS", json);
    }
}

fn normalize_browser_choice(value: &str, fallback: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        fallback.to_string()
    } else {
        trimmed.to_string()
    }
}

fn normalize_domain_list(values: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut result = Vec::new();
    for value in values {
        if let Some(domain) = normalize_domain(&value) {
            if seen.insert(domain.clone()) {
                result.push(domain);
            }
        }
    }
    result
}

fn normalize_domain(value: &str) -> Option<String> {
    let trimmed = value
        .trim()
        .trim_start_matches("http://")
        .trim_start_matches("https://")
        .trim_start_matches("*.")
        .trim_matches('/')
        .to_ascii_lowercase();
    let domain = trimmed
        .split('/')
        .next()
        .unwrap_or("")
        .split(':')
        .next()
        .unwrap_or("");
    if domain.is_empty()
        || !domain
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '.'))
        || !domain.contains('.')
    {
        return None;
    }
    Some(domain.to_string())
}

#[cfg(test)]
mod tests {
    use crate::protocol::BrowserSettings;

    use super::*;

    #[test]
    fn browser_settings_normalize_domains_and_defaults() {
        let settings = normalize_browser_settings(BrowserSettings {
            enabled: true,
            annotation_screenshots: "".to_string(),
            approval_policy: "  Ask once  ".to_string(),
            blocked_domains: vec![
                "https://Example.com/path".to_string(),
                "*.example.com".to_string(),
                "invalid".to_string(),
            ],
            allowed_domains: vec!["OpenAI.com:443".to_string()],
        });

        assert_eq!(settings.annotation_screenshots, "Always include");
        assert_eq!(settings.approval_policy, "Ask once");
        assert_eq!(settings.blocked_domains, vec!["example.com"]);
        assert_eq!(settings.allowed_domains, vec!["openai.com"]);
    }

    #[test]
    fn browser_settings_validate_rejects_invalid_domains() {
        let settings = BrowserSettings {
            enabled: true,
            annotation_screenshots: "Always include".to_string(),
            approval_policy: "Always ask".to_string(),
            blocked_domains: vec!["localhost".to_string()],
            allowed_domains: Vec::new(),
        };

        assert!(validate_browser_settings(&settings).is_err());
    }
}
