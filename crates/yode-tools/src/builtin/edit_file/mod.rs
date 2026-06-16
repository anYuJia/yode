use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};

use crate::builtin::edit_artifact::{diff_artifact_metadata, persist_edit_diff_artifact};
use crate::tool::{Tool, ToolCapabilities, ToolContext, ToolResult};

pub mod snip;
pub use snip::SnipTool;

pub struct EditFileTool;

#[async_trait]
impl Tool for EditFileTool {
    fn name(&self) -> &str {
        "edit_file"
    }

    fn user_facing_name(&self) -> &str {
        "Edit File"
    }

    fn activity_description(&self, params: &Value) -> String {
        let file_path = params
            .get("file_path")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        format!("Editing file: {}", file_path)
    }

    fn description(&self) -> &str {
        r#"Performs exact string replacements in files.

Usage:
- When editing text from Read tool output, ensure you preserve the exact indentation (tabs/spaces) as it appears AFTER the line number prefix. Everything after that is the actual file content to match. Never include any part of the line number prefix in the old_string or new_string.
- ALWAYS prefer editing existing files in the codebase. NEVER write new files unless explicitly required.
- The edit will FAIL if `old_string` is not unique in the file. Either provide a larger string with more surrounding context to make it unique or use `replace_all` to change every instance of `old_string`.
- Use `replace_all` for replacing and renaming strings across the file. This parameter is useful if you want to rename a variable for instance."#
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Absolute path to the file to edit"
                },
                "old_string": {
                    "type": "string",
                    "description": "The exact literal string to be replaced"
                },
                "new_string": {
                    "type": "string",
                    "description": "The new literal string to replace it with"
                },
                "replace_all": {
                    "type": "boolean",
                    "default": false,
                    "description": "If true, replace all occurrences of old_string. If false (default), only one occurrence is replaced."
                }
            },
            "required": ["file_path", "old_string", "new_string"]
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

        let old_string = params
            .get("old_string")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: old_string"))?;

        let new_string = params
            .get("new_string")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: new_string"))?;

        let replace_all = params
            .get("replace_all")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        tracing::debug!(
            file_path = %file_path,
            old_len = old_string.len(),
            new_len = new_string.len(),
            replace_all = replace_all,
            "Editing file"
        );

        // Read file
        let content = match tokio::fs::read_to_string(file_path).await {
            Ok(c) => c,
            Err(e) => {
                return Ok(ToolResult::error(format!(
                    "Failed to read file '{}': {}",
                    file_path, e
                )));
            }
        };

        if old_string == new_string {
            return Ok(ToolResult::error(
                "old_string and new_string are identical. No changes to make.".to_string(),
            ));
        }

        let actual_old = match locate_edit_target(&content, old_string) {
            Some(value) => value,
            None => {
                return Ok(ToolResult::error(format!(
                    "The exact string to replace was not found in '{}'. \
                     Ensure you provided the EXACT text including indentation and quotes as seen in 'read_file' output. \
                     Fallbacks: re-read the file, use snip on a narrower line range, or include more surrounding context in old_string.",
                    file_path
                )));
            }
        };

        // Count occurrences
        let count = content.matches(&actual_old).count();

        if count > 1 && !replace_all {
            return Ok(ToolResult::error(format!(
                "Found {} matches of the string to replace in '{}', but replace_all is false. \
                 Please provide more context to uniquely identify the instance or set replace_all=true. \
                 Fallbacks: extend old_string with neighboring lines, narrow the target with snip, or intentionally switch to replace_all=true for broad renames.",
                count, file_path
            )));
        }

        // Perform replacement
        let new_content = if replace_all {
            content.replace(&actual_old, new_string)
        } else {
            content.replacen(&actual_old, new_string, 1)
        };

        // Write back
        match tokio::fs::write(file_path, &new_content).await {
            Ok(()) => {
                // --- LSP Notification ---
                if let Some(_lsp) = &ctx.lsp_manager {
                    // let mut lsp_guard = lsp.lock().await;
                    // let _ = lsp_guard.notify_file_change(file_path, &new_content).await;
                }

                let replacements = if replace_all { count } else { 1 };
                let removed = actual_old
                    .lines()
                    .take(5)
                    .map(|line| line.to_string())
                    .collect::<Vec<_>>();
                let added = new_string
                    .lines()
                    .take(5)
                    .map(|line| line.to_string())
                    .collect::<Vec<_>>();
                let old_lines = actual_old
                    .lines()
                    .map(|line| line.to_string())
                    .collect::<Vec<_>>();
                let new_lines = new_string
                    .lines()
                    .map(|line| line.to_string())
                    .collect::<Vec<_>>();
                let mut full_removed = Vec::new();
                let mut full_added = Vec::new();
                for _ in 0..replacements {
                    full_removed.extend(old_lines.iter().cloned());
                    full_added.extend(new_lines.iter().cloned());
                }
                let artifact =
                    persist_edit_diff_artifact(ctx, file_path, &full_removed, &full_added).await;
                let mut metadata = json!({
                    "file_path": file_path,
                    "replacements": replacements,
                    "diff_preview": {
                        "removed": removed,
                        "added": added,
                        "more_removed": full_removed.len().saturating_sub(5),
                        "more_added": full_added.len().saturating_sub(5),
                    },
                    "diff_full": {
                        "removed": full_removed,
                        "added": full_added,
                    },
                });
                merge_metadata(&mut metadata, diff_artifact_metadata(artifact));

                Ok(ToolResult::success_with_metadata(
                    format!("The file {} has been updated successfully.", file_path),
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

fn merge_metadata(target: &mut Value, extra: Value) {
    if let (Some(target), Some(extra)) = (target.as_object_mut(), extra.as_object()) {
        for (key, value) in extra {
            target.insert(key.clone(), value.clone());
        }
    }
}

fn locate_edit_target(content: &str, old_string: &str) -> Option<String> {
    if old_string.is_empty() {
        return None;
    }

    if content.contains(old_string) {
        return Some(old_string.to_string());
    }

    relaxed_line_match(content, old_string)
}

fn relaxed_line_match(content: &str, needle: &str) -> Option<String> {
    let content_lines: Vec<&str> = content.split_inclusive('\n').collect();
    let needle_lines: Vec<&str> = needle.split_inclusive('\n').collect();
    if content_lines.is_empty()
        || needle_lines.is_empty()
        || needle_lines.len() > content_lines.len()
    {
        return None;
    }

    let normalized_needle: Vec<String> = needle_lines
        .iter()
        .map(|line| normalize_edit_line(line))
        .collect();
    let mut matches = Vec::new();

    for start in 0..=content_lines.len() - needle_lines.len() {
        let mut matched = true;
        for (offset, needle_line) in normalized_needle.iter().enumerate() {
            if normalize_edit_line(content_lines[start + offset]) != *needle_line {
                matched = false;
                break;
            }
        }

        if matched {
            let candidate = content_lines[start..start + needle_lines.len()].concat();
            matches.push(candidate);
            if matches.len() > 1 {
                return None;
            }
        }
    }

    matches.into_iter().next()
}

fn normalize_edit_line(line: &str) -> String {
    line.trim_end_matches(['\n', '\r', ' ', '\t']).to_string()
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::sync::Arc;

    use serde_json::json;
    use tokio::sync::Mutex;

    use crate::tool::{Tool, ToolContext};

    use super::EditFileTool;

    fn temp_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("yode-edit-file-{}-{}", name, uuid::Uuid::new_v4()))
    }

    #[tokio::test]
    async fn edits_existing_files_without_preread_when_old_string_matches() {
        let path = temp_path("no-preread.txt");
        tokio::fs::write(&path, "let value = 1;\n").await.unwrap();

        let result = EditFileTool
            .execute(
                json!({
                    "file_path": path.display().to_string(),
                    "old_string": "value",
                    "new_string": "answer"
                }),
                &ToolContext::empty(),
            )
            .await
            .unwrap();

        assert!(!result.is_error);
        let updated = tokio::fs::read_to_string(&path).await.unwrap();
        assert_eq!(updated, "let answer = 1;\n");

        let _ = tokio::fs::remove_file(&path).await;
    }

    #[tokio::test]
    async fn replace_all_updates_file_and_metadata() {
        let dir = temp_path("replace-all-dir");
        let path = dir.join("replace-all.txt");
        tokio::fs::create_dir_all(&dir).await.unwrap();
        tokio::fs::write(&path, "foo = 1\nfoo = 2\n").await.unwrap();

        let mut seen = HashSet::new();
        seen.insert(path.clone());
        let history = Arc::new(Mutex::new(seen));
        let mut ctx = ToolContext::empty();
        ctx.read_file_history = Some(history);
        ctx.working_dir = Some(dir.clone());

        let result = EditFileTool
            .execute(
                json!({
                    "file_path": path.display().to_string(),
                    "old_string": "foo",
                    "new_string": "bar",
                    "replace_all": true
                }),
                &ctx,
            )
            .await
            .unwrap();

        assert!(!result.is_error);
        assert_eq!(result.metadata.as_ref().unwrap()["replacements"], json!(2));
        let updated = tokio::fs::read_to_string(&path).await.unwrap();
        assert_eq!(updated, "bar = 1\nbar = 2\n");
        assert_eq!(
            result.metadata.as_ref().unwrap()["diff_preview"]["added"][0],
            json!("bar")
        );
        let artifact_path = result.metadata.as_ref().unwrap()["diff_artifact_path"]
            .as_str()
            .unwrap();
        assert!(artifact_path.starts_with(".yode/edit-diffs/"));
        assert_eq!(
            result.metadata.as_ref().unwrap()["full_added_line_count"],
            json!(2)
        );
        let artifact = tokio::fs::read_to_string(dir.join(artifact_path))
            .await
            .unwrap();
        assert!(artifact.contains("-foo"));
        assert!(artifact.contains("+bar"));

        let _ = tokio::fs::remove_dir_all(&dir).await;
    }

    #[tokio::test]
    async fn edits_when_only_trailing_space_or_crlf_differs() {
        let path = temp_path("relaxed-line-endings.txt");
        tokio::fs::write(&path, "func main() {\r\n\tprintln(\"hi\")   \r\n}\r\n")
            .await
            .unwrap();

        let result = EditFileTool
            .execute(
                json!({
                    "file_path": path.display().to_string(),
                    "old_string": "func main() {\n\tprintln(\"hi\")\n}\n",
                    "new_string": "func main() {\n\tprintln(\"hello\")\n}\n"
                }),
                &ToolContext::empty(),
            )
            .await
            .unwrap();

        assert!(!result.is_error);
        let updated = tokio::fs::read_to_string(&path).await.unwrap();
        assert_eq!(updated, "func main() {\n\tprintln(\"hello\")\n}\n");

        let _ = tokio::fs::remove_file(&path).await;
    }

    #[tokio::test]
    async fn relaxed_match_keeps_leading_indentation_significant() {
        let path = temp_path("indentation-sensitive.txt");
        tokio::fs::write(&path, "\treturn nil\n").await.unwrap();

        let result = EditFileTool
            .execute(
                json!({
                    "file_path": path.display().to_string(),
                    "old_string": "    return nil\n",
                    "new_string": "return err\n"
                }),
                &ToolContext::empty(),
            )
            .await
            .unwrap();

        assert!(result.is_error);
        assert!(result.content.contains("exact string"));

        let _ = tokio::fs::remove_file(&path).await;
    }

    #[tokio::test]
    async fn rejects_ambiguous_match_without_replace_all() {
        let path = temp_path("ambiguous.txt");
        tokio::fs::write(&path, "foo = 1\nfoo = 2\n").await.unwrap();

        let mut seen = HashSet::new();
        seen.insert(path.clone());
        let history = Arc::new(Mutex::new(seen));
        let mut ctx = ToolContext::empty();
        ctx.read_file_history = Some(history);

        let result = EditFileTool
            .execute(
                json!({
                    "file_path": path.display().to_string(),
                    "old_string": "foo",
                    "new_string": "bar"
                }),
                &ctx,
            )
            .await
            .unwrap();

        assert!(result.is_error);
        assert!(result.content.contains("replace_all is false"));

        let _ = tokio::fs::remove_file(&path).await;
    }
}
