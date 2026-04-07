use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use std::path::Path;

use crate::tool::{Tool, ToolCapabilities, ToolContext, ToolResult};

pub struct TestRunnerTool;

/// Detected test framework and its run command.
struct DetectedFramework {
    name: &'static str,
    command: String,
    args: Vec<String>,
}

/// Detect the test framework from project files.
fn detect_framework(dir: &Path, filter: Option<&str>) -> Option<DetectedFramework> {
    // Cargo.toml → cargo test
    if dir.join("Cargo.toml").exists() {
        let mut args = vec!["test".to_string()];
        if let Some(f) = filter {
            args.push(f.to_string());
        }
        args.push("--".to_string());
        args.push("--nocapture".to_string());
        return Some(DetectedFramework {
            name: "cargo",
            command: "cargo".to_string(),
            args,
        });
    }

    // package.json → npm test / npx jest / npx vitest
    if dir.join("package.json").exists() {
        if let Ok(content) = std::fs::read_to_string(dir.join("package.json")) {
            if let Ok(pkg) = serde_json::from_str::<Value>(&content) {
                // Check for vitest
                let has_vitest = pkg.get("devDependencies")
                    .and_then(|d| d.get("vitest"))
                    .is_some();
                if has_vitest {
                    let mut args = vec!["vitest".to_string(), "run".to_string()];
                    if let Some(f) = filter {
                        args.push(f.to_string());
                    }
                    return Some(DetectedFramework {
                        name: "vitest",
                        command: "npx".to_string(),
                        args,
                    });
                }

                // Check for jest
                let has_jest = pkg.get("devDependencies")
                    .and_then(|d| d.get("jest"))
                    .is_some();
                if has_jest {
                    let mut args = vec!["jest".to_string()];
                    if let Some(f) = filter {
                        args.push(f.to_string());
                    }
                    return Some(DetectedFramework {
                        name: "jest",
                        command: "npx".to_string(),
                        args,
                    });
                }

                // Fallback: npm test if script exists
                let has_test_script = pkg.get("scripts")
                    .and_then(|s| s.get("test"))
                    .and_then(|v| v.as_str())
                    .map(|s| s != "echo \"Error: no test specified\" && exit 1")
                    .unwrap_or(false);
                if has_test_script {
                    return Some(DetectedFramework {
                        name: "npm",
                        command: "npm".to_string(),
                        args: vec!["test".to_string()],
                    });
                }
            }
        }
    }

    // pytest / python
    if dir.join("pytest.ini").exists()
        || dir.join("pyproject.toml").exists()
        || dir.join("setup.py").exists()
    {
        let mut args = Vec::new();
        if let Some(f) = filter {
            args.push("-k".to_string());
            args.push(f.to_string());
        }
        args.push("-v".to_string());
        return Some(DetectedFramework {
            name: "pytest",
            command: "pytest".to_string(),
            args,
        });
    }

    // go test
    if dir.join("go.mod").exists() {
        let mut args = vec!["test".to_string()];
        if let Some(f) = filter {
            args.push("-run".to_string());
            args.push(f.to_string());
        }
        args.push("./...".to_string());
        args.push("-v".to_string());
        return Some(DetectedFramework {
            name: "go",
            command: "go".to_string(),
            args,
        });
    }

    None
}

/// Parse test results from output to extract pass/fail counts.
fn parse_test_counts(output: &str, framework: &str) -> (u32, u32) {
    let mut passed = 0u32;
    let mut failed = 0u32;

    match framework {
        "cargo" => {
            // "test result: ok. 5 passed; 0 failed;"
            for line in output.lines() {
                if line.starts_with("test result:") {
                    if let Some(p) = extract_number(line, "passed") {
                        passed = p;
                    }
                    if let Some(f) = extract_number(line, "failed") {
                        failed = f;
                    }
                }
            }
        }
        "jest" | "vitest" => {
            // "Tests: 2 passed, 1 failed, 3 total"
            for line in output.lines() {
                let trimmed = line.trim();
                if trimmed.contains("passed") || trimmed.contains("failed") {
                    if let Some(p) = extract_number(trimmed, "passed") {
                        passed = p;
                    }
                    if let Some(f) = extract_number(trimmed, "failed") {
                        failed = f;
                    }
                }
            }
        }
        "pytest" => {
            // "5 passed, 1 failed"
            for line in output.lines() {
                if line.contains("passed") || line.contains("failed") {
                    if let Some(p) = extract_number(line, "passed") {
                        passed = p;
                    }
                    if let Some(f) = extract_number(line, "failed") {
                        failed = f;
                    }
                }
            }
        }
        "go" => {
            // count "ok" and "FAIL" lines
            for line in output.lines() {
                if line.starts_with("ok") {
                    passed += 1;
                } else if line.starts_with("FAIL") {
                    failed += 1;
                }
            }
        }
        _ => {}
    }

    (passed, failed)
}

fn extract_number(line: &str, after_word: &str) -> Option<u32> {
    // Find pattern like "5 passed" or "1 failed"
    let parts: Vec<&str> = line.split_whitespace().collect();
    for (i, part) in parts.iter().enumerate() {
        if part.contains(after_word) && i > 0 {
            return parts[i - 1].trim_matches(|c: char| !c.is_ascii_digit()).parse().ok();
        }
    }
    None
}

#[async_trait]
impl Tool for TestRunnerTool {
    fn name(&self) -> &str {
        "test_runner"
    }

    fn user_facing_name(&self) -> &str {
        "Test Runner"
    }

    fn activity_description(&self, params: &Value) -> String {
        let filter = params.get("filter").and_then(|v| v.as_str());
        match filter {
            Some(f) => format!("Running tests matching: {}", f),
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
        let custom_command = params.get("command").and_then(|v| v.as_str());
        let path = params.get("path").and_then(|v| v.as_str());
        let filter = params.get("filter").and_then(|v| v.as_str());

        let working_dir = if let Some(p) = path {
            Path::new(p).to_path_buf()
        } else {
            ctx.working_dir
                .clone()
                .unwrap_or_else(|| Path::new(".").to_path_buf())
        };

        if !working_dir.exists() {
            return Ok(ToolResult::error(format!(
                "Working directory does not exist: {}",
                working_dir.display()
            )));
        }

        let (command, args, framework_name) = if let Some(cmd) = custom_command {
            // Custom command: split into command + args
            let parts: Vec<&str> = cmd.split_whitespace().collect();
            if parts.is_empty() {
                return Ok(ToolResult::error("Empty command provided".to_string()));
            }
            (
                parts[0].to_string(),
                parts[1..].iter().map(|s| s.to_string()).collect(),
                "custom",
            )
        } else {
            // Auto-detect framework
            match detect_framework(&working_dir, filter) {
                Some(fw) => (fw.command, fw.args, fw.name),
                None => {
                    return Ok(ToolResult::error(
                        "No test framework detected. Looked for: Cargo.toml, package.json, pytest.ini/pyproject.toml, go.mod. Use the 'command' parameter to specify a custom test command.".to_string(),
                    ));
                }
            }
        };

        use tokio::io::AsyncBufReadExt;
        let mut child = tokio::process::Command::new(&command)
            .args(&args)
            .current_dir(&working_dir)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;

        let mut stdout_reader = tokio::io::BufReader::new(child.stdout.take().unwrap()).lines();
        let mut stderr_reader = tokio::io::BufReader::new(child.stderr.take().unwrap()).lines();

        let mut out_stdout = Vec::new();
        let mut out_stderr = Vec::new();
        let progress_tx = ctx.progress_tx.clone();

        loop {
            tokio::select! {
                line = stdout_reader.next_line() => {
                    match line? {
                        Some(l) => {
                            if let Some(ref tx) = progress_tx {
                                let _ = tx.send(crate::tool::ToolProgress {
                                    message: l.clone(),
                                    percent: None,
                                });
                            }
                            out_stdout.push(l);
                        }
                        None => break,
                    }
                }
                line = stderr_reader.next_line() => {
                    match line? {
                        Some(l) => {
                            if let Some(ref tx) = progress_tx {
                                let _ = tx.send(crate::tool::ToolProgress {
                                    message: format!("[stderr] {}", l),
                                    percent: None,
                                });
                            }
                            out_stderr.push(l);
                        }
                        None => break,
                    }
                }
            }
        }

        let output = child.wait().await?;
        let stdout = out_stdout.join("\n");
        let stderr = out_stderr.join("\n");
        let combined = format!("{}{}", stdout, stderr);
        let (passed, failed) = parse_test_counts(&combined, framework_name);

        let status = if output.success() {
            "PASSED"
        } else {
            "FAILED"
        };

        let mut result_str = format!(
            "## Test Results ({})\n\nStatus: **{}** (exit code: {})\n",
            framework_name,
            status,
            output.code().unwrap_or(-1)
        );

        if passed > 0 || failed > 0 {
            result_str.push_str(&format!(
                "Passed: {} | Failed: {}\n",
                passed, failed
            ));
        }

        result_str.push_str(&format!("\n### Output\n```\n{}\n```", combined.trim()));

        Ok(ToolResult::success(result_str))
    }
}
