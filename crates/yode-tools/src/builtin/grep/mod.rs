use std::path::Path;
use std::process::Command;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};

use crate::tool::{Tool, ToolCapabilities, ToolContext, ToolResult};

pub struct GrepTool;

#[async_trait]
impl Tool for GrepTool {
    fn name(&self) -> &str {
        "grep"
    }

    fn user_facing_name(&self) -> &str {
        "Search" 
    }

    fn activity_description(&self, params: &Value) -> String {
        let pattern = params.get("pattern").and_then(|v| v.as_str()).unwrap_or("");
        format!("Searching for: {}", pattern)
    }

    fn description(&self) -> &str {
        r#"A powerful search tool built on ripgrep.

Usage:
- ALWAYS use Grep for search tasks. NEVER invoke `grep` or `rg` as a Bash command.
- Supports full regex syntax (e.g., "log.*Error", "function\s+\w+").
- Output modes: "content" (matching lines), "files_with_matches" (paths only), "count" (match counts).
- Supports multiline matching with multiline: true.
- Use Agent tool for open-ended searches requiring multiple rounds."#
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
                    "description": "File or directory to search in. Defaults to current working directory."
                },
                "glob": {
                    "type": "string",
                    "description": "Glob pattern to filter files (e.g. '*.js', '*.{ts,tsx}') - maps to rg --glob"
                },
                "output_mode": {
                    "type": "string",
                    "enum": ["content", "files_with_matches", "count"],
                    "default": "files_with_matches",
                    "description": "Output mode: 'content' (matching lines), 'files_with_matches' (paths only), 'count' (match counts)."
                },
                "context": {
                    "type": "integer",
                    "description": "Number of lines to show before and after each match (rg -C)."
                },
                "context_before": {
                    "type": "integer",
                    "description": "Number of lines to show before each match (rg -B)."
                },
                "context_after": {
                    "type": "integer",
                    "description": "Number of lines to show after each match (rg -A)."
                },
                "case_insensitive": {
                    "type": "boolean",
                    "default": false,
                    "description": "Case insensitive search (rg -i)."
                },
                "multiline": {
                    "type": "boolean",
                    "default": false,
                    "description": "Enable multiline mode (rg -U)."
                },
                "head_limit": {
                    "type": "integer",
                    "default": 250,
                    "description": "Limit output to first N lines/entries. Pass 0 for unlimited."
                },
                "offset": {
                    "type": "integer",
                    "default": 0,
                    "description": "Skip first N lines/entries before applying head_limit."
                }
            },
            "required": ["pattern"]
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
        let pattern = params.get("pattern").and_then(|v| v.as_str()).unwrap_or("");
        let path = params.get("path").and_then(|v| v.as_str()).unwrap_or(".");
        let output_mode = params.get("output_mode").and_then(|v| v.as_str()).unwrap_or("files_with_matches");
        
        let mut args = vec!["--hidden".to_string()];
        
        // Output mode
        match output_mode {
            "files_with_matches" => args.push("-l".to_string()),
            "count" => args.push("-c".to_string()),
            _ => args.push("-n".to_string()), // content mode includes line numbers
        }

        // Context
        if let Some(c) = params.get("context").and_then(|v| v.as_u64()) {
            args.push("-C".to_string());
            args.push(c.to_string());
        } else {
            if let Some(b) = params.get("context_before").and_then(|v| v.as_u64()) {
                args.push("-B".to_string());
                args.push(b.to_string());
            }
            if let Some(a) = params.get("context_after").and_then(|v| v.as_u64()) {
                args.push("-A".to_string());
                args.push(a.to_string());
            }
        }

        if params.get("case_insensitive").and_then(|v| v.as_bool()).unwrap_or(false) {
            args.push("-i".to_string());
        }

        if params.get("multiline").and_then(|v| v.as_bool()).unwrap_or(false) {
            args.push("-U".to_string());
            args.push("--multiline-dotall".to_string());
        }

        if let Some(glob) = params.get("glob").and_then(|v| v.as_str()) {
            args.push("--glob".to_string());
            args.push(glob.to_string());
        }

        // Limit column length to avoid base64 noise
        args.push("--max-columns".to_string());
        args.push("500".to_string());

        // Pattern and Path
        args.push(pattern.to_string());
        args.push(path.to_string());

        let working_dir = ctx.working_dir.as_deref().unwrap_or_else(|| Path::new("."));

        // Execute ripgrep
        let output = match Command::new("rg").args(&args).current_dir(working_dir).output() {
            Ok(o) => o,
            Err(_) => {
                // Fallback to internal implementation if rg is not installed
                return Ok(ToolResult::error("ripgrep (rg) is not installed in the system path.".to_string()));
            }
        };

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        if !output.status.success() && stderr.len() > 0 {
            return Ok(ToolResult::error(format!("rg error: {}", stderr)));
        }

        if stdout.is_empty() {
            return Ok(ToolResult::success("No matches found.".to_string()));
        }

        // Apply head_limit and offset
        let head_limit = params.get("head_limit").and_then(|v| v.as_u64()).unwrap_or(250) as usize;
        let offset = params.get("offset").and_then(|v| v.as_u64()).unwrap_or(0) as usize;

        let lines: Vec<&str> = stdout.lines().collect();
        let total_count = lines.len();
        
        let start = offset.min(total_count);
        let end = if head_limit == 0 { total_count } else { (start + head_limit).min(total_count) };
        
        let result_lines = &lines[start..end];
        let mut final_output = result_lines.join("\n");

        if end < total_count {
            final_output.push_str(&format!("\n\n[Showing results with pagination = limit: {}, offset: {}]", head_limit, offset));
        }

        Ok(ToolResult::success(final_output))
    }
}
