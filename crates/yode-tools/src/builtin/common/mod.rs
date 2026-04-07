use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::tool::{Tool, ToolCapabilities, ToolContext, ToolResult};

pub mod config;
pub mod sleep;
pub mod send_file;
pub mod repl;

pub use config::ConfigTool;
pub use sleep::SleepTool;
pub use send_file::SendUserFileTool;
pub use repl::REPLTool;

pub struct SendUserMessageTool;

#[derive(Debug, Serialize, Deserialize)]
struct SendUserMessageParams {
    message: String,
    attachments: Option<Vec<String>>,
    status: MessageStatus,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum MessageStatus {
    Normal,
    Proactive,
}

#[async_trait]
impl Tool for SendUserMessageTool {
    fn name(&self) -> &str {
        "send_user_message"
    }

    fn user_facing_name(&self) -> &str {
        "" 
    }

    fn aliases(&self) -> Vec<String> {
        vec!["brief".to_string(), "Brief".to_string(), "SendUserMessage".to_string()]
    }

    fn activity_description(&self, params: &Value) -> String {
        let msg = params
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let truncated = if msg.len() > 30 {
            format!("{}...", &msg[..30])
        } else {
            msg.to_string()
        };
        format!("Sending message: {}", truncated)
    }

    fn description(&self) -> &str {
        "Send a message to the user - your primary visible output channel."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "message": {
                    "type": "string",
                    "description": "The message for the user. Supports markdown formatting."
                },
                "attachments": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Optional file paths (absolute or relative to cwd) to attach. Use for photos, screenshots, diffs, logs."
                },
                "status": {
                    "type": "string",
                    "enum": ["normal", "proactive"],
                    "description": "Use 'proactive' when surfacing unsolicited updates; 'normal' for direct replies."
                }
            },
            "required": ["message", "status"]
        })
    }

    fn capabilities(&self) -> ToolCapabilities {
        ToolCapabilities {
            requires_confirmation: false,
            supports_auto_execution: true,
            read_only: true,
        }
    }

    async fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolResult> {
        let params: SendUserMessageParams = serde_json::from_value(params)?;
        
        let attachment_count = params.attachments.as_ref().map(|a| a.len()).unwrap_or(0);
        let suffix = if attachment_count > 0 {
            format!(" ({} attachment(s) included)", attachment_count)
        } else {
            "".to_string()
        };

        Ok(ToolResult::success(format!("Message delivered to user.{}", suffix)))
    }
}
