use std::path::PathBuf;
#[cfg(test)]
use std::sync::{LazyLock, Mutex};

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;

use crate::tool::{Tool, ToolCapabilities, ToolContext, ToolResult};

pub struct LspTool;

#[cfg(test)]
static LSP_TEST_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

#[async_trait]
impl Tool for LspTool {
    fn name(&self) -> &str {
        "lsp"
    }

    fn user_facing_name(&self) -> &str {
        "LSP"
    }

    fn activity_description(&self, params: &Value) -> String {
        let op = params
            .get("operation")
            .and_then(|v| v.as_str())
            .unwrap_or("query");
        let file = params
            .get("filePath")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        format!("LSP {}: {}", op, file)
    }

    fn description(&self) -> &str {
        "Interact with Language Server Protocol (LSP) servers for code intelligence. \
         Supports goToDefinition, findReferences, hover, and documentSymbol operations. \
         LSP servers are started on demand per language."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "operation": {
                    "type": "string",
                    "enum": ["goToDefinition", "findReferences", "hover", "documentSymbol"],
                    "description": "The LSP operation to perform"
                },
                "filePath": {
                    "type": "string",
                    "description": "Absolute path to the file"
                },
                "line": {
                    "type": "integer",
                    "description": "Line number (0-based)"
                },
                "character": {
                    "type": "integer",
                    "description": "Character offset (0-based)"
                }
            },
            "required": ["operation", "filePath", "line", "character"]
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
        let operation = params
            .get("operation")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let file_path = params
            .get("filePath")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let line = params.get("line").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
        let character = params
            .get("character")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32;

        if operation.is_empty() || file_path.is_empty() {
            return Ok(ToolResult::error(
                "operation and filePath are required".to_string(),
            ));
        }

        let lsp_mgr = ctx
            .lsp_manager
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("LSP manager not available"))?;

        let path = PathBuf::from(file_path);
        let mut mgr = lsp_mgr.lock().await;

        match mgr.execute(operation, &path, line, character).await {
            Ok(result) => {
                let formatted =
                    serde_json::to_string_pretty(&result).unwrap_or_else(|_| result.to_string());
                let metadata = serde_json::json!({
                    "operation": operation,
                    "file_path": file_path,
                    "line": line,
                    "character": character,
                });
                Ok(ToolResult::success_with_metadata(formatted, metadata))
            }
            Err(e) => Ok(ToolResult::error(format!("LSP operation failed: {}", e))),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::Arc;

    use serde_json::json;
    use tokio::sync::Mutex;

    use crate::lsp_manager::LspManager;
    use crate::tool::{Tool, ToolContext};

    use super::{LspTool, LSP_TEST_LOCK};

    fn write_fake_pyright(dir: &tempfile::TempDir) -> std::path::PathBuf {
        let path = dir.path().join("pyright-langserver");
        fs::write(
            &path,
            r#"#!/usr/bin/env python3
import json, sys

def read_message():
    content_length = 0
    while True:
        line = sys.stdin.buffer.readline()
        if not line:
            return None
        if line in (b"\r\n", b"\n"):
            break
        if line.lower().startswith(b"content-length:"):
            content_length = int(line.split(b":", 1)[1].strip())
    if content_length <= 0:
        return None
    body = sys.stdin.buffer.read(content_length)
    if not body:
        return None
    return json.loads(body.decode("utf-8"))

def send(obj):
    body = json.dumps(obj).encode("utf-8")
    sys.stdout.buffer.write(f"Content-Length: {len(body)}\r\n\r\n".encode("utf-8"))
    sys.stdout.buffer.write(body)
    sys.stdout.buffer.flush()

while True:
    msg = read_message()
    if msg is None:
        break
    method = msg.get("method")
    if method == "initialize":
        send({"jsonrpc":"2.0","id":msg["id"],"result":{"capabilities":{}}})
    elif method == "textDocument/hover":
        send({"jsonrpc":"2.0","id":msg["id"],"result":{"contents":"hover info"}})
"#,
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&path).unwrap().permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&path, perms).unwrap();
        }
        path
    }

    #[tokio::test]
    async fn lsp_tool_requires_manager() {
        let result = LspTool
            .execute(
                json!({
                    "operation": "hover",
                    "filePath": "/tmp/test.py",
                    "line": 0,
                    "character": 0
                }),
                &ToolContext::empty(),
            )
            .await;

        assert!(result.is_err());
        assert!(format!("{}", result.unwrap_err()).contains("LSP manager not available"));
    }

    #[tokio::test]
    async fn lsp_tool_executes_hover_against_fake_server() {
        let _guard = LSP_TEST_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let fake = write_fake_pyright(&dir);
        let old_path = std::env::var_os("PATH");
        let new_path = std::env::join_paths(
            std::iter::once(dir.path().to_path_buf()).chain(
                old_path
                    .as_ref()
                    .map(std::env::split_paths)
                    .into_iter()
                    .flatten(),
            ),
        )
        .unwrap();
        std::env::set_var("PATH", &new_path);

        let file = dir.path().join("main.py");
        tokio::fs::write(&file, "print('hi')\n").await.unwrap();

        let mut ctx = ToolContext::empty();
        ctx.lsp_manager = Some(Arc::new(Mutex::new(LspManager::new(
            dir.path().to_path_buf(),
        ))));

        let result = LspTool
            .execute(
                json!({
                    "operation": "hover",
                    "filePath": file.display().to_string(),
                    "line": 0,
                    "character": 0
                }),
                &ctx,
            )
            .await
            .unwrap();

        assert!(!result.is_error);
        assert!(result.content.contains("hover info"));
        assert_eq!(result.metadata.as_ref().unwrap()["operation"], json!("hover"));

        if let Some(path) = old_path {
            std::env::set_var("PATH", path);
        } else {
            std::env::remove_var("PATH");
        }
        let _ = fake;
    }
}
