mod detection;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use std::path::Path;

use crate::tool::{Tool, ToolCapabilities, ToolContext, ToolResult};

use self::detection::{detect_framework, parse_test_counts};

pub struct TestRunnerTool;

#[async_trait]
impl Tool for TestRunnerTool {
    fn name(&self) -> &str {
        "test_runner"
    }

    fn user_facing_name(&self) -> &str {
        "Test Runner"
    }

    fn activity_description(&self, params: &Value) -> String {
        let filter = params.get("filter").and_then(|value| value.as_str());
        match filter {
            Some(filter) => format!("Running tests matching: {}", filter),
            None => "Running all tests".to_string(),
        }
    }

    fn description(&self) -> &str {
        "Run tests with automatic framework detection. Detects Cargo, npm/Jest/Vitest, pytest, and Go test. Parses pass/fail counts from output."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "Override test command (skips auto-detection). e.g. \"cargo test -- --test-threads=1\""
                },
                "path": {
                    "type": "string",
                    "description": "Working directory for running tests (defaults to project root)"
                },
                "filter": {
                    "type": "string",
                    "description": "Test name filter/pattern to run a subset of tests"
                }
            }
        })
    }

    fn capabilities(&self) -> ToolCapabilities {
        ToolCapabilities {
            requires_confirmation: true,
            supports_auto_execution: false,
            read_only: false,
        }
    }

    async fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolResult> {
        let custom_command = params.get("command").and_then(|value| value.as_str());
        let path = params.get("path").and_then(|value| value.as_str());
        let filter = params.get("filter").and_then(|value| value.as_str());

        let working_dir = if let Some(path) = path {
            Path::new(path).to_path_buf()
        } else {
            ctx.working_dir
                .clone()
                .unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
        };

        let (framework_name, command, args) = if let Some(custom_command) = custom_command {
            let parts: Vec<&str> = custom_command.split_whitespace().collect();
            if parts.is_empty() {
                return Ok(ToolResult::error("Empty command".to_string()));
            }
            let command = parts[0].to_string();
            let args = parts[1..].iter().map(|part| part.to_string()).collect();
            ("custom", command, args)
        } else {
            match detect_framework(&working_dir, filter) {
                Some(framework) => (framework.name, framework.command, framework.args),
                None => {
                    return Ok(ToolResult::error(
                        "Could not detect test framework. Provide explicit command.".to_string(),
                    ));
                }
            }
        };

        let output = tokio::process::Command::new(&command)
            .args(&args)
            .current_dir(&working_dir)
            .output()
            .await?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let combined = if stderr.is_empty() {
            stdout.clone()
        } else if stdout.is_empty() {
            stderr.clone()
        } else {
            format!("{}\n{}", stdout, stderr)
        };

        let (passed, failed) = parse_test_counts(&combined, framework_name);
        let success = output.status.success();

        Ok(ToolResult::success_with_metadata(
            format!(
                "Framework: {}\nCommand: {} {}\nExit: {}\nPassed: {}\nFailed: {}\n\n{}",
                framework_name,
                command,
                args.join(" "),
                output.status.code().unwrap_or(-1),
                passed,
                failed,
                combined
            ),
            json!({
                "framework": framework_name,
                "command": command,
                "args": args,
                "passed": passed,
                "failed": failed,
                "success": success,
                "working_dir": working_dir,
            }),
        ))
    }
}
