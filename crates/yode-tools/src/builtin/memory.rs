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
                    .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' || c == '.' { c } else { '_' })
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
        let scope = params.get("scope").and_then(|v| v.as_str()).unwrap_or("project");

        match action {
            "save" => {
                if name.is_empty() {
                    return Ok(ToolResult::error("'name' is required for save action".to_string()));
                }
                if content.is_empty() {
                    return Ok(ToolResult::error("'content' is required for save action".to_string()));
                }
                let dir = Self::memory_dir(ctx, scope)?;
                let safe_name = Self::sanitize_name(name);
                let file_path = dir.join(format!("{}.md", safe_name));
                if let Some(parent) = file_path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(&file_path, content)?;
                Ok(ToolResult::success(format!("Memory '{}' saved to {}", name, file_path.display())))
            }
            "read" => {
                if name.is_empty() {
                    return Ok(ToolResult::error("'name' is required for read action".to_string()));
                }
                let dir = Self::memory_dir(ctx, scope)?;
                let safe_name = Self::sanitize_name(name);
                let file_path = dir.join(format!("{}.md", safe_name));
                match std::fs::read_to_string(&file_path) {
                    Ok(content) => Ok(ToolResult::success(content)),
                    Err(_) => Ok(ToolResult::error(format!("Memory '{}' not found", name))),
                }
            }
            "list" => {
                let dir = Self::memory_dir(ctx, scope)?;
                if !dir.exists() {
                    return Ok(ToolResult::success("No memories found.".to_string()));
                }
                let mut entries = Vec::new();
                collect_memory_files(&dir, &dir, &mut entries)?;
                if entries.is_empty() {
                    Ok(ToolResult::success("No memories found.".to_string()))
                } else {
                    entries.sort();
                    Ok(ToolResult::success(entries.join("\n")))
                }
            }
            "delete" => {
                if name.is_empty() {
                    return Ok(ToolResult::error("'name' is required for delete action".to_string()));
                }
                let dir = Self::memory_dir(ctx, scope)?;
                let safe_name = Self::sanitize_name(name);
                let file_path = dir.join(format!("{}.md", safe_name));
                if file_path.exists() {
                    std::fs::remove_file(&file_path)?;
                    Ok(ToolResult::success(format!("Memory '{}' deleted", name)))
                } else {
                    Ok(ToolResult::error(format!("Memory '{}' not found", name)))
                }
            }
            _ => Ok(ToolResult::error(format!("Unknown action: '{}'. Use save/read/list/delete.", action))),
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
