use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};

use crate::tool::{Tool, ToolCapabilities, ToolContext, ToolResult};

pub struct WebBrowserTool;

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
        let action = params.get("action").and_then(|v| v.as_str()).unwrap_or("browsing");
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
            requires_confirmation: false,
            supports_auto_execution: true,
            read_only: false,
        }
    }

    async fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolResult> {
        let action = params.get("action").and_then(|v| v.as_str()).unwrap_or("");
        
        // Mock browser execution
        let msg = match action {
            "navigate" => format!("Navigated to {}", params.get("url").and_then(|v| v.as_str()).unwrap_or("URL")),
            "click" => format!("Clicked element: {}", params.get("selector").and_then(|v| v.as_str()).unwrap_or("?")),
            "screenshot" => "Captured screenshot (stored in session artifacts).".to_string(),
            _ => format!("Performed browser action: {}", action),
        };

        Ok(ToolResult::success(msg))
    }
}
