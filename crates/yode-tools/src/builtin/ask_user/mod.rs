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
        let first_q = params
            .get("questions")
            .and_then(|v| v.as_array())
            .and_then(|a| a.first())
            .and_then(|q| q.get("question"))
            .and_then(|v| v.as_str())
            .unwrap_or("questions");
        format!("Asking user: {}", first_q)
    }

    fn description(&self) -> &str {
        "Ask the user one or more multiple-choice questions and wait for their response. \
         Use this when you need clarification, choice between approaches, or specific input."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "questions": {
                    "type": "array",
                    "minItems": 1,
                    "maxItems": 4,
                    "items": {
                        "type": "object",
                        "properties": {
                            "question": {
                                "type": "string",
                                "description": "The complete question to ask the user. Should end with a question mark."
                            },
                            "header": {
                                "type": "string",
                                "description": "Very short label displayed as a chip/tag. Examples: 'Auth method', 'Approach'."
                            },
                            "options": {
                                "type": "array",
                                "minItems": 2,
                                "maxItems": 4,
                                "items": {
                                    "type": "object",
                                    "properties": {
                                        "label": {
                                            "type": "string",
                                            "description": "Short display text for this option (1-5 words)."
                                        },
                                        "description": {
                                            "type": "string",
                                            "description": "Explanation of trade-offs or implications of this choice."
                                        },
                                        "preview": {
                                            "type": "string",
                                            "description": "Optional code snippet or visual preview for this option."
                                        }
                                    },
                                    "required": ["label", "description"]
                                }
                            },
                            "multiSelect": {
                                "type": "boolean",
                                "default": false,
                                "description": "Whether the user can select multiple options."
                            }
                        },
                        "required": ["question", "header", "options"]
                    }
                }
            },
            "required": ["questions"]
        })
    }

    fn capabilities(&self) -> crate::tool::ToolCapabilities {
        crate::tool::ToolCapabilities {
            requires_confirmation: false,
            supports_auto_execution: false,
            read_only: true,
        }
    }

    async fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolResult> {
        let questions_val = params
            .get("questions")
            .and_then(|v| v.as_array())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: questions"))?;

        let mut questions = Vec::new();
        for q_val in questions_val {
            let question = q_val.get("question").and_then(|v| v.as_str()).unwrap_or_default().to_string();
            let header = q_val.get("header").and_then(|v| v.as_str()).unwrap_or_default().to_string();
            let multi_select = q_val.get("multiSelect").and_then(|v| v.as_bool()).unwrap_or(false);
            
            let mut options = Vec::new();
            if let Some(opts_val) = q_val.get("options").and_then(|v| v.as_array()) {
                for opt_val in opts_val {
                    options.push(crate::tool::UserQueryOption {
                        label: opt_val.get("label").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
                        description: opt_val.get("description").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
                        preview: opt_val.get("preview").and_then(|v| v.as_str()).map(|s| s.to_string()),
                    });
                }
            }
            
            questions.push(crate::tool::UserQuestion {
                question,
                header,
                options,
                multi_select,
            });
        }

        let tx = match &ctx.user_input_tx {
            Some(t) => t,
            None => return Ok(ToolResult::error("User input channel not available.".to_string())),
        };

        let rx = match &ctx.user_input_rx {
            Some(r) => r,
            None => return Ok(ToolResult::error("User input response channel not available.".to_string())),
        };

        let id = Uuid::new_v4().to_string();

        // Send structured query to TUI
        if let Err(e) = tx.send(UserQuery {
            id: id.clone(),
            questions: questions.clone(),
        }) {
            return Ok(ToolResult::error(format!("Failed to send query to user: {}", e)));
        }

        // Wait for response (TUI will send a JSON string mapping question text to answer)
        let mut guard = rx.lock().await;
        match guard.recv().await {
            Some(answer_json) => {
                Ok(ToolResult::success_with_metadata(
                    format!("User answered: {}", answer_json),
                    json!({ "raw_answers": answer_json })
                ))
            }
            None => Ok(ToolResult::error("User input channel closed.".to_string())),
        }
    }
}
