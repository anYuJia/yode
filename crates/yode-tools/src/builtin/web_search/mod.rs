use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};

use crate::tool::{Tool, ToolContext, ToolResult};

pub struct WebSearchTool;

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
        "Search the web using the Tavily API. Requires TAVILY_API_KEY environment variable."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Search query"
                },
                "max_results": {
                    "type": "integer",
                    "description": "Maximum number of results to return. Default 5."
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

        let api_key = match std::env::var("TAVILY_API_KEY") {
            Ok(k) => k,
            Err(_) => {
                return Ok(ToolResult::error(
                    "TAVILY_API_KEY environment variable not set. Web search requires a Tavily API key.".to_string(),
                ));
            }
        };

        tracing::debug!(query = %query, max_results = max_results, "Web search");

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| anyhow::anyhow!("Failed to create HTTP client: {}", e))?;

        let request_body = json!({
            "api_key": api_key,
            "query": query,
            "max_results": max_results,
            "search_depth": "basic"
        });

        let response = match client
            .post("https://api.tavily.com/search")
            .json(&request_body)
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                return Ok(ToolResult::error(format!(
                    "Search request failed: {}",
                    e
                )));
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
                let title = item.get("title").and_then(|v| v.as_str()).unwrap_or("(no title)");
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
            Ok(ToolResult::success_with_metadata(format!(
                "No results found for '{}'.",
                query
            ), metadata))
        } else {
            let count = result.get("results").and_then(|v| v.as_array()).map(|a| a.len()).unwrap_or(0);
            let metadata = json!({
                "query": query,
                "result_count": count,
            });
            Ok(ToolResult::success_with_metadata(output, metadata))
        }
    }
}
