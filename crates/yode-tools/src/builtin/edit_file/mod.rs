use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};

use crate::tool::{Tool, ToolContext, ToolResult};

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
- You must use the `read_file` tool at least once in the conversation before editing. This tool will error if you attempt an edit without reading the file.
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

    fn requires_confirmation(&self) -> bool {
        true
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

        // --- Mandatory Pre-read Check ---
        if let Some(history) = &ctx.read_file_history {
            let h = history.lock().await;
            if !h.contains(&std::path::PathBuf::from(file_path)) {
                return Ok(ToolResult::error_typed(
                    format!("File '{}' has not been read yet. You must use 'read_file' at least once in the conversation before editing.", file_path),
                    crate::tool::ToolErrorType::Validation,
                    true,
                    Some(format!("Call read_file(file_path=\"{}\") first.", file_path)),
                ));
            }
        }

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

        // --- Smarter String Matching (Quote Robustness) ---
        let actual_old = old_string.to_string();

        // Count occurrences
        let count = content.matches(&actual_old).count();

        if count == 0 {
            return Ok(ToolResult::error(format!(
                "The exact string to replace was not found in '{}'. \
                 Ensure you provided the EXACT text including indentation and quotes as seen in 'read_file' output.",
                file_path
            )));
        }

        if count > 1 && !replace_all {
            return Ok(ToolResult::error(format!(
                "Found {} matches of the string to replace in '{}', but replace_all is false. \
                 Please provide more context to uniquely identify the instance or set replace_all=true.",
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
                let metadata = json!({
                    "file_path": file_path,
                    "replacements": replacements,
                    "diff_preview": {
                        "removed": removed,
                        "added": added,
                        "more_removed": actual_old.lines().count().saturating_sub(5),
                        "more_added": new_string.lines().count().saturating_sub(5),
                    },
                });

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
