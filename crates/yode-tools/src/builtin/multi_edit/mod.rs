use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};

use crate::tool::{Tool, ToolContext, ToolErrorType, ToolResult};

pub struct MultiEditTool;

#[async_trait]
impl Tool for MultiEditTool {
    fn name(&self) -> &str {
        "multi_edit"
    }

    fn user_facing_name(&self) -> &str {
        "Multi-Edit"
    }

    fn activity_description(&self, params: &Value) -> String {
        let file = params
            .get("file_path")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let count = params
            .get("edits")
            .and_then(|v| v.as_array())
            .map(|a| a.len())
            .unwrap_or(0);
        format!("Applying {} edits to: {}", count, file)
    }

    fn description(&self) -> &str {
        "Apply multiple edits to a single file in one operation. Each edit replaces an exact string match. All old_strings must be unique in the file."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Absolute path to the file to edit"
                },
                "edits": {
                    "type": "array",
                    "description": "Array of edits to apply",
                    "items": {
                        "type": "object",
                        "properties": {
                            "old_string": {
                                "type": "string",
                                "description": "The exact string to find"
                            },
                            "new_string": {
                                "type": "string",
                                "description": "The replacement string"
                            }
                        },
                        "required": ["old_string", "new_string"]
                    }
                }
            },
            "required": ["file_path", "edits"]
        })
    }

    fn requires_confirmation(&self) -> bool {
        true
    }

    async fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolResult> {
        let file_path = params
            .get("file_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: file_path"))?;

        let edits = params
            .get("edits")
            .and_then(|v| v.as_array())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: edits"))?;

        if edits.is_empty() {
            return Ok(ToolResult::error("No edits provided.".to_string()));
        }

        if let Some(history) = &ctx.read_file_history {
            let h = history.lock().await;
            if !h.contains(&std::path::PathBuf::from(file_path)) {
                return Ok(ToolResult::error_typed(
                    format!(
                        "File '{}' has not been read yet. You must use 'read_file' at least once before applying multi_edit.",
                        file_path
                    ),
                    ToolErrorType::Validation,
                    true,
                    Some(format!("Call read_file(file_path=\"{}\") first.", file_path)),
                ));
            }
        }

        // Read file
        let mut content = match tokio::fs::read_to_string(file_path).await {
            Ok(c) => c,
            Err(e) => {
                return Ok(ToolResult::error(format!(
                    "Failed to read file '{}': {}",
                    file_path, e
                )));
            }
        };

        // Validate all edits first
        for (i, edit) in edits.iter().enumerate() {
            let old_string = edit
                .get("old_string")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Edit {} missing old_string", i))?;

            let new_string = edit
                .get("new_string")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Edit {} missing new_string", i))?;

            if old_string == new_string {
                return Ok(ToolResult::error(format!(
                    "Edit {}: old_string and new_string are identical.",
                    i
                )));
            }

            let count = content.matches(old_string).count();
            if count == 0 {
                return Ok(ToolResult::error(format!(
                    "Edit {}: old_string not found in '{}'.",
                    i, file_path
                )));
            }
            if count > 1 {
                return Ok(ToolResult::error(format!(
                    "Edit {}: old_string found {} times in '{}'. Each old_string must be unique.",
                    i, count, file_path
                )));
            }
        }

        // Apply all edits sequentially
        let mut applied: usize = 0;
        let mut removed_preview = Vec::new();
        let mut added_preview = Vec::new();
        for edit in edits {
            let old_string = edit.get("old_string").unwrap().as_str().unwrap();
            let new_string = edit.get("new_string").unwrap().as_str().unwrap();
            if removed_preview.len() < 5 {
                removed_preview.push(old_string.lines().next().unwrap_or("").to_string());
            }
            if added_preview.len() < 5 {
                added_preview.push(new_string.lines().next().unwrap_or("").to_string());
            }
            content = content.replacen(old_string, new_string, 1);
            applied += 1;
        }

        // Write back
        match tokio::fs::write(file_path, &content).await {
            Ok(()) => {
                let metadata = json!({
                    "file_path": file_path,
                    "applied_edits": applied,
                    "diff_preview": {
                        "removed": removed_preview,
                        "added": added_preview,
                        "more_removed": applied.saturating_sub(removed_preview.len()),
                        "more_added": applied.saturating_sub(added_preview.len()),
                    },
                });
                Ok(ToolResult::success_with_metadata(
                    format!(
                        "Successfully applied {} edit(s) to '{}'.",
                        applied, file_path
                    ),
                    metadata,
                ))
            }
            Err(e) => Ok(ToolResult::error(format!(
                "Failed to write file '{}': {}",
                file_path, e
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::sync::Arc;

    use serde_json::json;
    use tokio::sync::Mutex;

    use crate::tool::{Tool, ToolContext, ToolErrorType};

    use super::MultiEditTool;

    fn temp_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("yode-multi-edit-{}-{}", name, uuid::Uuid::new_v4()))
    }

    #[tokio::test]
    async fn multi_edit_requires_preread() {
        let path = temp_path("preread.txt");
        tokio::fs::write(&path, "alpha\nbeta\n").await.unwrap();

        let history = Arc::new(Mutex::new(HashSet::new()));
        let mut ctx = ToolContext::empty();
        ctx.read_file_history = Some(history);

        let result = MultiEditTool
            .execute(
                json!({
                    "file_path": path.display().to_string(),
                    "edits": [{"old_string":"alpha","new_string":"one"}]
                }),
                &ctx,
            )
            .await
            .unwrap();

        assert!(result.is_error);
        assert_eq!(result.error_type, Some(ToolErrorType::Validation));

        let _ = tokio::fs::remove_file(&path).await;
    }

    #[tokio::test]
    async fn multi_edit_applies_multiple_unique_replacements() {
        let path = temp_path("apply.txt");
        tokio::fs::write(&path, "alpha\nbeta\ngamma\n").await.unwrap();

        let mut seen = HashSet::new();
        seen.insert(path.clone());
        let history = Arc::new(Mutex::new(seen));
        let mut ctx = ToolContext::empty();
        ctx.read_file_history = Some(history);

        let result = MultiEditTool
            .execute(
                json!({
                    "file_path": path.display().to_string(),
                    "edits": [
                        {"old_string":"alpha","new_string":"one"},
                        {"old_string":"beta","new_string":"two"}
                    ]
                }),
                &ctx,
            )
            .await
            .unwrap();

        assert!(!result.is_error);
        let updated = tokio::fs::read_to_string(&path).await.unwrap();
        assert_eq!(updated, "one\ntwo\ngamma\n");
        assert_eq!(result.metadata.as_ref().unwrap()["applied_edits"], json!(2));

        let _ = tokio::fs::remove_file(&path).await;
    }

    #[tokio::test]
    async fn multi_edit_rejects_non_unique_old_string() {
        let path = temp_path("duplicate.txt");
        tokio::fs::write(&path, "alpha\nalpha\n").await.unwrap();

        let mut seen = HashSet::new();
        seen.insert(path.clone());
        let history = Arc::new(Mutex::new(seen));
        let mut ctx = ToolContext::empty();
        ctx.read_file_history = Some(history);

        let result = MultiEditTool
            .execute(
                json!({
                    "file_path": path.display().to_string(),
                    "edits": [{"old_string":"alpha","new_string":"one"}]
                }),
                &ctx,
            )
            .await
            .unwrap();

        assert!(result.is_error);
        assert!(result.content.contains("must be unique"));

        let _ = tokio::fs::remove_file(&path).await;
    }
}
