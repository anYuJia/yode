use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};

use crate::tool::{Tool, ToolContext, ToolResult};

pub mod browser;
pub use browser::WebBrowserTool;

pub struct WebFetchTool;

#[async_trait]
impl Tool for WebFetchTool {
    fn name(&self) -> &str {
        "web_fetch"
    }

    fn user_facing_name(&self) -> &str {
        "Web Fetch"
    }

    fn activity_description(&self, params: &Value) -> String {
        let url = params.get("url").and_then(|v| v.as_str()).unwrap_or("");
        format!("Fetching: {}", url)
    }

    fn description(&self) -> &str {
        r#"Fetch content from a URL.
        
IMPORTANT: WebFetch WILL FAIL for authenticated or private URLs. Before using this tool, check if the URL points to an authenticated service (e.g. Google Docs, Confluence, Jira, GitHub). If so, look for a specialized MCP tool that provides authenticated access."#
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "The URL to fetch content from"
                },
                "prompt": {
                    "type": "string",
                    "description": "The prompt to run on the fetched content"
                }
            },
            "required": ["url", "prompt"]
        })
    }

    fn requires_confirmation(&self) -> bool {
        true
    }

    async fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolResult> {
        let url = params
            .get("url")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: url"))?;

        let _prompt = params.get("prompt").and_then(|v| v.as_str()).unwrap_or("");

        tracing::debug!(url = %url, "Fetching URL");

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .user_agent("yode/0.1")
            .build()
            .map_err(|e| anyhow::anyhow!("Failed to create HTTP client: {}", e))?;

        let response = match client.get(url).send().await {
            Ok(r) => r,
            Err(e) => {
                return Ok(ToolResult::error(format!(
                    "Failed to fetch '{}': {}",
                    url, e
                )));
            }
        };

        let status = response.status();
        if !status.is_success() {
            return Ok(ToolResult::error(format!("HTTP {} for '{}'", status, url)));
        }

        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();

        let body = match response.text().await {
            Ok(t) => t,
            Err(e) => {
                return Ok(ToolResult::error(format!(
                    "Failed to read response body: {}",
                    e
                )));
            }
        };

        // Convert HTML to text
        let text = if content_type.contains("text/html") {
            match html2text::from_read(body.as_bytes(), 80) {
                Ok(t) => t,
                Err(_) => body,
            }
        } else {
            body
        };

        // Truncate if needed
        let metadata = json!({
            "url": url,
            "content_type": content_type,
            "original_length": text.len(),
        });

        Ok(ToolResult::success_with_metadata(text, metadata))
    }
}
