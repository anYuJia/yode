use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use std::time::Duration;

use crate::tool::{Tool, ToolCapabilities, ToolContext, ToolResult};

pub struct SleepTool;

#[async_trait]
impl Tool for SleepTool {
    fn name(&self) -> &str {
        "sleep"
    }

    fn user_facing_name(&self) -> &str {
        "Sleep" 
    }

    fn activity_description(&self, params: &Value) -> String {
        let ms = params.get("duration_ms").and_then(|v| v.as_u64()).unwrap_or(0);
        format!("Sleeping for {}ms", ms)
    }

    fn description(&self) -> &str {
        "Wait for a specified duration before continuing. Use this to wait for background tasks or external state to sync."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "duration_ms": {
                    "type": "integer",
                    "description": "Duration to sleep in milliseconds (max 60000)."
                }
            },
            "required": ["duration_ms"]
        })
    }

    fn capabilities(&self) -> ToolCapabilities {
        ToolCapabilities {
            requires_confirmation: false,
            supports_auto_execution: true,
            read_only: true, // Sleeping is side-effect free on state
        }
    }

    async fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolResult> {
        let ms = params
            .get("duration_ms")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| anyhow::anyhow!("'duration_ms' is required"))?;

        let duration = Duration::from_millis(ms.min(60000));
        tokio::time::sleep(duration).await;

        Ok(ToolResult::success(format!("Slept for {}ms.", ms)))
    }
}
