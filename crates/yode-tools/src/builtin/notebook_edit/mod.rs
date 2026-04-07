use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;

use crate::tool::{Tool, ToolCapabilities, ToolContext, ToolResult};

pub struct NotebookEditTool;

#[async_trait]
impl Tool for NotebookEditTool {
    fn name(&self) -> &str {
        "notebook_edit"
    }

    fn user_facing_name(&self) -> &str {
        "Notebook Edit"
    }

    fn activity_description(&self, params: &Value) -> String {
        let path = params.get("notebook_path").and_then(|v| v.as_str()).unwrap_or("");
        let mode = params.get("edit_mode").and_then(|v| v.as_str()).unwrap_or("edit");
        format!("Jupyter {}: {}", mode, path)
    }

    fn description(&self) -> &str {
        "Edit a Jupyter notebook (.ipynb) cell. Supports replacing, inserting, or deleting cells."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "notebook_path": {
                    "type": "string",
                    "description": "Absolute path to the .ipynb file"
                },
                "cell_number": {
                    "type": "integer",
                    "description": "0-based cell index to operate on"
                },
                "edit_mode": {
                    "type": "string",
                    "enum": ["replace", "insert", "delete"],
                    "default": "replace",
                    "description": "replace: overwrite cell, insert: add new cell at index, delete: remove cell"
                },
                "cell_type": {
                    "type": "string",
                    "enum": ["code", "markdown"],
                    "description": "Cell type. Required for insert mode."
                },
                "new_source": {
                    "type": "string",
                    "description": "New cell content. Required for replace/insert."
                }
            },
            "required": ["notebook_path", "cell_number", "new_source"]
        })
    }

    fn capabilities(&self) -> ToolCapabilities {
        ToolCapabilities {
            requires_confirmation: true,
            supports_auto_execution: false,
            read_only: false,
        }
    }

    async fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolResult> {
        let path = params.get("notebook_path").and_then(|v| v.as_str()).unwrap_or("");
        let cell_number = params.get("cell_number").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
        let edit_mode = params.get("edit_mode").and_then(|v| v.as_str()).unwrap_or("replace");
        let cell_type = params.get("cell_type").and_then(|v| v.as_str()).unwrap_or("code");
        let new_source = params.get("new_source").and_then(|v| v.as_str()).unwrap_or("");

        if path.is_empty() {
            return Ok(ToolResult::error("notebook_path is required".to_string()));
        }

        // Read notebook
        let content = std::fs::read_to_string(path)
            .map_err(|e| anyhow::anyhow!("Failed to read notebook: {}", e))?;
        let mut notebook: Value = serde_json::from_str(&content)
            .map_err(|e| anyhow::anyhow!("Failed to parse notebook JSON: {}", e))?;

        let cells = notebook
            .get_mut("cells")
            .and_then(|v| v.as_array_mut())
            .ok_or_else(|| anyhow::anyhow!("Notebook has no 'cells' array"))?;

        match edit_mode {
            "replace" => {
                if cell_number >= cells.len() {
                    return Ok(ToolResult::error(format!(
                        "Cell {} does not exist (notebook has {} cells)",
                        cell_number,
                        cells.len()
                    )));
                }
                // Split source into lines for ipynb format
                let source_lines = source_to_lines(new_source);
                cells[cell_number]["source"] = source_lines;
                if !cell_type.is_empty() {
                    cells[cell_number]["cell_type"] = Value::String(cell_type.to_string());
                }
            }
            "insert" => {
                if cell_number > cells.len() {
                    return Ok(ToolResult::error(format!(
                        "Insert position {} is out of range (notebook has {} cells)",
                        cell_number,
                        cells.len()
                    )));
                }
                let source_lines = source_to_lines(new_source);
                let new_cell = serde_json::json!({
                    "cell_type": cell_type,
                    "metadata": {},
                    "source": source_lines,
                    "outputs": if cell_type == "code" { Value::Array(vec![]) } else { Value::Null }
                });
                // Remove null outputs for markdown cells
                let mut new_cell_map = new_cell;
                if cell_type != "code" {
                    if let Some(obj) = new_cell_map.as_object_mut() {
                        obj.remove("outputs");
                    }
                }
                cells.insert(cell_number, new_cell_map);
            }
            "delete" => {
                if cell_number >= cells.len() {
                    return Ok(ToolResult::error(format!(
                        "Cell {} does not exist (notebook has {} cells)",
                        cell_number,
                        cells.len()
                    )));
                }
                cells.remove(cell_number);
            }
            _ => {
                return Ok(ToolResult::error(format!("Unknown edit_mode: '{}'", edit_mode)));
            }
        }

        // Write back
        let output = serde_json::to_string_pretty(&notebook)?;
        std::fs::write(path, output)?;

        let metadata = serde_json::json!({
            "notebook_path": path,
            "edit_mode": edit_mode,
            "cell_number": cell_number,
        });

        Ok(ToolResult::success_with_metadata(
            format!("Notebook {} updated: {} cell at index {}", path, edit_mode, cell_number),
            metadata
        ))
    }
}

/// Convert a source string to the ipynb line-array format.
fn source_to_lines(source: &str) -> Value {
    let lines: Vec<String> = source
        .split('\n')
        .enumerate()
        .map(|(i, line)| {
            // All lines except the last get a trailing newline
            if i < source.split('\n').count() - 1 {
                format!("{}\n", line)
            } else {
                line.to_string()
            }
        })
        .collect();
    serde_json::json!(lines)
}
