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

#[cfg(test)]
mod tests {
    use super::detect_framework;

    #[test]
    fn detects_frameworks_by_ecosystem() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "[package]\nname = \"x\"\n").unwrap();
        assert_eq!(detect_framework(dir.path(), None).unwrap().name, "cargo");

        let node = tempfile::tempdir().unwrap();
        std::fs::write(
            node.path().join("package.json"),
            r#"{"devDependencies":{"vitest":"1.0.0"}}"#,
        )
        .unwrap();
        assert_eq!(detect_framework(node.path(), None).unwrap().name, "vitest");

        let py = tempfile::tempdir().unwrap();
        std::fs::write(py.path().join("pytest.ini"), "[pytest]").unwrap();
        assert_eq!(detect_framework(py.path(), None).unwrap().name, "pytest");

        let go = tempfile::tempdir().unwrap();
        std::fs::write(go.path().join("go.mod"), "module example.com/x").unwrap();
        assert_eq!(detect_framework(go.path(), None).unwrap().name, "go");
    }
}
