use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::tool::{Tool, ToolCapabilities, ToolContext, ToolErrorType, ToolResult};

pub struct WebBrowserTool;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BrowserRuntimeSettings {
    enabled: bool,
    annotation_screenshots: String,
    approval_policy: String,
    blocked_domains: Vec<String>,
    allowed_domains: Vec<String>,
}

impl Default for BrowserRuntimeSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            annotation_screenshots: "Always include".to_string(),
            approval_policy: "Always ask".to_string(),
            blocked_domains: Vec::new(),
            allowed_domains: Vec::new(),
        }
    }
}

#[async_trait]
impl Tool for WebBrowserTool {
    fn name(&self) -> &str {
        "web_browser"
    }

    fn aliases(&self) -> Vec<String> {
        vec!["WebBrowser".to_string()]
    }

    fn user_facing_name(&self) -> &str {
        "Browser"
    }

    fn activity_description(&self, params: &Value) -> String {
        let action = params
            .get("action")
            .and_then(Value::as_str)
            .unwrap_or("browsing");
        let url = params.get("url").and_then(Value::as_str).unwrap_or("");
        if url.is_empty() {
            format!("Browser: {action}")
        } else {
            format!("Browser: {action} {url}")
        }
    }

    fn description(&self) -> &str {
        "Interact with a real browser to navigate pages, click elements, type text, scroll, evaluate JavaScript, and capture screenshots. Browser actions never report success unless a real executor completed them."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["navigate", "click", "type", "scroll", "screenshot", "evaluate"]
                },
                "url": { "type": "string" },
                "selector": { "type": "string" },
                "text": { "type": "string" },
                "code": { "type": "string" },
                "delta_y": { "type": "integer", "default": 600 }
            },
            "required": ["action"]
        })
    }

    fn capabilities(&self) -> ToolCapabilities {
        ToolCapabilities {
            requires_confirmation: true,
            supports_auto_execution: false,
            read_only: false,
        }
    }

    async fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolResult> {
        let action = params.get("action").and_then(Value::as_str).unwrap_or("");
        let settings = browser_runtime_settings();
        if !settings.enabled {
            return Ok(ToolResult::error_typed(
                "浏览器功能已在设置中关闭。".to_string(),
                ToolErrorType::PermissionDeny,
                true,
                Some("请在 设置 > 浏览器 中开启浏览器功能后重试。".to_string()),
            ));
        }

        let url = params.get("url").and_then(Value::as_str).unwrap_or("");
        let domain = extract_domain(url);
        if let Some(domain) = domain.as_deref() {
            if domain_matches_any(domain, &settings.blocked_domains) {
                return Ok(ToolResult::error_typed(
                    format!("浏览器已拦截域名：{domain}"),
                    ToolErrorType::PermissionDeny,
                    false,
                    Some("请从 设置 > 浏览器 > 已拦截域名 中移除该域名后重试。".to_string()),
                ));
            }
            if action == "navigate"
                && settings.approval_policy == "Never allow"
                && !domain_matches_any(domain, &settings.allowed_domains)
            {
                return Ok(ToolResult::error_typed(
                    format!("当前浏览器审批策略不允许打开未加入白名单的域名：{domain}"),
                    ToolErrorType::PermissionDeny,
                    true,
                    Some("请将该域名加入 设置 > 浏览器 > 已允许域名，或调整授权审批策略。".to_string()),
                ));
            }
        } else if action == "navigate" {
            return Ok(ToolResult::error_typed(
                "navigate 操作需要提供有效 URL。".to_string(),
                ToolErrorType::Validation,
                true,
                Some("请传入包含域名的 http:// 或 https:// URL。".to_string()),
            ));
        }

        Ok(ToolResult::error_typed(
            format!(
                "Browser action '{action}' was not executed because no real browser executor is connected. Mock browser success has been disabled."
            ),
            ToolErrorType::Execution,
            true,
            Some("Install or enable the Yode desktop browser runtime, then retry the browser action.".to_string()),
        ))
    }
}

fn browser_runtime_settings() -> BrowserRuntimeSettings {
    std::env::var("YODE_BROWSER_SETTINGS")
        .ok()
        .and_then(|raw| serde_json::from_str::<BrowserRuntimeSettings>(&raw).ok())
        .unwrap_or_default()
}

fn extract_domain(raw_url: &str) -> Option<String> {
    let trimmed = raw_url.trim();
    if trimmed.is_empty() {
        return None;
    }
    let without_scheme = trimmed
        .strip_prefix("http://")
        .or_else(|| trimmed.strip_prefix("https://"))?;
    let host = without_scheme
        .split('/')
        .next()
        .unwrap_or("")
        .split('@')
        .next_back()
        .unwrap_or("")
        .split(':')
        .next()
        .unwrap_or("")
        .trim()
        .trim_end_matches('.')
        .to_ascii_lowercase();
    if host.is_empty()
        || !host.contains('.')
        || !host
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '.'))
    {
        return None;
    }
    Some(host)
}

fn domain_matches_any(domain: &str, patterns: &[String]) -> bool {
    patterns.iter().any(|pattern| domain_matches(domain, pattern))
}

fn domain_matches(domain: &str, pattern: &str) -> bool {
    let normalized = pattern
        .trim()
        .trim_start_matches("http://")
        .trim_start_matches("https://")
        .trim_start_matches("*.")
        .trim_matches('/')
        .to_ascii_lowercase();
    domain == normalized || domain.ends_with(&format!(".{normalized}"))
}

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;

    use serde_json::json;

    use crate::tool::Tool;

    use super::WebBrowserTool;

    async fn env_lock() -> tokio::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| tokio::sync::Mutex::new(())).lock().await
    }

    #[tokio::test]
    async fn enabled_browser_never_fakes_success_without_executor() {
        let _guard = env_lock().await;
        std::env::remove_var("YODE_BROWSER_SETTINGS");
        let result = WebBrowserTool
            .execute(
                json!({"action":"navigate","url":"https://example.com"}),
                &crate::tool::ToolContext::empty(),
            )
            .await
            .unwrap();
        assert!(result.is_error);
        assert!(result.content.contains("Mock browser success has been disabled"));
    }

    #[test]
    fn web_browser_requires_confirmation_for_external_actions() {
        let caps = WebBrowserTool.capabilities();
        assert!(caps.requires_confirmation);
        assert!(!caps.supports_auto_execution);
        assert!(!caps.read_only);
    }
}
