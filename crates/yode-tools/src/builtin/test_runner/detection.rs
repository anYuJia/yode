use serde_json::Value;
use std::path::Path;

/// Detected test framework and its run command.
pub(super) struct DetectedFramework {
    pub(super) name: &'static str,
    pub(super) command: String,
    pub(super) args: Vec<String>,
}

/// Detect the test framework from project files.
pub(super) fn detect_framework(dir: &Path, filter: Option<&str>) -> Option<DetectedFramework> {
    if dir.join("Cargo.toml").exists() {
        let mut args = vec!["test".to_string()];
        if let Some(filter) = filter {
            args.push(filter.to_string());
        }
        args.push("--".to_string());
        args.push("--nocapture".to_string());
        return Some(DetectedFramework {
            name: "cargo",
            command: "cargo".to_string(),
            args,
        });
    }

    if dir.join("package.json").exists() {
        if let Ok(content) = std::fs::read_to_string(dir.join("package.json")) {
            if let Ok(package) = serde_json::from_str::<Value>(&content) {
                let has_vitest = package
                    .get("devDependencies")
                    .and_then(|deps| deps.get("vitest"))
                    .is_some();
                if has_vitest {
                    let mut args = vec!["vitest".to_string(), "run".to_string()];
                    if let Some(filter) = filter {
                        args.push(filter.to_string());
                    }
                    return Some(DetectedFramework {
                        name: "vitest",
                        command: "npx".to_string(),
                        args,
                    });
                }

                let has_jest = package
                    .get("devDependencies")
                    .and_then(|deps| deps.get("jest"))
                    .is_some();
                if has_jest {
                    let mut args = vec!["jest".to_string()];
                    if let Some(filter) = filter {
                        args.push(filter.to_string());
                    }
                    return Some(DetectedFramework {
                        name: "jest",
                        command: "npx".to_string(),
                        args,
                    });
                }

                let has_test_script = package
                    .get("scripts")
                    .and_then(|scripts| scripts.get("test"))
                    .and_then(|value| value.as_str())
                    .map(|script| script != "echo \"Error: no test specified\" && exit 1")
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

    if dir.join("pytest.ini").exists()
        || dir.join("pyproject.toml").exists()
        || dir.join("setup.py").exists()
    {
        let mut args = Vec::new();
        if let Some(filter) = filter {
            args.push("-k".to_string());
            args.push(filter.to_string());
        }
        args.push("-v".to_string());
        return Some(DetectedFramework {
            name: "pytest",
            command: "pytest".to_string(),
            args,
        });
    }

    if dir.join("go.mod").exists() {
        let mut args = vec!["test".to_string()];
        if let Some(filter) = filter {
            args.push("-run".to_string());
            args.push(filter.to_string());
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
pub(super) fn parse_test_counts(output: &str, framework: &str) -> (u32, u32) {
    let mut passed = 0u32;
    let mut failed = 0u32;

    match framework {
        "cargo" => {
            for line in output.lines() {
                if line.starts_with("test result:") {
                    if let Some(value) = extract_number(line, "passed") {
                        passed = value;
                    }
                    if let Some(value) = extract_number(line, "failed") {
                        failed = value;
                    }
                }
            }
        }
        "jest" | "vitest" => {
            for line in output.lines() {
                let trimmed = line.trim();
                if trimmed.contains("passed") || trimmed.contains("failed") {
                    if let Some(value) = extract_number(trimmed, "passed") {
                        passed = value;
                    }
                    if let Some(value) = extract_number(trimmed, "failed") {
                        failed = value;
                    }
                }
            }
        }
        "pytest" => {
            for line in output.lines() {
                if line.contains("passed") || line.contains("failed") {
                    if let Some(value) = extract_number(line, "passed") {
                        passed = value;
                    }
                    if let Some(value) = extract_number(line, "failed") {
                        failed = value;
                    }
                }
            }
        }
        "go" => {
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
    let parts: Vec<&str> = line.split_whitespace().collect();
    for (index, part) in parts.iter().enumerate() {
        if part.contains(after_word) && index > 0 {
            return parts[index - 1]
                .trim_matches(|c: char| !c.is_ascii_digit())
                .parse()
                .ok();
        }
    }
    None
}
