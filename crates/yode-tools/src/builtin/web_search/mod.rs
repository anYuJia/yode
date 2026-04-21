use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
#[cfg(test)]
use std::sync::{LazyLock, Mutex};

use crate::tool::{Tool, ToolContext, ToolResult};

pub struct WebSearchTool;

#[cfg(test)]
#[derive(Debug, Default, Clone)]
struct WebSearchTestOverrides {
    use_override: bool,
    url: Option<String>,
    api_key: Option<String>,
}

#[cfg(test)]
static WEB_SEARCH_TEST_OVERRIDES: LazyLock<Mutex<WebSearchTestOverrides>> =
    LazyLock::new(|| Mutex::new(WebSearchTestOverrides::default()));
#[cfg(test)]
static WEB_SEARCH_TEST_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

#[async_trait]
impl Tool for WebSearchTool {
    fn name(&self) -> &str {
        "web_search"
    }

    fn user_facing_name(&self) -> &str {
        "Web Search"
    }

    fn activity_description(&self, params: &Value) -> String {
        let query = params.get("query").and_then(|v| v.as_str()).unwrap_or("");
        format!("Searching web for: {}", query)
    }

    fn description(&self) -> &str {
        "Search the web for current information. Returns titles, URLs, and snippets of matching pages."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "The search query to use"
                },
                "max_results": {
                    "type": "integer",
                    "description": "Maximum number of results to return"
                },
                "allowed_domains": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Only include search results from these domains"
                },
                "blocked_domains": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Never include search results from these domains"
                }
            },
            "required": ["query"]
        })
    }

    fn requires_confirmation(&self) -> bool {
        true
    }

    async fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolResult> {
        let query = params
            .get("query")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: query"))?;

        let max_results = params
            .get("max_results")
            .and_then(|v| v.as_u64())
            .unwrap_or(5);

        let api_key = match tavily_api_key() {
            Ok(k) => k,
            Err(_) => {
                return Ok(ToolResult::error(
                    "TAVILY_API_KEY environment variable not set. Web search requires a Tavily API key.".to_string(),
                ));
            }
        };

        let allowed_domains = params
            .get("allowed_domains")
            .and_then(|v| v.as_array())
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_str().map(str::to_string))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        let blocked_domains = params
            .get("blocked_domains")
            .and_then(|v| v.as_array())
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_str().map(str::to_string))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        tracing::debug!(query = %query, max_results = max_results, "Web search");

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| anyhow::anyhow!("Failed to create HTTP client: {}", e))?;

        let request_body = json!({
            "api_key": api_key,
            "query": query,
            "max_results": max_results,
            "search_depth": "basic",
            "include_domains": allowed_domains,
            "exclude_domains": blocked_domains
        });

        let response = match client
            .post(tavily_search_url())
            .json(&request_body)
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                return Ok(ToolResult::error(format!("Search request failed: {}", e)));
            }
        };

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Ok(ToolResult::error(format!(
                "Tavily API error (HTTP {}): {}",
                status, body
            )));
        }

        let result: Value = match response.json().await {
            Ok(v) => v,
            Err(e) => {
                return Ok(ToolResult::error(format!(
                    "Failed to parse search response: {}",
                    e
                )));
            }
        };

        // Format results
        let mut output = String::new();
        if let Some(results) = result.get("results").and_then(|v| v.as_array()) {
            for (i, item) in results.iter().enumerate() {
                let title = item
                    .get("title")
                    .and_then(|v| v.as_str())
                    .unwrap_or("(no title)");
                let url = item.get("url").and_then(|v| v.as_str()).unwrap_or("");
                let content = item.get("content").and_then(|v| v.as_str()).unwrap_or("");

                output.push_str(&format!("{}. {}\n", i + 1, title));
                output.push_str(&format!("   URL: {}\n", url));
                if !content.is_empty() {
                    output.push_str(&format!("   {}\n", content));
                }
                output.push('\n');
            }
        }

        if output.is_empty() {
            let metadata = json!({
                "query": query,
                "result_count": 0,
            });
            Ok(ToolResult::success_with_metadata(
                format!("No results found for '{}'.", query),
                metadata,
            ))
        } else {
            let count = result
                .get("results")
                .and_then(|v| v.as_array())
                .map(|a| a.len())
                .unwrap_or(0);
            let metadata = json!({
                "query": query,
                "result_count": count,
            });
            Ok(ToolResult::success_with_metadata(output, metadata))
        }
    }
}

fn tavily_api_key() -> std::result::Result<String, std::env::VarError> {
    #[cfg(test)]
    {
        if let Ok(overrides) = WEB_SEARCH_TEST_OVERRIDES.lock() {
            if overrides.use_override {
                return overrides
                    .api_key
                    .clone()
                    .ok_or(std::env::VarError::NotPresent);
            }
        }
    }
    std::env::var("TAVILY_API_KEY")
}

fn tavily_search_url() -> String {
    #[cfg(test)]
    {
        if let Ok(overrides) = WEB_SEARCH_TEST_OVERRIDES.lock() {
            if overrides.use_override {
                if let Some(url) = &overrides.url {
                    return url.clone();
                }
            }
        }
    }
    "https://api.tavily.com/search".to_string()
}

#[cfg(test)]
fn set_web_search_test_overrides(url: Option<String>, api_key: Option<String>) {
    *WEB_SEARCH_TEST_OVERRIDES.lock().unwrap() = WebSearchTestOverrides {
        use_override: true,
        url,
        api_key,
    };
}

#[cfg(test)]
fn clear_web_search_test_overrides() {
    *WEB_SEARCH_TEST_OVERRIDES.lock().unwrap() = WebSearchTestOverrides::default();
}

#[cfg(test)]
mod tests {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    use tokio::sync::oneshot;

    use crate::tool::Tool;

    use super::{
        clear_web_search_test_overrides, set_web_search_test_overrides, WebSearchTool,
        WEB_SEARCH_TEST_LOCK,
    };

    async fn spawn_tavily_server(
        status: &str,
        body: &str,
    ) -> (String, oneshot::Receiver<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let status = status.to_string();
        let body = body.to_string();
        let (tx, rx) = oneshot::channel();

        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 4096];
            let n = socket.read(&mut buf).await.unwrap();
            let request = String::from_utf8_lossy(&buf[..n]).to_string();
            let captured = request
                .split("\r\n\r\n")
                .nth(1)
                .unwrap_or("")
                .to_string();
            let _ = tx.send(captured);
            let response = format!(
                "HTTP/1.1 {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                status,
                body.len(),
                body
            );
            let _ = socket.write_all(response.as_bytes()).await;
        });

        (format!("http://{}", addr), rx)
    }

    #[tokio::test]
    async fn returns_error_when_api_key_is_missing() {
        let _guard = WEB_SEARCH_TEST_LOCK.lock().unwrap();
        set_web_search_test_overrides(None, None);

        let result = WebSearchTool
            .execute(
                serde_json::json!({
                    "query": "rust"
                }),
                &crate::tool::ToolContext::empty(),
            )
            .await
            .unwrap();

        assert!(result.is_error);
        assert!(result.content.contains("TAVILY_API_KEY"));
        clear_web_search_test_overrides();
    }

    #[tokio::test]
    async fn sends_domain_filters_and_formats_results() {
        let _guard = WEB_SEARCH_TEST_LOCK.lock().unwrap();
        let (url, body_rx) = spawn_tavily_server(
            "200 OK",
            r#"{"results":[{"title":"Rust","url":"https://www.rust-lang.org","content":"systems language"}]}"#,
        )
        .await;

        set_web_search_test_overrides(Some(url), Some("test-key".to_string()));

        let result = WebSearchTool
            .execute(
                serde_json::json!({
                    "query": "rust",
                    "max_results": 3,
                    "allowed_domains": ["rust-lang.org"],
                    "blocked_domains": ["example.com"]
                }),
                &crate::tool::ToolContext::empty(),
            )
            .await
            .unwrap();

        let request_body = body_rx.await.unwrap();
        assert!(request_body.contains("\"include_domains\":[\"rust-lang.org\"]"));
        assert!(request_body.contains("\"exclude_domains\":[\"example.com\"]"));
        assert!(request_body.contains("\"max_results\":3"));

        assert!(!result.is_error);
        assert!(result.content.contains("Rust"));
        assert!(result.content.contains("https://www.rust-lang.org"));
        assert_eq!(
            result.metadata.as_ref().unwrap()["result_count"],
            serde_json::json!(1)
        );

        clear_web_search_test_overrides();
    }
}
