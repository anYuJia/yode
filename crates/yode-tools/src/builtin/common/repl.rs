use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::process::Command;

use crate::tool::{Tool, ToolCapabilities, ToolContext, ToolResult};

pub struct REPLTool;

#[async_trait]
impl Tool for REPLTool {
    fn name(&self) -> &str {
        "repl"
    }

    fn aliases(&self) -> Vec<String> {
        vec!["REPL".to_string()]
    }

    fn user_facing_name(&self) -> &str {
        "REPL"
    }

    fn activity_description(&self, params: &Value) -> String {
        let lang = params
            .get("language")
            .and_then(|v| v.as_str())
            .unwrap_or("code");
        format!("Running {} REPL", lang)
    }

    fn description(&self) -> &str {
        "Run code in an interactive REPL environment. Useful for testing snippets, performing calculations, or prototyping logic."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "language": {
                    "type": "string",
                    "enum": ["python", "nodejs", "bash"],
                    "description": "The language to use"
                },
                "code": {
                    "type": "string",
                    "description": "The code to execute"
                }
            },
            "required": ["language", "code"]
        })
    }

    fn capabilities(&self) -> ToolCapabilities {
        ToolCapabilities {
            requires_confirmation: true,
            supports_auto_execution: false,
            read_only: false,
        }
    }

    async fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolResult> {
        let language = params
            .get("language")
            .and_then(|v| v.as_str())
            .unwrap_or("python");
        let code = params.get("code").and_then(|v| v.as_str()).unwrap_or("");

        let output = match language {
            "python" => Command::new("python3").arg("-c").arg(code).output().await?,
            "nodejs" => Command::new("node").arg("-e").arg(code).output().await?,
            "bash" => Command::new("bash").arg("-c").arg(code).output().await?,
            _ => {
                return Ok(ToolResult::error(format!(
                    "Unsupported language: {}",
                    language
                )))
            }
        };

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        let mut result = stdout.to_string();
        if !stderr.is_empty() {
            if !result.is_empty() {
                result.push_str("\n--- STDERR ---\n");
            }
            result.push_str(&stderr);
        }

        if result.is_empty() && output.status.success() {
            result = "(Execution successful, no output)".to_string();
        }

        Ok(ToolResult::success(result))
    }
}
