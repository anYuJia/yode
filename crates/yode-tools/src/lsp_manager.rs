use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use anyhow::{Context, Result};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout};
use tracing::{debug, info};

/// An active LSP server process.
struct LspServer {
    _process: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    initialized: bool,
    request_id: i64,
}

/// Manages LSP server instances per language.
pub struct LspManager {
    servers: HashMap<String, LspServer>,
    workspace_root: PathBuf,
}

/// Send a JSON-RPC request on the given server and read the response.
async fn send_request(server: &mut LspServer, method: &str, params: Value) -> Result<Value> {
    server.request_id += 1;
    let id = server.request_id;

    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": method,
        "params": params
    });

    let body = serde_json::to_string(&request)?;
    let header = format!("Content-Length: {}\r\n\r\n", body.len());

    debug!(method = %method, id = id, "LSP request");

    server.stdin.write_all(header.as_bytes()).await?;
    server.stdin.write_all(body.as_bytes()).await?;
    server.stdin.flush().await?;

    read_response(server, id).await
}

/// Send a JSON-RPC notification (no response expected).
async fn send_notification(server: &mut LspServer, method: &str, params: Value) -> Result<()> {
    let notification = serde_json::json!({
        "jsonrpc": "2.0",
        "method": method,
        "params": params
    });

    let body = serde_json::to_string(&notification)?;
    let header = format!("Content-Length: {}\r\n\r\n", body.len());

    server.stdin.write_all(header.as_bytes()).await?;
    server.stdin.write_all(body.as_bytes()).await?;
    server.stdin.flush().await?;

    Ok(())
}

/// Read a JSON-RPC response, skipping notifications until we find our response.
async fn read_response(server: &mut LspServer, expected_id: i64) -> Result<Value> {
    loop {
        let mut content_length: usize = 0;
        loop {
            let mut line = String::new();
            server.stdout.read_line(&mut line).await?;
            let line = line.trim().to_string();
            if line.is_empty() {
                break;
            }
            if let Some(len_str) = line.strip_prefix("Content-Length: ") {
                content_length = len_str.parse()?;
            }
        }

        if content_length == 0 {
            return Err(anyhow::anyhow!("Empty LSP response"));
        }

        let mut buf = vec![0u8; content_length];
        server.stdout.read_exact(&mut buf).await?;
        let response: Value = serde_json::from_slice(&buf)?;

        if let Some(id) = response.get("id").and_then(|v| v.as_i64()) {
            if id == expected_id {
                if let Some(error) = response.get("error") {
                    return Err(anyhow::anyhow!("LSP error: {}", error));
                }
                return Ok(response.get("result").cloned().unwrap_or(Value::Null));
            }
        }
        debug!("Skipping LSP message (waiting for id {})", expected_id);
    }
}

impl LspManager {
    pub fn new(workspace_root: PathBuf) -> Self {
        Self {
            servers: HashMap::new(),
            workspace_root,
        }
    }

    /// Map file extension to language server command.
    fn server_command(ext: &str) -> Option<(&'static str, Vec<&'static str>)> {
        match ext {
            "rs" => Some(("rust-analyzer", vec![])),
            "ts" | "tsx" | "js" | "jsx" => Some(("typescript-language-server", vec!["--stdio"])),
            "py" => Some(("pyright-langserver", vec!["--stdio"])),
            "go" => Some(("gopls", vec!["serve"])),
            "java" => Some(("jdtls", vec![])),
            _ => None,
        }
    }

    /// Get the language key for a file extension.
    fn lang_key(ext: &str) -> &str {
        match ext {
            "ts" | "tsx" | "js" | "jsx" => "typescript",
            "rs" => "rust",
            "py" => "python",
            "go" => "go",
            "java" => "java",
            other => other,
        }
    }

    /// Ensure the LSP server for the given file is running.
    async fn ensure_server(&mut self, file_path: &Path) -> Result<String> {
        let ext = file_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        let lang = Self::lang_key(ext).to_string();

        if !self.servers.contains_key(&lang) {
            let (cmd, args) = Self::server_command(ext)
                .ok_or_else(|| anyhow::anyhow!("No LSP server configured for .{} files", ext))?;

            info!(language = %lang, command = %cmd, "Starting LSP server");

            let mut process = tokio::process::Command::new(cmd)
                .args(&args)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .kill_on_drop(true)
                .spawn()
                .with_context(|| format!("Failed to start LSP server: {}", cmd))?;

            let stdin = process.stdin.take().unwrap();
            let stdout = BufReader::new(process.stdout.take().unwrap());

            let mut server = LspServer {
                _process: process,
                stdin,
                stdout,
                initialized: false,
                request_id: 0,
            };

            // Send initialize request
            let init_params = serde_json::json!({
                "processId": std::process::id(),
                "rootUri": format!("file://{}", self.workspace_root.display()),
                "capabilities": {
                    "textDocument": {
                        "definition": { "dynamicRegistration": false },
                        "references": { "dynamicRegistration": false },
                        "hover": { "dynamicRegistration": false, "contentFormat": ["plaintext", "markdown"] },
                        "documentSymbol": { "dynamicRegistration": false }
                    }
                }
            });
            let _result = send_request(&mut server, "initialize", init_params).await?;
            send_notification(&mut server, "initialized", serde_json::json!({})).await?;
            server.initialized = true;

            self.servers.insert(lang.clone(), server);
        }

        Ok(lang)
    }

    /// Execute an LSP operation.
    pub async fn execute(
        &mut self,
        operation: &str,
        file_path: &Path,
        line: u32,
        character: u32,
    ) -> Result<Value> {
        let lang = self.ensure_server(file_path).await?;
        let uri = format!("file://{}", file_path.display());

        let server = self.servers.get_mut(&lang)
            .ok_or_else(|| anyhow::anyhow!("LSP server not running for {}", lang))?;

        match operation {
            "goToDefinition" => {
                let params = serde_json::json!({
                    "textDocument": { "uri": uri },
                    "position": { "line": line, "character": character }
                });
                send_request(server, "textDocument/definition", params).await
            }
            "findReferences" => {
                let params = serde_json::json!({
                    "textDocument": { "uri": uri },
                    "position": { "line": line, "character": character },
                    "context": { "includeDeclaration": true }
                });
                send_request(server, "textDocument/references", params).await
            }
            "hover" => {
                let params = serde_json::json!({
                    "textDocument": { "uri": uri },
                    "position": { "line": line, "character": character }
                });
                send_request(server, "textDocument/hover", params).await
            }
            "documentSymbol" => {
                let params = serde_json::json!({
                    "textDocument": { "uri": uri }
                });
                send_request(server, "textDocument/documentSymbol", params).await
            }
            "diagnostics" => {
                Ok(serde_json::json!({ "message": "Diagnostics are push-based. Use hover or references instead." }))
            }
            _ => Err(anyhow::anyhow!("Unknown LSP operation: {}", operation)),
        }
    }

    /// Shutdown all running LSP servers.
    pub async fn shutdown_all(&mut self) {
        let keys: Vec<String> = self.servers.keys().cloned().collect();
        for lang in keys {
            if let Some(mut server) = self.servers.remove(&lang) {
                info!(language = %lang, "Shutting down LSP server");
                let _ = send_notification(&mut server, "shutdown", Value::Null).await;
            }
        }
    }
}
