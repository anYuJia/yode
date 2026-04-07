use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};

use crate::tool::{Tool, ToolContext, ToolResult};

pub struct EditFileTool;

#[async_trait]
impl Tool for EditFileTool {
    fn name(&self) -> &str {
        "edit_file"
    }

    fn description(&self) -> &str {
        r#"Performs exact string replacements in files.

Usage:
- You must use your `read_file` tool at least once in the conversation before editing. This tool will error if you attempt an edit without reading the file.
- When editing text from Read tool output, ensure you preserve the exact indentation (tabs/spaces) as it appears AFTER the line number prefix. The line number prefix format is: line number + tab. Everything after that is the actual file content to match. Never include any part of the line number prefix in the old_string or new_string.
- ALWAYS prefer editing existing files in the codebase. NEVER write new files unless explicitly required.
- Only use emojis if the user explicitly requests it. Avoid adding emojis to files unless asked.
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
                    "description": "The exact string to find and replace"
                },
                "new_string": {
                    "type": "string",
                    "description": "The replacement string"
                },
                "replace_all": {
                    "type": "boolean",
                    "description": "If true, replace all occurrences. Default false (requires unique match)."
                }
            },
            "required": ["file_path", "old_string", "new_string"]
        })
    }

    fn requires_confirmation(&self) -> bool {
        true
    }

    async fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolResult> {
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
                "old_string and new_string are identical.".to_string(),
            ));
        }

        // Count occurrences
        let count = content.matches(old_string).count();

        if count == 0 {
            return Ok(ToolResult::error(format!(
                "old_string not found in '{}'.",
                file_path
            )));
        }

        if count > 1 && !replace_all {
            return Ok(ToolResult::error(format!(
                "old_string found {} times in '{}'. Use replace_all=true to replace all, or provide a more specific old_string.",
                count, file_path
            )));
        }

        // Perform replacement
        let new_content = if replace_all {
            content.replace(old_string, new_string)
        } else {
            content.replacen(old_string, new_string, 1)
        };

        // Write back
        match tokio::fs::write(file_path, &new_content).await {
            Ok(()) => {
                tracing::debug!(
                    file_path = %file_path,
                    replacements = count,
                    "File edited successfully"
                );
                Ok(ToolResult::success(format!(
                    "Successfully replaced {} occurrence(s) in '{}'.",
                    if replace_all { count } else { 1 },
                    file_path
                )))
            }
            Err(e) => Ok(ToolResult::error(format!(
                "Failed to write file '{}': {}",
                file_path, e
            ))),
        }
    }
}
