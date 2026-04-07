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

    fn user_facing_name(&self) -> &str {
        "Glob"
    }

    fn activity_description(&self, params: &Value) -> String {
        let pattern = params
            .get("pattern")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        format!("Searching for files: {}", pattern)
    }

    fn description(&self) -> &str {
        r#"Fast file pattern matching tool that works with any codebase size.

Supports glob patterns like "**/*.js" or "src/**/*.ts".
Returns matching file paths sorted by modification time.
Use this tool when you need to find files by name patterns.

When you are doing an open ended search that may require multiple rounds of globbing and grepping, use the Agent tool instead.

Examples:
- "**/*.rs" - Find all Rust files
- "src/**/*.ts" - Find TypeScript files in src
- "Cargo.toml" - Find Cargo.toml files
- "**/*.md" - Find all Markdown files"#
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "The glob pattern to match files against"
                },
                "path": {
                    "type": "string",
                    "description": "The directory to search in. If not specified, the current working directory will be used. Omit this field to use the default directory."
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
            let metadata = json!({
                "pattern": pattern,
                "path": base_path,
                "match_count": 0,
            });
            return Ok(ToolResult::success_with_metadata(format!(
                "No files matching '{}' found in '{}'.",
                pattern, base_path
            ), metadata));
        }

        let result: Vec<String> = matches.iter().map(|(p, _)| p.clone()).collect();
        let metadata = json!({
            "pattern": pattern,
            "path": base_path,
            "match_count": result.len(),
        });
        Ok(ToolResult::success_with_metadata(result.join("\n"), metadata))
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
