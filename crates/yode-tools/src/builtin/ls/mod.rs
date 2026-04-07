use std::path::Path;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};

use crate::tool::{Tool, ToolContext, ToolResult};

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
        let path = params
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or(".");
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

    fn requires_confirmation(&self) -> bool {
        false
    }

    async fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolResult> {
        let path = params
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or(".");

        let recursive = params
            .get("recursive")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let show_hidden = params
            .get("show_hidden")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let base = Path::new(path);
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
        list_dir_with_counts(base, base, recursive, show_hidden, &mut entries, 0, &mut counts);

        let metadata = json!({
            "path": path,
            "recursive": recursive,
            "show_hidden": show_hidden,
            "file_count": counts.0,
            "dir_count": counts.1,
        });

        if entries.is_empty() {
            return Ok(ToolResult::success_with_metadata("(empty directory)".to_string(), metadata));
        }

        Ok(ToolResult::success_with_metadata(entries.join("\n"), metadata))
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
    depth: usize,
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
        let rel = path.strip_prefix(base).unwrap_or(&path);
        let rel_str = rel.to_string_lossy();

        if path.is_dir() {
            counts.1 += 1;
            entries.push(format!("{}/", rel_str));

            if recursive && !SKIP_DIRS.contains(&name_str.as_ref()) {
                list_dir_with_counts(base, &path, recursive, show_hidden, entries, depth + 1, counts);
            }
        } else {
            counts.0 += 1;
            entries.push(rel_str.to_string());
        }
    }
}
