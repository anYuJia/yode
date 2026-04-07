use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};

use crate::tool::{Tool, ToolCapabilities, ToolContext, ToolResult};

pub struct NotebookEditTool;

#[async_trait]
impl Tool for NotebookEditTool {
    fn name(&self) -> &str {
        "notebook_edit"
    }

    fn user_facing_name(&self) -> &str {
        "" 
    }

    fn activity_description(&self, params: &Value) -> String {
        let path = params.get("notebook_path").and_then(|v| v.as_str()).unwrap_or("");
        let mode = params.get("edit_mode").and_then(|v| v.as_str()).unwrap_or("edit");
        format!("Editing notebook {}: {}", mode, path)
    }

    fn description(&self) -> &str {
        "Completely replaces the contents of a specific cell in a Jupyter notebook (.ipynb file) with new source. \
         The notebook_path parameter must be an absolute path. The cell_id can be an actual cell ID or a numeric index (e.g. 'cell-0'). \
         Use edit_mode=insert to add a new cell, or edit_mode=delete to remove one."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "notebook_path": {
                    "type": "string",
                    "description": "The absolute path to the Jupyter notebook file to edit"
                },
                "cell_id": {
                    "type": "string",
                    "description": "The ID of the cell to edit (e.g. 'cell-0', 'cell-1' or a UUID). Required unless inserting at the start."
                },
                "new_source": {
                    "type": "string",
                    "description": "The new source for the cell"
                },
                "cell_type": {
                    "type": "string",
                    "enum": ["code", "markdown"],
                    "description": "The type of the cell (code or markdown). Required for insert mode."
                },
                "edit_mode": {
                    "type": "string",
                    "enum": ["replace", "insert", "delete"],
                    "default": "replace",
                    "description": "The type of edit to make (replace, insert, delete). Defaults to replace."
                }
            },
            "required": ["notebook_path", "new_source"]
        })
    }

    fn capabilities(&self) -> ToolCapabilities {
        ToolCapabilities {
            requires_confirmation: true,
            supports_auto_execution: false,
            read_only: false,
        }
    }

    async fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolResult> {
        let notebook_path = params
            .get("notebook_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("'notebook_path' is required"))?;

        // --- Mandatory Pre-read Check ---
        if let Some(history) = &ctx.read_file_history {
            let h = history.lock().await;
            if !h.contains(&std::path::PathBuf::from(notebook_path)) {
                return Ok(ToolResult::error_typed(
                    format!("Notebook '{}' has not been read yet. Read it first before editing.", notebook_path),
                    crate::tool::ToolErrorType::Validation,
                    true,
                    Some(format!("Call read_file(file_path=\"{}\") first.", notebook_path)),
                ));
            }
        }

        let edit_mode = params.get("edit_mode").and_then(|v| v.as_str()).unwrap_or("replace");
        let cell_id = params.get("cell_id").and_then(|v| v.as_str());
        let new_source = params.get("new_source").and_then(|v| v.as_str()).unwrap_or("");
        let cell_type = params.get("cell_type").and_then(|v| v.as_str());

        // Read and parse
        let content = std::fs::read_to_string(notebook_path)?;
        let mut notebook: Value = serde_json::from_str(&content)?;

        let cells = notebook
            .get_mut("cells")
            .and_then(|v| v.as_array_mut())
            .ok_or_else(|| anyhow::anyhow!("Notebook has no 'cells' array"))?;

        // Resolve cell index from cell_id
        let cell_index = match cell_id {
            Some(id) if id.starts_with("cell-") => {
                id.strip_prefix("cell-")
                    .and_then(|s| s.parse::<usize>().ok())
                    .unwrap_or(0)
            }
            Some(id) => {
                cells.iter().position(|c| c.get("id").and_then(|v| v.as_str()) == Some(id)).unwrap_or(0)
            }
            None => 0,
        };

        match edit_mode {
            "replace" => {
                if cell_index >= cells.len() {
                    return Ok(ToolResult::error(format!("Cell index {} out of range.", cell_index)));
                }
                let cell = &mut cells[cell_index];
                cell["source"] = source_to_lines(new_source);
                if let Some(ct) = cell_type {
                    cell["cell_type"] = json!(ct);
                }
                if cell["cell_type"] == "code" {
                    cell["outputs"] = json!([]);
                    cell["execution_count"] = json!(null);
                }
            }
            "insert" => {
                let ct = cell_type.unwrap_or("code");
                let mut new_cell = json!({
                    "cell_type": ct,
                    "metadata": {},
                    "source": source_to_lines(new_source),
                });
                if ct == "code" {
                    new_cell["outputs"] = json!([]);
                    new_cell["execution_count"] = json!(null);
                }
                let insert_pos = if cell_id.is_some() { cell_index + 1 } else { 0 };
                cells.insert(insert_pos.min(cells.len()), new_cell);
            }
            "delete" => {
                if cell_index < cells.len() {
                    cells.remove(cell_index);
                }
            }
            _ => return Ok(ToolResult::error(format!("Unknown edit_mode: {}", edit_mode))),
        }

        // Write back
        let updated_content = serde_json::to_string_pretty(&notebook)?;
        std::fs::write(notebook_path, &updated_content)?;

        Ok(ToolResult::success(format!("Notebook {} updated successfully ({}).", notebook_path, edit_mode)))
    }
}

fn source_to_lines(source: &str) -> Value {
    let lines: Vec<String> = source
        .lines()
        .map(|l| format!("{}\n", l))
        .collect();
    json!(lines)
}
