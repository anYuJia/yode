use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use std::path::{Path, PathBuf};

use crate::tool::{Tool, ToolCapabilities, ToolContext, ToolResult};

pub struct MemoryTool;

impl MemoryTool {
    fn memory_dir(ctx: &ToolContext, scope: &str) -> Result<PathBuf> {
        match scope {
            "global" => {
                let home = dirs::home_dir()
                    .ok_or_else(|| anyhow::anyhow!("Cannot determine home directory"))?;
                Ok(home.join(".yode").join("memory"))
            }
            _ => {
                // project scope
                let working_dir = ctx
                    .working_dir
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("Working directory not set"))?;
                Ok(working_dir.join(".yode").join("memory"))
            }
        }
    }

    fn sanitize_name(name: &str) -> String {
        // Allow forward slashes for topic organization, but sanitize each segment
        name.split('/')
            .map(|segment| {
                segment
                    .chars()
                    .map(|c| {
                        if c.is_alphanumeric() || c == '-' || c == '_' || c == '.' {
                            c
                        } else {
                            '_'
                        }
                    })
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("/")
    }
}

#[async_trait]
impl Tool for MemoryTool {
    fn name(&self) -> &str {
        "memory"
    }

    fn user_facing_name(&self) -> &str {
        "Memory"
    }

    fn activity_description(&self, params: &Value) -> String {
        let action = params
            .get("action")
            .and_then(|v| v.as_str())
            .unwrap_or("manage");
        let name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
        if name.is_empty() {
            format!("Memory: {} action", action)
        } else {
            format!("Memory {}: {}", action, name)
        }
    }

    fn description(&self) -> &str {
        "Save, read, list, or delete persistent memory entries. Memories are stored as markdown files \
         and persist across sessions. Use scope 'project' for project-specific memories or 'global' \
         for cross-project memories. Names can include '/' for topic organization (e.g. 'auth/login')."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["save", "read", "list", "delete"],
                    "description": "The action to perform"
                },
                "name": {
                    "type": "string",
                    "description": "Memory name (can include '/' for topics). Required for save/read/delete."
                },
                "content": {
                    "type": "string",
                    "description": "Content to save. Required for save action."
                },
                "scope": {
                    "type": "string",
                    "enum": ["project", "global"],
                    "default": "project",
                    "description": "Memory scope: 'project' (default) or 'global'"
                }
            },
            "required": ["action"]
        })
    }

    fn capabilities(&self) -> ToolCapabilities {
        ToolCapabilities {
            requires_confirmation: false,
            supports_auto_execution: true,
            read_only: false,
        }
    }

    async fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolResult> {
        let action = params.get("action").and_then(|v| v.as_str()).unwrap_or("");
        let name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let content = params.get("content").and_then(|v| v.as_str()).unwrap_or("");
        let scope = params
            .get("scope")
            .and_then(|v| v.as_str())
            .unwrap_or("project");

        match action {
            "save" => {
                if name.is_empty() {
                    return Ok(ToolResult::error(
                        "'name' is required for save action".to_string(),
                    ));
                }
                if content.is_empty() {
                    return Ok(ToolResult::error(
                        "'content' is required for save action".to_string(),
                    ));
                }
                let dir = Self::memory_dir(ctx, scope)?;
                let safe_name = Self::sanitize_name(name);
                let file_path = dir.join(format!("{}.md", safe_name));
                if let Some(parent) = file_path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(&file_path, content)?;
                let metadata =
                    serde_json::json!({ "action": "save", "name": name, "scope": scope });
                Ok(ToolResult::success_with_metadata(
                    format!("Memory '{}' saved to {}", name, file_path.display()),
                    metadata,
                ))
            }
            "read" => {
                if name.is_empty() {
                    return Ok(ToolResult::error(
                        "'name' is required for read action".to_string(),
                    ));
                }
                let dir = Self::memory_dir(ctx, scope)?;
                let safe_name = Self::sanitize_name(name);
                let file_path = dir.join(format!("{}.md", safe_name));
                match std::fs::read_to_string(&file_path) {
                    Ok(content) => {
                        let metadata =
                            serde_json::json!({ "action": "read", "name": name, "scope": scope });
                        Ok(ToolResult::success_with_metadata(content, metadata))
                    }
                    Err(_) => Ok(ToolResult::error(format!("Memory '{}' not found", name))),
                }
            }
            "list" => {
                let dir = Self::memory_dir(ctx, scope)?;
                if !dir.exists() {
                    let metadata =
                        serde_json::json!({ "action": "list", "scope": scope, "count": 0 });
                    return Ok(ToolResult::success_with_metadata(
                        "No memories found.".to_string(),
                        metadata,
                    ));
                }
                let mut entries = Vec::new();
                collect_memory_files(&dir, &dir, &mut entries)?;
                let metadata =
                    serde_json::json!({ "action": "list", "scope": scope, "count": entries.len() });
                if entries.is_empty() {
                    Ok(ToolResult::success_with_metadata(
                        "No memories found.".to_string(),
                        metadata,
                    ))
                } else {
                    entries.sort();
                    Ok(ToolResult::success_with_metadata(
                        entries.join("\n"),
                        metadata,
                    ))
                }
            }
            "delete" => {
                if name.is_empty() {
                    return Ok(ToolResult::error(
                        "'name' is required for delete action".to_string(),
                    ));
                }
                let dir = Self::memory_dir(ctx, scope)?;
                let safe_name = Self::sanitize_name(name);
                let file_path = dir.join(format!("{}.md", safe_name));
                if file_path.exists() {
                    std::fs::remove_file(&file_path)?;
                    let metadata =
                        serde_json::json!({ "action": "delete", "name": name, "scope": scope });
                    Ok(ToolResult::success_with_metadata(
                        format!("Memory '{}' deleted", name),
                        metadata,
                    ))
                } else {
                    Ok(ToolResult::error(format!("Memory '{}' not found", name)))
                }
            }
            _ => Ok(ToolResult::error(format!(
                "Unknown action: '{}'. Use save/read/list/delete.",
                action
            ))),
        }
    }
}

/// Recursively collect .md files relative to the base directory.
fn collect_memory_files(dir: &Path, base: &Path, entries: &mut Vec<String>) -> Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_memory_files(&path, base, entries)?;
        } else if path.extension().and_then(|e| e.to_str()) == Some("md") {
            if let Ok(rel) = path.strip_prefix(base) {
                let name = rel.with_extension("").display().to_string();
                entries.push(name);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::tool::Tool;

    use super::MemoryTool;

    #[tokio::test]
    async fn memory_save_read_list_delete_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let mut ctx = crate::tool::ToolContext::empty();
        ctx.working_dir = Some(dir.path().to_path_buf());

        let save = MemoryTool
            .execute(
                json!({
                    "action": "save",
                    "name": "auth/login",
                    "content": "remember this",
                    "scope": "project"
                }),
                &ctx,
            )
            .await
            .unwrap();
        assert!(!save.is_error);

        let read = MemoryTool
            .execute(
                json!({
                    "action": "read",
                    "name": "auth/login",
                    "scope": "project"
                }),
                &ctx,
            )
            .await
            .unwrap();
        assert_eq!(read.content, "remember this");

        let list = MemoryTool
            .execute(
                json!({
                    "action": "list",
                    "scope": "project"
                }),
                &ctx,
            )
            .await
            .unwrap();
        assert!(list.content.contains("auth/login"));

        let delete = MemoryTool
            .execute(
                json!({
                    "action": "delete",
                    "name": "auth/login",
                    "scope": "project"
                }),
                &ctx,
            )
            .await
            .unwrap();
        assert!(!delete.is_error);
    }

    #[tokio::test]
    async fn memory_list_empty_scope_reports_no_memories() {
        let dir = tempfile::tempdir().unwrap();
        let mut ctx = crate::tool::ToolContext::empty();
        ctx.working_dir = Some(dir.path().to_path_buf());

        let result = MemoryTool
            .execute(
                json!({
                    "action": "list",
                    "scope": "project"
                }),
                &ctx,
            )
            .await
            .unwrap();

        assert!(!result.is_error);
        assert!(result.content.contains("No memories found"));
        assert_eq!(result.metadata.as_ref().unwrap()["count"], json!(0));
    }
}
