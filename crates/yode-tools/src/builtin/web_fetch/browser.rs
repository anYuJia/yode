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
            .and_then(|v| v.as_str())
            .unwrap_or("browsing");
        let url = params.get("url").and_then(|v| v.as_str()).unwrap_or("");
        if !url.is_empty() {
            format!("Browser: {} {}", action, url)
        } else {
            format!("Browser: {}", action)
        }
    }

    fn description(&self) -> &str {
        "Interact with a web browser to navigate pages, click elements, type text, and capture screenshots. \
         Use this for testing web applications or accessing dynamic content."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["navigate", "click", "type", "scroll", "screenshot", "evaluate"],
                    "description": "The browser action to perform"
                },
                "url": {
                    "type": "string",
                    "description": "URL to navigate to (for 'navigate')"
                },
                "selector": {
                    "type": "string",
                    "description": "CSS selector for the element (for 'click', 'type')"
                },
                "text": {
                    "type": "string",
                    "description": "Text to type (for 'type')"
                },
                "code": {
                    "type": "string",
                    "description": "JavaScript code to evaluate (for 'evaluate')"
                }
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
        let action = params.get("action").and_then(|v| v.as_str()).unwrap_or("");
        let settings = browser_runtime_settings();
        if !settings.enabled {
            return Ok(ToolResult::error_typed(
                "浏览器功能已在设置中关闭。".to_string(),
                ToolErrorType::PermissionDeny,
                true,
                Some("请在 设置 > 浏览器 中开启浏览器功能后重试。".to_string()),
            ));
        }

        let url = params.get("url").and_then(|v| v.as_str()).unwrap_or("");
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
                    Some(
                        "请将该域名加入 设置 > 浏览器 > 已允许域名，或调整授权审批策略。"
                            .to_string(),
                    ),
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

        // Mock browser execution
        let msg = match action {
            "navigate" => format!("Navigated to {}", url),
            "click" => format!(
                "Clicked element: {}",
                params
                    .get("selector")
                    .and_then(|v| v.as_str())
                    .unwrap_or("?")
            ),
            "screenshot" => "Captured screenshot (stored in session artifacts).".to_string(),
            _ => format!("Performed browser action: {}", action),
        };

        Ok(ToolResult::success_with_metadata(
            msg,
            json!({
                "action": action,
                "url": if url.is_empty() { Value::Null } else { json!(url) },
                "domain": domain,
                "browser_enabled": settings.enabled,
                "approval_policy": settings.approval_policy,
                "annotation_screenshots": settings.annotation_screenshots,
                "domain_allowed": domain
                    .as_deref()
                    .map(|domain| domain_matches_any(domain, &settings.allowed_domains))
                    .unwrap_or(false),
                "domain_blocked": false,
                "executor": "mock",
            }),
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
    patterns
        .iter()
        .any(|pattern| domain_matches(domain, pattern))
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
    use std::sync::{Mutex, OnceLock};

    use serde_json::json;

    use crate::tool::Tool;

    use super::WebBrowserTool;

    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(())).lock().unwrap()
    }

    fn set_browser_settings(settings: serde_json::Value) {
        std::env::set_var("YODE_BROWSER_SETTINGS", settings.to_string());
    }

    #[tokio::test]
    async fn web_browser_formats_navigate_and_click_actions() {
        let _guard = env_lock();
        std::env::remove_var("YODE_BROWSER_SETTINGS");
        let navigate = WebBrowserTool
            .execute(
                json!({"action":"navigate","url":"https://example.com"}),
                &crate::tool::ToolContext::empty(),
            )
            .await
            .unwrap();
        assert!(!navigate.is_error);
        assert!(navigate
            .content
            .contains("Navigated to https://example.com"));
        assert_eq!(
            navigate.metadata.as_ref().unwrap()["domain"],
            json!("example.com")
        );

        let click = WebBrowserTool
            .execute(
                json!({"action":"click","selector":"#submit"}),
                &crate::tool::ToolContext::empty(),
            )
            .await
            .unwrap();
        assert!(!click.is_error);
        assert!(click.content.contains("Clicked element: #submit"));
    }

    #[tokio::test]
    async fn web_browser_formats_screenshot_and_generic_actions() {
        let _guard = env_lock();
        set_browser_settings(json!({
            "enabled": true,
            "annotationScreenshots": "Never include",
            "approvalPolicy": "Always ask",
            "blockedDomains": [],
            "allowedDomains": []
        }));
        let screenshot = WebBrowserTool
            .execute(
                json!({"action":"screenshot"}),
                &crate::tool::ToolContext::empty(),
            )
            .await
            .unwrap();
        assert!(screenshot.content.contains("Captured screenshot"));
        assert_eq!(
            screenshot.metadata.as_ref().unwrap()["annotation_screenshots"],
            json!("Never include")
        );

        let evaluate = WebBrowserTool
            .execute(
                json!({"action":"evaluate","code":"1+1"}),
                &crate::tool::ToolContext::empty(),
            )
            .await
            .unwrap();
        assert!(evaluate
            .content
            .contains("Performed browser action: evaluate"));
    }

    #[test]
    fn web_browser_requires_confirmation_for_external_actions() {
        let caps = WebBrowserTool.capabilities();
        assert!(caps.requires_confirmation);
        assert!(!caps.supports_auto_execution);
        assert!(!caps.read_only);
    }

    #[tokio::test]
    async fn web_browser_rejects_when_disabled() {
        let _guard = env_lock();
        set_browser_settings(json!({
            "enabled": false,
            "annotationScreenshots": "Always include",
            "approvalPolicy": "Always ask",
            "blockedDomains": [],
            "allowedDomains": []
        }));

        let result = WebBrowserTool
            .execute(
                json!({"action":"navigate","url":"https://example.com"}),
                &crate::tool::ToolContext::empty(),
            )
            .await
            .unwrap();
        assert!(result.is_error);
        assert!(result.content.contains("已在设置中关闭"));
    }

    #[tokio::test]
    async fn web_browser_rejects_blocked_domain() {
        let _guard = env_lock();
        set_browser_settings(json!({
            "enabled": true,
            "annotationScreenshots": "Always include",
            "approvalPolicy": "Always allow",
            "blockedDomains": ["example.com"],
            "allowedDomains": []
        }));

        let result = WebBrowserTool
            .execute(
                json!({"action":"navigate","url":"https://docs.example.com/path"}),
                &crate::tool::ToolContext::empty(),
            )
            .await
            .unwrap();
        assert!(result.is_error);
        assert!(result.content.contains("docs.example.com"));
    }

    #[tokio::test]
    async fn web_browser_never_allow_requires_allowed_domain() {
        let _guard = env_lock();
        set_browser_settings(json!({
            "enabled": true,
            "annotationScreenshots": "Always include",
            "approvalPolicy": "Never allow",
            "blockedDomains": [],
            "allowedDomains": ["trusted.test"]
        }));

        let rejected = WebBrowserTool
            .execute(
                json!({"action":"navigate","url":"https://example.com"}),
                &crate::tool::ToolContext::empty(),
            )
            .await
            .unwrap();
        assert!(rejected.is_error);

        let allowed = WebBrowserTool
            .execute(
                json!({"action":"navigate","url":"https://app.trusted.test"}),
                &crate::tool::ToolContext::empty(),
            )
            .await
            .unwrap();
        assert!(!allowed.is_error);
        assert_eq!(
            allowed.metadata.as_ref().unwrap()["domain_allowed"],
            json!(true)
        );
    }
}
