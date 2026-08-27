mod cdp;

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::tool::{Tool, ToolCapabilities, ToolContext, ToolErrorType, ToolResult};

pub use cdp::shutdown_browser_runtime;

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
        "Interact with a real Chromium browser over Chrome DevTools Protocol (CDP): navigate, click, type, scroll, evaluate JavaScript, and capture screenshot artifacts. Use this to verify web UI behavior instead of assuming frontend changes work."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["navigate", "click", "type", "scroll", "screenshot", "evaluate"],
                    "description": "Browser action to perform against the persistent desktop browser session"
                },
                "url": {
                    "type": "string",
                    "description": "http:// or https:// URL for navigate"
                },
                "selector": {
                    "type": "string",
                    "description": "CSS selector for click/type"
                },
                "text": {
                    "type": "string",
                    "description": "Text for type"
                },
                "code": {
                    "type": "string",
                    "description": "JavaScript expression for evaluate"
                },
                "delta_y": {
                    "type": "integer",
                    "default": 600,
                    "description": "Vertical pixels for scroll; negative scrolls upward"
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

    async fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolResult> {
        let action = match params.get("action").and_then(Value::as_str) {
            Some(
                action @ ("navigate" | "click" | "type" | "scroll" | "screenshot" | "evaluate"),
            ) => action,
            Some(other) => {
                return Ok(validation_error(
                    format!("Unknown browser action '{other}'"),
                    "Use navigate, click, type, scroll, screenshot, or evaluate.",
                ))
            }
            None => {
                return Ok(validation_error(
                    "Browser action is required.".to_string(),
                    "Provide the action field.",
                ))
            }
        };

        let settings = browser_runtime_settings();
        if !settings.enabled {
            return Ok(ToolResult::error_typed(
                "浏览器功能已在设置中关闭。".to_string(),
                ToolErrorType::PermissionDeny,
                true,
                Some("请在 设置 > 浏览器 中开启浏览器功能后重试。".to_string()),
            ));
        }

        if let Some(error) = validate_action_inputs(action, &params) {
            return Ok(error);
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
                    Some(
                        "请将该域名加入 设置 > 浏览器 > 已允许域名，或调整授权审批策略。"
                            .to_string(),
                    ),
                ));
            }
        } else if action == "navigate" {
            return Ok(validation_error(
                "navigate 操作需要有效的 http:// 或 https:// URL。".to_string(),
                "Provide a URL with a valid domain.",
            ));
        }

        let workspace = ctx
            .working_dir
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("Working directory not set for browser artifacts"))?;
        match cdp::execute_browser_action(action, &params, workspace).await {
            Ok(mut execution) => {
                if let Some(metadata) = execution.metadata.as_object_mut() {
                    metadata.insert("browser_enabled".to_string(), json!(settings.enabled));
                    metadata.insert(
                        "approval_policy".to_string(),
                        json!(settings.approval_policy),
                    );
                    metadata.insert(
                        "annotation_screenshots".to_string(),
                        json!(settings.annotation_screenshots),
                    );
                    metadata.insert("requested_domain".to_string(), json!(domain));
                }
                Ok(ToolResult::success_with_metadata(
                    execution.message,
                    execution.metadata,
                ))
            }
            Err(error) => Ok(ToolResult::error_typed(
                format!("Real browser execution failed: {error:#}"),
                ToolErrorType::Execution,
                true,
                Some(
                    "Ensure Chrome, Chromium, or Edge is installed. You may set YODE_BROWSER_EXECUTABLE to an explicit browser path, then retry."
                        .to_string(),
                ),
            )),
        }
    }
}

fn validate_action_inputs(action: &str, params: &Value) -> Option<ToolResult> {
    let non_empty = |key: &str| {
        params
            .get(key)
            .and_then(Value::as_str)
            .is_some_and(|value| !value.trim().is_empty())
    };
    let missing = match action {
        "navigate" if !non_empty("url") => Some("url"),
        "click" if !non_empty("selector") => Some("selector"),
        "type" if !non_empty("selector") => Some("selector"),
        "type" if !non_empty("text") => Some("text"),
        "evaluate" if !non_empty("code") => Some("code"),
        _ => None,
    };
    missing.map(|key| {
        validation_error(
            format!("Browser action '{action}' requires non-empty '{key}'."),
            "Provide the missing browser action parameter.",
        )
    })
}

fn validation_error(message: String, suggestion: &str) -> ToolResult {
    ToolResult::error_typed(
        message,
        ToolErrorType::Validation,
        true,
        Some(suggestion.to_string()),
    )
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
    use std::sync::OnceLock;

    use serde_json::{json, Value};

    use crate::tool::Tool;

    use super::WebBrowserTool;

    async fn env_lock() -> tokio::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
            .lock()
            .await
    }

    fn set_browser_settings(settings: Value) {
        std::env::set_var("YODE_BROWSER_SETTINGS", settings.to_string());
    }

    #[test]
    fn web_browser_requires_confirmation_for_external_actions() {
        let caps = WebBrowserTool.capabilities();
        assert!(caps.requires_confirmation);
        assert!(!caps.supports_auto_execution);
        assert!(!caps.read_only);
    }

    #[tokio::test]
    async fn web_browser_rejects_when_disabled_without_starting_runtime() {
        let _guard = env_lock().await;
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
    async fn web_browser_rejects_blocked_domain_before_runtime() {
        let _guard = env_lock().await;
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
    async fn action_validation_happens_before_runtime_launch() {
        let _guard = env_lock().await;
        std::env::remove_var("YODE_BROWSER_SETTINGS");
        let result = WebBrowserTool
            .execute(
                json!({"action":"click","selector":""}),
                &crate::tool::ToolContext::empty(),
            )
            .await
            .unwrap();
        assert!(result.is_error);
        assert_eq!(
            result.error_type,
            Some(crate::tool::ToolErrorType::Validation)
        );
    }
}
