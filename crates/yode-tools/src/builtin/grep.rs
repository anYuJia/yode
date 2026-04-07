use std::path::Path;

use anyhow::Result;
use async_trait::async_trait;
use regex::Regex;
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

/// File extensions considered binary (skip).
const BINARY_EXTENSIONS: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "bmp", "ico", "svg", "woff", "woff2", "ttf", "eot", "otf",
    "mp3", "mp4", "avi", "mov", "zip", "tar", "gz", "bz2", "7z", "rar", "pdf", "doc", "docx",
    "xls", "xlsx", "ppt", "pptx", "exe", "dll", "so", "dylib", "o", "a", "class", "jar", "war",
    "pyc", "pyo", "wasm",
];

pub struct GrepTool;

#[async_trait]
impl Tool for GrepTool {
    fn name(&self) -> &str {
        "grep"
    }

    fn description(&self) -> &str {
        r#"A powerful search tool built on ripgrep.

Usage:
- ALWAYS use Grep for search tasks. NEVER invoke `grep` or `rg` as a Bash command. The Grep tool has been optimized for correct permissions and access.
- Supports full regex syntax (e.g., "log.*Error", "function\s+\w+")
- Filter files with glob parameter (e.g., "*.js", "*.{ts,tsx}") or type parameter
- Output modes: "content" shows matching lines, "files_with_matches" shows only file paths, "count" shows match counts
- Use Agent tool for open-ended searches requiring multiple rounds

Examples:
- Pattern: "fn\s+\w+" to find function definitions
- Pattern: "impl\s+\w+" to find implementations
- Glob: "*.rs" to search only Rust files
- Context: Set to 2 to show 2 lines before and after each match"#
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "Regex pattern to search for in file contents"
                },
                "path": {
                    "type": "string",
                    "description": "Directory or file to search in. Defaults to current working directory."
                },
                "glob": {
                    "type": "string",
                    "description": "Optional glob pattern to filter files (e.g. \"*.rs\", \"*.{ts,tsx}\")"
                },
                "context": {
                    "type": "integer",
                    "description": "Number of context lines before and after each match. Default 0."
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

        let file_glob = params.get("glob").and_then(|v| v.as_str());

        let context_lines = params
            .get("context")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;

        tracing::debug!(
            pattern = %pattern,
            path = %base_path,
            glob = ?file_glob,
            context = context_lines,
            "Grep search"
        );

        let re = match Regex::new(pattern) {
            Ok(r) => r,
            Err(e) => {
                return Ok(ToolResult::error(format!(
                    "Invalid regex pattern: {}",
                    e
                )));
            }
        };

        let glob_set = if let Some(glob_pat) = file_glob {
            match globset::Glob::new(glob_pat) {
                Ok(g) => {
                    let mut builder = globset::GlobSetBuilder::new();
                    builder.add(g);
                    builder.build().ok()
                }
                Err(e) => {
                    return Ok(ToolResult::error(format!(
                        "Invalid glob filter: {}",
                        e
                    )));
                }
            }
        } else {
            None
        };

        let base = Path::new(base_path);
        if !base.exists() {
            return Ok(ToolResult::error(format!(
                "Path '{}' does not exist.",
                base_path
            )));
        }

        let mut results = Vec::new();
        let mut file_count = 0u32;
        let max_results = 200;

        search_dir(
            base,
            base,
            &re,
            glob_set.as_ref(),
            context_lines,
            &mut results,
            &mut file_count,
            max_results,
        );

        if results.is_empty() {
            return Ok(ToolResult::success(format!(
                "No matches found for '{}' in '{}'.",
                pattern, base_path
            )));
        }

        let output = results.join("\n");

        // Truncate if too long
        if output.len() > 50_000 {
            let truncated: String = output.chars().take(50_000).collect();
            Ok(ToolResult::success(format!(
                "{}\n\n...(results truncated, {} files matched)",
                truncated, file_count
            )))
        } else {
            Ok(ToolResult::success(format!(
                "{}\n\n{} file(s) matched.",
                output, file_count
            )))
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn search_dir(
    base: &Path,
    dir: &Path,
    re: &Regex,
    glob_set: Option<&globset::GlobSet>,
    context_lines: usize,
    results: &mut Vec<String>,
    file_count: &mut u32,
    max_results: usize,
) {
    if results.len() >= max_results {
        return;
    }

    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        if results.len() >= max_results {
            return;
        }

        let path = entry.path();
        let file_name = entry.file_name();
        let name = file_name.to_string_lossy();

        if path.is_dir() {
            if SKIP_DIRS.contains(&name.as_ref()) {
                continue;
            }
            search_dir(base, &path, re, glob_set, context_lines, results, file_count, max_results);
        } else {
            // Skip binary files
            if let Some(ext) = path.extension() {
                if BINARY_EXTENSIONS.contains(&ext.to_string_lossy().to_lowercase().as_str()) {
                    continue;
                }
            }

            let rel = path.strip_prefix(base).unwrap_or(&path);
            let rel_str = rel.to_string_lossy();

            // Apply glob filter
            if let Some(gs) = glob_set {
                if !gs.is_match(rel_str.as_ref()) {
                    // Also try matching just the filename
                    if !gs.is_match(name.as_ref()) {
                        continue;
                    }
                }
            }

            // Read and search file
            let content = match std::fs::read_to_string(&path) {
                Ok(c) => c,
                Err(_) => continue, // Skip files that can't be read as UTF-8
            };

            let lines: Vec<&str> = content.lines().collect();
            let mut file_matches = Vec::new();

            for (i, line) in lines.iter().enumerate() {
                if re.is_match(line) {
                    let start = i.saturating_sub(context_lines);
                    let end = (i + context_lines + 1).min(lines.len());

                    for j in start..end {
                        let prefix = if j == i { ">" } else { " " };
                        file_matches.push(format!(
                            "{}{}:{}", prefix, j + 1, lines[j]
                        ));
                    }
                    if context_lines > 0 && end < lines.len() {
                        file_matches.push("---".to_string());
                    }
                }
            }

            if !file_matches.is_empty() {
                *file_count += 1;
                results.push(format!("{}:\n{}", rel_str, file_matches.join("\n")));
            }
        }
    }
}
