use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};

use crate::tool::{Tool, ToolCapabilities, ToolContext, ToolResult};

pub struct ConfigTool;

#[async_trait]
impl Tool for ConfigTool {
    fn name(&self) -> &str {
        "config"
    }

    fn user_facing_name(&self) -> &str {
        "Config"
    }

    fn activity_description(&self, params: &Value) -> String {
        let setting = params.get("setting").and_then(|v| v.as_str()).unwrap_or("setting");
        if params.get("value").is_some() {
            format!("Updating config: {}", setting)
        } else {
            format!("Reading config: {}", setting)
        }
    }

    fn description(&self) -> &str {
        "Get or set Yode configuration settings like theme, model, and tool behaviors."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "setting": {
                    "type": "string",
                    "description": "The setting key (e.g., 'theme', 'model', 'default_mode')"
                },
                "value": {
                    "type": ["string", "boolean", "number", "null"],
                    "description": "The new value. Omit to get current value."
                }
            },
            "required": ["setting"]
        })
    }

    fn capabilities(&self) -> ToolCapabilities {
        ToolCapabilities {
            requires_confirmation: true, // Writing config should be confirmed
            supports_auto_execution: false,
            read_only: false, // Can be read-only if value is None
        }
    }

    async fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolResult> {
        let setting = params
            .get("setting")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: setting"))?;

        let _value = params.get("value");

        // Note: Currently Yode core manages config. 
        // In this implementation, we simulate the get/set logic.
        // Future: integrate with yode_core::config::Config
        
        if let Some(v) = _value {
            Ok(ToolResult::success(format!("Set {} to {}", setting, v)))
        } else {
            // Mock response for now
            Ok(ToolResult::success(format!("{} = \"default\"", setting)))
        }
    }
}
