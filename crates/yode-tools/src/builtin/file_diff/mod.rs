use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::tool::{Tool, ToolCapabilities, ToolContext, ToolErrorType, ToolResult};

pub struct FileDiffTool;

#[async_trait]
impl Tool for FileDiffTool {
    fn name(&self) -> &str {
        "file_diff"
    }

    fn description(&self) -> &str {
        "Compare two files using unified diff format. Shows line-by-line differences between file_a and file_b."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "file_a": {
                    "type": "string",
                    "description": "Path to the first file"
                },
                "file_b": {
                    "type": "string",
                    "description": "Path to the second file"
                },
                "context_lines": {
                    "type": "integer",
                    "description": "Number of context lines around changes (default: 3)",
                    "default": 3
                }
            },
            "required": ["file_a", "file_b"]
        })
    }

    fn capabilities(&self) -> ToolCapabilities {
        ToolCapabilities {
            requires_confirmation: false,
            supports_auto_execution: true,
            read_only: true,
        }
    }

    async fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolResult> {
        let file_a = params
            .get("file_a")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: file_a"))?;
        let file_b = params
            .get("file_b")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: file_b"))?;
        let context_lines = params
            .get("context_lines")
            .and_then(|v| v.as_i64())
            .unwrap_or(3);
        let working_dir = ctx.working_dir.as_deref().unwrap_or_else(|| Path::new("."));
        let file_a_path = match resolve_workspace_file(working_dir, file_a) {
            Ok(path) => path,
            Err(message) => {
                return Ok(ToolResult::error_typed(
                    message,
                    ToolErrorType::Permission,
                    false,
                    Some("Use a path inside the current workspace.".to_string()),
                ));
            }
        };
        let file_b_path = match resolve_workspace_file(working_dir, file_b) {
            Ok(path) => path,
            Err(message) => {
                return Ok(ToolResult::error_typed(
                    message,
                    ToolErrorType::Permission,
                    false,
                    Some("Use a path inside the current workspace.".to_string()),
                ));
            }
        };

        // Validate files exist
        if !file_a_path.exists() {
            return Ok(ToolResult::error_typed(
                format!("File not found: {}", file_a),
                ToolErrorType::NotFound,
                true,
                Some("Check the file path and try again".to_string()),
            ));
        }
        if !file_b_path.exists() {
            return Ok(ToolResult::error_typed(
                format!("File not found: {}", file_b),
                ToolErrorType::NotFound,
                true,
                Some("Check the file path and try again".to_string()),
            ));
        }

        let output = Command::new("diff")
            .args([
                "-u",
                &format!("--label={}", file_a),
                &format!("--label={}", file_b),
                "-U",
                &context_lines.to_string(),
                &file_a_path.display().to_string(),
                &file_b_path.display().to_string(),
            ])
            .current_dir(working_dir)
            .output()
            .map_err(|e| anyhow::anyhow!("Failed to run diff: {}", e))?;

        let stdout = String::from_utf8_lossy(&output.stdout);

        // diff exits with 0 = identical, 1 = different, 2 = error
        match output.status.code() {
            Some(0) => Ok(ToolResult::success("Files are identical.".to_string())),
            Some(1) => Ok(ToolResult::success(stdout.to_string())),
            _ => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                Ok(ToolResult::error(format!("diff failed: {}", stderr.trim())))
            }
        }
    }
}

fn resolve_workspace_file(working_dir: &Path, raw: &str) -> std::result::Result<PathBuf, String> {
    let base = working_dir
        .canonicalize()
        .map_err(|err| format!("Failed to resolve workspace: {}", err))?;
    let candidate = if Path::new(raw).is_absolute() {
        PathBuf::from(raw)
    } else {
        base.join(raw)
    };
    let resolved = candidate
        .canonicalize()
        .map_err(|_| format!("File not found: {}", raw))?;
    if !resolved.starts_with(&base) {
        return Err(format!("Path escapes workspace: {}", raw));
    }
    Ok(resolved)
}

#[cfg(test)]
mod tests {
    use super::{resolve_workspace_file, FileDiffTool};
    use crate::tool::{Tool, ToolContext};
    use serde_json::json;

    #[test]
    fn resolve_workspace_file_rejects_parent_escape() {
        let dir = tempfile::tempdir().unwrap();
        let outside = tempfile::NamedTempFile::new().unwrap();
        let rel = outside.path().display().to_string();
        assert!(resolve_workspace_file(dir.path(), &rel)
            .unwrap_err()
            .contains("escapes workspace"));
    }

    #[tokio::test]
    async fn file_diff_rejects_paths_outside_workspace() {
        let dir = tempfile::tempdir().unwrap();
        let outside = tempfile::NamedTempFile::new().unwrap();
        let inside = dir.path().join("inside.txt");
        std::fs::write(&inside, "a\n").unwrap();
        let mut ctx = ToolContext::empty();
        ctx.working_dir = Some(dir.path().to_path_buf());

        let result = FileDiffTool
            .execute(
                json!({
                    "file_a": inside.display().to_string(),
                    "file_b": outside.path().display().to_string(),
                }),
                &ctx,
            )
            .await
            .unwrap();

        assert!(result.is_error);
        assert!(result.content.contains("escapes workspace"));
    }
}
