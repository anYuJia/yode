use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use serde_json::{json, Value};
use yode_index::{IndexOptions, RepositoryIndex, SearchHit};

use crate::tool::{Tool, ToolCapabilities, ToolContext, ToolResult};

pub struct RepositorySearchTool;

const DEFAULT_LIMIT: usize = 8;
const MAX_LIMIT: usize = 24;

#[async_trait]
impl Tool for RepositorySearchTool {
    fn name(&self) -> &str {
        "repository_search"
    }

    fn user_facing_name(&self) -> &str {
        "Repository Search"
    }

    fn description(&self) -> &str {
        "Search a persistent repository intelligence index by code concept, symbol, file path, or import relationship. Prefer this before broad grep when exploring an unfamiliar codebase. Supports search, stats, rebuild, and single-file incremental update."
    }

    fn activity_description(&self, params: &Value) -> String {
        match params
            .get("action")
            .and_then(Value::as_str)
            .unwrap_or("search")
        {
            "rebuild" => "Rebuilding repository intelligence index".to_string(),
            "stats" => "Inspecting repository intelligence index".to_string(),
            "update" => format!(
                "Updating repository index: {}",
                params.get("path").and_then(Value::as_str).unwrap_or("file")
            ),
            _ => format!(
                "Searching repository: {}",
                params.get("query").and_then(Value::as_str).unwrap_or("")
            ),
        }
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["search", "stats", "rebuild", "update"],
                    "default": "search",
                    "description": "search queries the persisted index; stats reports coverage; rebuild rescans the repository; update refreshes one relative file path"
                },
                "query": {
                    "type": "string",
                    "description": "Concept, symbol, file, module, or import relationship to find"
                },
                "path": {
                    "type": "string",
                    "description": "Repository-relative file path for action=update"
                },
                "limit": {
                    "type": "integer",
                    "minimum": 1,
                    "maximum": 24,
                    "default": 8
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
        let root = ctx
            .working_dir
            .clone()
            .ok_or_else(|| anyhow!("Working directory not set"))?;
        let action = params
            .get("action")
            .and_then(Value::as_str)
            .unwrap_or("search")
            .to_string();
        let query = params
            .get("query")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .to_string();
        let relative_path = params
            .get("path")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let limit = params
            .get("limit")
            .and_then(Value::as_u64)
            .unwrap_or(DEFAULT_LIMIT as u64) as usize;
        let limit = limit.clamp(1, MAX_LIMIT);

        tokio::task::spawn_blocking(move || {
            execute_repository_search(&root, &action, &query, relative_path.as_deref(), limit)
        })
        .await
        .context("repository_search blocking task failed")?
    }
}

fn execute_repository_search(
    root: &Path,
    action: &str,
    query: &str,
    relative_path: Option<&str>,
    limit: usize,
) -> Result<ToolResult> {
    let options = IndexOptions::default();
    let index_path = index_path(root);

    match action {
        "rebuild" => {
            let index = RepositoryIndex::build(root, options)?;
            index.save_json(&index_path)?;
            let stats = index.stats();
            Ok(ToolResult::success_with_metadata(
                format!(
                    "Repository index rebuilt: {} files, {} symbols, {} searchable terms.",
                    stats.files, stats.symbols, stats.unique_terms
                ),
                json!({
                    "action": "rebuild",
                    "index_path": index_path.display().to_string(),
                    "stats": stats
                }),
            ))
        }
        "stats" => {
            let (index, rebuilt) = load_or_build(root, &index_path, options)?;
            let stats = index.stats();
            Ok(ToolResult::success_with_metadata(
                format!(
                    "Repository index: {} files, {} symbols, {} searchable terms{}.",
                    stats.files,
                    stats.symbols,
                    stats.unique_terms,
                    if rebuilt { " (rebuilt)" } else { "" }
                ),
                json!({
                    "action": "stats",
                    "rebuilt": rebuilt,
                    "index_path": index_path.display().to_string(),
                    "stats": stats
                }),
            ))
        }
        "update" => {
            let relative_path = relative_path
                .ok_or_else(|| anyhow!("action=update requires repository-relative 'path'"))?;
            validate_relative_path(relative_path)?;
            let (mut index, rebuilt) = load_or_build(root, &index_path, options.clone())?;
            index.update_file(relative_path, &options)?;
            index.save_json(&index_path)?;
            let hits = index.search(relative_path, 4);
            Ok(ToolResult::success_with_metadata(
                format!("Repository index updated for `{relative_path}`."),
                json!({
                    "action": "update",
                    "path": relative_path,
                    "rebuilt_before_update": rebuilt,
                    "index_path": index_path.display().to_string(),
                    "hits": hits,
                    "stats": index.stats()
                }),
            ))
        }
        "search" => {
            if query.is_empty() {
                return Ok(ToolResult::error_typed(
                    "repository_search action=search requires a non-empty query".to_string(),
                    crate::tool::ToolErrorType::Validation,
                    true,
                    Some("Search for a code concept such as 'session restore auth' or a symbol name.".to_string()),
                ));
            }
            let (index, rebuilt) = load_or_build(root, &index_path, options)?;
            let hits = index.search(query, limit);
            let body = render_hits(query, &hits, rebuilt);
            Ok(ToolResult::success_with_metadata(
                body,
                json!({
                    "action": "search",
                    "query": query,
                    "rebuilt": rebuilt,
                    "index_path": index_path.display().to_string(),
                    "hits": hits,
                    "stats": index.stats()
                }),
            ))
        }
        other => Ok(ToolResult::error_typed(
            format!("Unknown repository_search action '{other}'"),
            crate::tool::ToolErrorType::Validation,
            true,
            Some("Use search, stats, rebuild, or update.".to_string()),
        )),
    }
}

fn load_or_build(
    root: &Path,
    index_path: &Path,
    options: IndexOptions,
) -> Result<(RepositoryIndex, bool)> {
    if index_path.exists() {
        match RepositoryIndex::load_json(index_path) {
            Ok(index) if same_root(&index, root) => return Ok((index, false)),
            Ok(_) | Err(_) => {}
        }
    }
    let index = RepositoryIndex::build(root, options)?;
    index.save_json(index_path)?;
    Ok((index, true))
}

fn same_root(index: &RepositoryIndex, root: &Path) -> bool {
    root.canonicalize()
        .ok()
        .is_some_and(|canonical| canonical.display().to_string() == index.root)
}

fn index_path(root: &Path) -> PathBuf {
    root.join(".yode")
        .join("index")
        .join("repository-index.json")
}

fn validate_relative_path(path: &str) -> Result<()> {
    let candidate = Path::new(path);
    if candidate.is_absolute()
        || candidate
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err(anyhow!("index update path must stay inside the repository"));
    }
    Ok(())
}

fn render_hits(query: &str, hits: &[SearchHit], rebuilt: bool) -> String {
    if hits.is_empty() {
        return format!(
            "No repository-index matches for `{query}`.{}",
            if rebuilt {
                " The index was rebuilt first."
            } else {
                ""
            }
        );
    }

    let mut output = format!(
        "Repository intelligence matches for `{query}`{}:\n",
        if rebuilt { " (index rebuilt)" } else { "" }
    );
    for (index, hit) in hits.iter().enumerate() {
        output.push_str(&format!(
            "\n{}. `{}` — score {:.2}\n",
            index + 1,
            hit.path,
            hit.score
        ));
        if !hit.matched_terms.is_empty() {
            output.push_str(&format!("   matched: {}\n", hit.matched_terms.join(", ")));
        }
        if !hit.symbols.is_empty() {
            let symbols = hit
                .symbols
                .iter()
                .take(8)
                .map(|symbol| format!("{:?} {}@{}", symbol.kind, symbol.name, symbol.line))
                .collect::<Vec<_>>()
                .join(", ");
            output.push_str(&format!("   symbols: {symbols}\n"));
        }
        if !hit.imports.is_empty() {
            output.push_str(&format!(
                "   imports: {}\n",
                hit.imports
                    .iter()
                    .take(4)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(" | ")
            ));
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tool::Tool;

    #[tokio::test]
    async fn tool_builds_persistent_index_and_searches_symbols() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(
            dir.path().join("src/auth.rs"),
            "pub struct SessionStore {}\npub fn restore_session() {}\n",
        )
        .unwrap();
        let mut ctx = ToolContext::empty();
        ctx.working_dir = Some(dir.path().to_path_buf());

        let result = RepositorySearchTool
            .execute(json!({"query":"SessionStore restore_session"}), &ctx)
            .await
            .unwrap();
        assert!(!result.is_error);
        assert!(result.content.contains("src/auth.rs"));
        assert!(index_path(dir.path()).exists());
    }

    #[test]
    fn update_path_rejects_escape() {
        assert!(validate_relative_path("../secret").is_err());
        assert!(validate_relative_path("src/lib.rs").is_ok());
    }
}
