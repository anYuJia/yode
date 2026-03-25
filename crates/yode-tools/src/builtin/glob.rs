use std::path::Path;
use std::time::SystemTime;

use anyhow::Result;
use async_trait::async_trait;
use globset::{Glob as GlobPattern, GlobSetBuilder};
use serde_json::{json, Value};

use crate::tool::{Tool, ToolContext, ToolResult};

/// Directories to skip when traversing.
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

pub struct GlobTool;

#[async_trait]
impl Tool for GlobTool {
    fn name(&self) -> &str {
        "glob"
    }

    fn description(&self) -> &str {
        "Find files matching a glob pattern. Supports patterns like \"**/*.rs\", \"src/**/*.ts\". Returns matching file paths sorted by modification time."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "Glob pattern to match files (e.g. \"**/*.rs\", \"src/**/*.ts\")"
                },
                "path": {
                    "type": "string",
                    "description": "Directory to search in. Defaults to current working directory."
                }
            },
            "required": ["pattern"]
        })
    }

    fn requires_confirmation(&self) -> bool {
        false
    }

    async fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolResult> {
        let pattern = params
            .get("pattern")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: pattern"))?;

        let base_path = params
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or(".");

        tracing::debug!(pattern = %pattern, path = %base_path, "Glob search");

        let glob = match GlobPattern::new(pattern) {
            Ok(g) => g,
            Err(e) => {
                return Ok(ToolResult::error(format!("Invalid glob pattern: {}", e)));
            }
        };

        let mut builder = GlobSetBuilder::new();
        builder.add(glob);
        let glob_set = match builder.build() {
            Ok(gs) => gs,
            Err(e) => {
                return Ok(ToolResult::error(format!(
                    "Failed to build glob set: {}",
                    e
                )));
            }
        };

        let base = Path::new(base_path);
        if !base.exists() {
            return Ok(ToolResult::error(format!(
                "Path '{}' does not exist.",
                base_path
            )));
        }

        let mut matches: Vec<(String, SystemTime)> = Vec::new();
        walk_dir(base, base, &glob_set, &mut matches);

        // Sort by modification time (newest first)
        matches.sort_by(|a, b| b.1.cmp(&a.1));

        if matches.is_empty() {
            return Ok(ToolResult::success(format!(
                "No files matching '{}' found in '{}'.",
                pattern, base_path
            )));
        }

        let result: Vec<String> = matches.iter().map(|(p, _)| p.clone()).collect();
        Ok(ToolResult::success(result.join("\n")))
    }
}

fn walk_dir(
    base: &Path,
    dir: &Path,
    glob_set: &globset::GlobSet,
    matches: &mut Vec<(String, SystemTime)>,
) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let file_name = entry.file_name();
        let name = file_name.to_string_lossy();

        if path.is_dir() {
            if SKIP_DIRS.contains(&name.as_ref()) {
                continue;
            }
            walk_dir(base, &path, glob_set, matches);
        } else {
            // Get relative path from base
            let rel = path.strip_prefix(base).unwrap_or(&path);
            let rel_str = rel.to_string_lossy();
            if glob_set.is_match(rel_str.as_ref()) {
                let mtime = entry
                    .metadata()
                    .and_then(|m| m.modified())
                    .unwrap_or(SystemTime::UNIX_EPOCH);
                matches.push((rel_str.to_string(), mtime));
            }
        }
    }
}
