use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::tool::{Tool, ToolContext, ToolResult, UserQuery};

pub struct AskUserTool;

#[async_trait]
impl Tool for AskUserTool {
    fn name(&self) -> &str {
        "ask_user"
    }

    fn user_facing_name(&self) -> &str {
        "Ask User"
    }

    fn activity_description(&self, params: &Value) -> String {
        let question = params
            .get("question")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        format!("Asking user: {}", question)
    }

    fn description(&self) -> &str {
        "Ask the user a question and wait for their response. Use this when you need clarification or input from the user."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "question": {
                    "type": "string",
                    "description": "The question to ask the user"
                }
            },
            "required": ["question"]
        })
    }

    fn requires_confirmation(&self) -> bool {
        false
    }

    async fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolResult> {
        let question = params
            .get("question")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: question"))?;

        let tx = match &ctx.user_input_tx {
            Some(t) => t,
            None => {
                return Ok(ToolResult::error(
                    "User input channel not available.".to_string(),
                ));
            }
        };

        let rx = match &ctx.user_input_rx {
            Some(r) => r,
            None => {
                return Ok(ToolResult::error(
                    "User input response channel not available.".to_string(),
                ));
            }
        };

        let id = Uuid::new_v4().to_string();

        // Send the question to the TUI
        if let Err(e) = tx.send(UserQuery {
            id: id.clone(),
            question: question.to_string(),
        }) {
            return Ok(ToolResult::error(format!(
                "Failed to send question to user: {}",
                e
            )));
        }

        // Wait for the user's response
        let mut guard = rx.lock().await;
        match guard.recv().await {
            Some(answer) => {
                let metadata = json!({
                    "question": question,
                });
                Ok(ToolResult::success_with_metadata(answer, metadata))
            }
            None => Ok(ToolResult::error(
                "User input channel closed without response.".to_string(),
            )),
        }
    }
}
