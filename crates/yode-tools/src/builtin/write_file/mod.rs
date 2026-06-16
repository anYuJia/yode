use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};

use crate::builtin::edit_artifact::{diff_artifact_metadata, persist_edit_diff_artifact};
use crate::tool::{Tool, ToolCapabilities, ToolContext, ToolResult};

pub struct WriteFileTool;

#[async_trait]
impl Tool for WriteFileTool {
    fn name(&self) -> &str {
        "write_file"
    }

    fn user_facing_name(&self) -> &str {
        "Write File"
    }

    fn activity_description(&self, params: &Value) -> String {
        let file_path = params
            .get("file_path")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        format!("Writing file: {}", file_path)
    }

    fn description(&self) -> &str {
        r#"Writes a file to the local filesystem.

Usage:
- This tool will overwrite the existing file if there is one at the provided path.
- If this is an existing file, you MUST use the `read_file` tool first to read the file's contents. This tool will fail if you did not read the file first.
- Prefer the `edit_file` tool for modifying existing files — it only sends the diff. Only use this tool to create new files or for complete rewrites.
- NEVER create documentation files (*.md) or README files unless explicitly requested by the User.
- Only use emojis if the user explicitly requests it. Avoid writing emojis to files unless asked."#
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Absolute path to write to"
                },
                "content": {
                    "type": "string",
                    "description": "Content to write"
                }
            },
            "required": ["file_path", "content"]
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
        let file_path = params
            .get("file_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: file_path"))?;

        let content = params
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: content"))?;

        let path = std::path::Path::new(file_path);

        // --- Mandatory Pre-read Check for Existing Files ---
        if path.exists() {
            if let Some(history) = &ctx.read_file_history {
                let h = history.lock().await;
                if !history_contains_path(&h, file_path) {
                    return Ok(ToolResult::error_typed(
                        format!("File '{}' exists but has not been read yet. You must use 'read_file' before overwriting an existing file.", file_path),
                        crate::tool::ToolErrorType::Validation,
                        true,
                        Some(format!("Call read_file(file_path=\"{}\") first.", file_path)),
                    ));
                }
            }
        }

        tracing::debug!(file_path = %file_path, "Writing file");

        // Create parent directories if they don't exist
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                tracing::debug!(parent = %parent.display(), "Creating parent directories");
                if let Err(e) = tokio::fs::create_dir_all(parent).await {
                    tracing::warn!(
                        parent = %parent.display(),
                        error = %e,
                        "Failed to create parent directories"
                    );
                    return Ok(ToolResult::error(format!(
                        "Failed to create parent directories for '{}': {}",
                        file_path, e
                    )));
                }
            }
        }

        match tokio::fs::write(file_path, content).await {
            Ok(()) => {
                let byte_count = content.len();
                let line_count = content.lines().count();
                let preview_lines = content
                    .lines()
                    .take(5)
                    .map(|line| line.to_string())
                    .collect::<Vec<_>>();
                let added_lines = content
                    .lines()
                    .map(|line| line.to_string())
                    .collect::<Vec<_>>();
                let artifact = persist_edit_diff_artifact(ctx, file_path, &[], &added_lines).await;
                tracing::debug!(
                    file_path = %file_path,
                    bytes = byte_count,
                    "File written successfully"
                );
                let mut metadata = json!({
                    "file_path": file_path,
                    "byte_count": byte_count,
                    "line_count": line_count,
                    "diff_preview": {
                        "removed": [],
                        "added": preview_lines,
                        "more_removed": 0,
                        "more_added": line_count.saturating_sub(5),
                    },
                    "diff_full": {
                        "removed": [],
                        "added": added_lines,
                    },
                });
                merge_metadata(&mut metadata, diff_artifact_metadata(artifact));
                Ok(ToolResult::success_with_metadata(
                    format!(
                        "Successfully wrote {} bytes ({} lines) to '{}'",
                        byte_count, line_count, file_path
                    ),
                    metadata,
                ))
            }
            Err(e) => {
                tracing::warn!(file_path = %file_path, error = %e, "Failed to write file");
                Ok(ToolResult::error(format!(
                    "Failed to write file '{}': {}",
                    file_path, e
                )))
            }
        }
    }
}

fn merge_metadata(target: &mut Value, extra: Value) {
    if let (Some(target), Some(extra)) = (target.as_object_mut(), extra.as_object()) {
        for (key, value) in extra {
            target.insert(key.clone(), value.clone());
        }
    }
}

fn history_contains_path(
    history: &std::collections::HashSet<std::path::PathBuf>,
    file_path: &str,
) -> bool {
    let target = normalize_history_path(file_path);
    history
        .iter()
        .any(|path| normalize_history_path(path.to_string_lossy().as_ref()) == target)
}

fn normalize_history_path(file_path: &str) -> std::path::PathBuf {
    std::fs::canonicalize(file_path).unwrap_or_else(|_| std::path::PathBuf::from(file_path))
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::sync::Arc;

    use serde_json::json;
    use tokio::sync::Mutex;

    use crate::tool::{Tool, ToolContext, ToolErrorType};

    use super::WriteFileTool;

    fn temp_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("yode-write-file-{}-{}", name, uuid::Uuid::new_v4()))
    }

    #[tokio::test]
    async fn creates_parent_dirs_and_writes_new_file() {
        let dir = temp_path("nested");
        let path = dir.join("a").join("b").join("file.txt");

        let result = WriteFileTool
            .execute(
                json!({
                    "file_path": path.display().to_string(),
                    "content": "hello\nworld\n"
                }),
                &ToolContext::empty(),
            )
            .await
            .unwrap();

        assert!(!result.is_error);
        assert!(path.exists());
        let written = tokio::fs::read_to_string(&path).await.unwrap();
        assert_eq!(written, "hello\nworld\n");
        assert_eq!(result.metadata.as_ref().unwrap()["line_count"], json!(2));
        assert_eq!(
            result.metadata.as_ref().unwrap()["diff_preview"]["added"][0],
            json!("hello")
        );

        let _ = tokio::fs::remove_dir_all(&dir).await;
    }

    #[tokio::test]
    async fn requires_preread_before_overwriting_existing_file() {
        let path = temp_path("existing.txt");
        tokio::fs::write(&path, "old\n").await.unwrap();

        let history = Arc::new(Mutex::new(HashSet::new()));
        let mut ctx = ToolContext::empty();
        ctx.read_file_history = Some(history);

        let result = WriteFileTool
            .execute(
                json!({
                    "file_path": path.display().to_string(),
                    "content": "new\n"
                }),
                &ctx,
            )
            .await
            .unwrap();

        assert!(result.is_error);
        assert_eq!(result.error_type, Some(ToolErrorType::Validation));
        assert!(result.content.contains("must use 'read_file'"));

        let _ = tokio::fs::remove_file(&path).await;
    }

    #[tokio::test]
    async fn allows_overwrite_after_preread() {
        let path = temp_path("overwrite.txt");
        tokio::fs::write(&path, "old\n").await.unwrap();

        let mut seen = HashSet::new();
        seen.insert(path.clone());
        let history = Arc::new(Mutex::new(seen));
        let mut ctx = ToolContext::empty();
        ctx.read_file_history = Some(history);

        let result = WriteFileTool
            .execute(
                json!({
                    "file_path": path.display().to_string(),
                    "content": "new\n"
                }),
                &ctx,
            )
            .await
            .unwrap();

        assert!(!result.is_error);
        let written = tokio::fs::read_to_string(&path).await.unwrap();
        assert_eq!(written, "new\n");

        let _ = tokio::fs::remove_file(&path).await;
    }

    #[tokio::test]
    async fn allows_overwrite_when_history_path_is_canonicalized() {
        let dir = temp_path("canonical");
        tokio::fs::create_dir_all(&dir).await.unwrap();
        let path = dir.join("overwrite.txt");
        tokio::fs::write(&path, "old\n").await.unwrap();

        let mut seen = HashSet::new();
        seen.insert(std::fs::canonicalize(&path).unwrap());
        let history = Arc::new(Mutex::new(seen));
        let mut ctx = ToolContext::empty();
        ctx.read_file_history = Some(history);

        let result = WriteFileTool
            .execute(
                json!({
                    "file_path": path.display().to_string(),
                    "content": "new\n"
                }),
                &ctx,
            )
            .await
            .unwrap();

        assert!(!result.is_error);
        let written = tokio::fs::read_to_string(&path).await.unwrap();
        assert_eq!(written, "new\n");

        let _ = tokio::fs::remove_dir_all(&dir).await;
    }
}
