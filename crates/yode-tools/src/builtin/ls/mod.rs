use std::path::{Path, PathBuf};

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};

use crate::path_format::relative_display_slash;
use crate::tool::{Tool, ToolCapabilities, ToolContext, ToolResult};

pub struct LsTool;

#[async_trait]
impl Tool for LsTool {
    fn name(&self) -> &str {
        "ls"
    }

    fn user_facing_name(&self) -> &str {
        "List Directory"
    }

    fn activity_description(&self, params: &Value) -> String {
        let path = params.get("path").and_then(|v| v.as_str()).unwrap_or(".");
        format!("Listing directory: {}", path)
    }

    fn description(&self) -> &str {
        "List files and directories in a given path. Returns names with type indicators (/ for directories)."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Directory path to list. Defaults to current directory."
                },
                "recursive": {
                    "type": "boolean",
                    "description": "If true, list recursively. Default false."
                },
                "show_hidden": {
                    "type": "boolean",
                    "description": "If true, show hidden files (starting with .). Default false."
                }
            }
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
        let path = params.get("path").and_then(|v| v.as_str()).unwrap_or(".");

        let recursive = params
            .get("recursive")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let show_hidden = params
            .get("show_hidden")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let base = resolve_path(path, ctx.working_dir.as_deref());
        if !base.exists() {
            return Ok(ToolResult::error(format!(
                "Path '{}' does not exist.",
                path
            )));
        }
        if !base.is_dir() {
            return Ok(ToolResult::error(format!(
                "Path '{}' is not a directory.",
                path
            )));
        }

        let mut entries = Vec::new();
        let mut counts = (0, 0); // (files, dirs)
        list_dir_with_counts(
            &base,
            &base,
            recursive,
            show_hidden,
            &mut entries,
            &mut counts,
        );

        let metadata = json!({
            "path": path,
            "resolved_path": base.display().to_string(),
            "recursive": recursive,
            "show_hidden": show_hidden,
            "file_count": counts.0,
            "dir_count": counts.1,
        });

        if entries.is_empty() {
            return Ok(ToolResult::success_with_metadata(
                "(empty directory)".to_string(),
                metadata,
            ));
        }

        Ok(ToolResult::success_with_metadata(
            entries.join("\n"),
            metadata,
        ))
    }
}

fn resolve_path(path: &str, working_dir: Option<&Path>) -> PathBuf {
    let raw = Path::new(path);
    if raw.is_absolute() {
        raw.to_path_buf()
    } else {
        working_dir.unwrap_or_else(|| Path::new(".")).join(raw)
    }
}

const SKIP_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    ".next",
    "__pycache__",
    ".venv",
    "venv",
    "dist",
    "build",
    ".cache",
];

fn list_dir_with_counts(
    base: &Path,
    dir: &Path,
    recursive: bool,
    show_hidden: bool,
    entries: &mut Vec<String>,
    counts: &mut (usize, usize),
) {
    let mut dir_entries: Vec<_> = match std::fs::read_dir(dir) {
        Ok(e) => e.flatten().collect(),
        Err(_) => return,
    };

    dir_entries.sort_by_key(|e| e.file_name());

    for entry in dir_entries {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        // Skip hidden files unless requested
        if !show_hidden && name_str.starts_with('.') {
            continue;
        }

        let path = entry.path();
        let rel_str = relative_display_slash(&path, base);

        if path.is_dir() {
            counts.1 += 1;
            entries.push(format!("{}/", rel_str));

            if recursive && !SKIP_DIRS.contains(&name_str.as_ref()) {
                list_dir_with_counts(base, &path, recursive, show_hidden, entries, counts);
            }
        } else {
            counts.0 += 1;
            entries.push(rel_str);
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::tool::Tool;

    use super::LsTool;

    #[tokio::test]
    async fn lists_recursive_entries_and_metadata() {
        let dir = tempfile::tempdir().unwrap();
        tokio::fs::create_dir_all(dir.path().join("src").join("nested"))
            .await
            .unwrap();
        tokio::fs::write(
            dir.path().join("src").join("nested").join("main.rs"),
            "fn main() {}",
        )
        .await
        .unwrap();

        let result = LsTool
            .execute(
                json!({
                    "path": dir.path().display().to_string(),
                    "recursive": true
                }),
                &crate::tool::ToolContext::empty(),
            )
            .await
            .unwrap();

        assert!(!result.is_error);
        assert!(result.content.contains("src/"));
        assert!(result.content.contains("src/nested/"));
        assert!(result.content.contains("src/nested/main.rs"));
        assert_eq!(result.metadata.as_ref().unwrap()["dir_count"], json!(2));
        assert_eq!(result.metadata.as_ref().unwrap()["file_count"], json!(1));
    }

    #[tokio::test]
    async fn hides_hidden_files_unless_requested() {
        let dir = tempfile::tempdir().unwrap();
        tokio::fs::write(dir.path().join(".secret"), "shh")
            .await
            .unwrap();

        let hidden = LsTool
            .execute(
                json!({
                    "path": dir.path().display().to_string(),
                    "show_hidden": false
                }),
                &crate::tool::ToolContext::empty(),
            )
            .await
            .unwrap();
        assert!(!hidden.content.contains(".secret"));

        let shown = LsTool
            .execute(
                json!({
                    "path": dir.path().display().to_string(),
                    "show_hidden": true
                }),
                &crate::tool::ToolContext::empty(),
            )
            .await
            .unwrap();
        assert!(shown.content.contains(".secret"));
    }

    #[tokio::test]
    async fn rejects_non_directory_paths() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("file.txt");
        tokio::fs::write(&file, "x").await.unwrap();

        let result = LsTool
            .execute(
                json!({
                    "path": file.display().to_string()
                }),
                &crate::tool::ToolContext::empty(),
            )
            .await
            .unwrap();

        assert!(result.is_error);
        assert!(result.content.contains("is not a directory"));
    }

    #[tokio::test]
    async fn resolves_relative_paths_against_tool_working_dir() {
        let dir = tempfile::tempdir().unwrap();
        tokio::fs::write(dir.path().join("workspace-file.txt"), "x")
            .await
            .unwrap();

        let mut ctx = crate::tool::ToolContext::empty();
        ctx.working_dir = Some(dir.path().to_path_buf());

        let result = LsTool
            .execute(
                json!({
                    "path": "."
                }),
                &ctx,
            )
            .await
            .unwrap();

        assert!(!result.is_error);
        assert!(result.content.contains("workspace-file.txt"));
        assert_eq!(
            result.metadata.as_ref().unwrap()["resolved_path"],
            json!(dir.path().join(".").display().to_string())
        );
    }
}
