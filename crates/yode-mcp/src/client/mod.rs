mod tool_wrapper;
use self::tool_wrapper::{annotations_from_mcp, wrapper_tool_name};

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, LazyLock, Mutex};

use anyhow::Result;
use rmcp::handler::client::ClientHandler;
use rmcp::model::{
    CallToolRequestParams, ClientCapabilities, ClientInfo, CreateElicitationRequestParams,
    CreateElicitationResult, ElicitationAction, ElicitationCapability, ErrorData as McpError,
    FormElicitationCapability, Implementation, ListResourceTemplatesResult, ListResourcesResult,
    ListToolsResult, ReadResourceRequestParams, ReadResourceResult, ResourceContents,
    UrlElicitationCapability,
};
use rmcp::service::{Peer, RequestContext, RoleClient, RunningService, ServiceExt};
use rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig;
use rmcp::transport::{ConfigureCommandExt, StreamableHttpClientTransport, TokioChildProcess};
use serde::Deserialize;
use tokio::process::Command;
use tokio::sync::Mutex as AsyncMutex;
use tracing::{info, warn};

use crate::config::{McpServerConfig, McpTransportConfig};
use yode_tools::registry::ToolRegistry;
use yode_tools::tool::{
    McpResource, McpResourceBlob, McpResourceProvider, McpResourceRead, McpResourceTemplate, Tool,
};

pub use tool_wrapper::{mcp_tool_latency_stats, McpToolLatencyEntry, McpToolWrapper};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct McpReconnectDiagnostic {
    pub server: String,
    pub attempts: u64,
    pub failures: u64,
    pub last_error: Option<String>,
    pub next_backoff_secs: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct McpElicitationDiagnostic {
    pub server: String,
    pub requests: u64,
    pub form_requests: u64,
    pub url_requests: u64,
    pub declined: u64,
    pub last_message: Option<String>,
    pub last_url: Option<String>,
}

static MCP_RECONNECT_DIAGNOSTICS: LazyLock<Mutex<BTreeMap<String, McpReconnectDiagnostic>>> =
    LazyLock::new(|| Mutex::new(BTreeMap::new()));
static MCP_ELICITATION_DIAGNOSTICS: LazyLock<Mutex<BTreeMap<String, McpElicitationDiagnostic>>> =
    LazyLock::new(|| Mutex::new(BTreeMap::new()));

#[derive(Debug, Deserialize)]
struct StoredMcpOAuthToken {
    access_token: String,
}

pub fn mcp_reconnect_diagnostics() -> Vec<McpReconnectDiagnostic> {
    MCP_RECONNECT_DIAGNOSTICS
        .lock()
        .map(|state| state.values().cloned().collect())
        .unwrap_or_default()
}

pub fn mcp_elicitation_diagnostics() -> Vec<McpElicitationDiagnostic> {
    MCP_ELICITATION_DIAGNOSTICS
        .lock()
        .map(|state| state.values().cloned().collect())
        .unwrap_or_default()
}

fn reconnect_backoff_secs(failure_count: u64) -> u64 {
    match failure_count {
        0 => 0,
        1 => 1,
        2 => 2,
        3 => 4,
        4 => 8,
        _ => 16,
    }
}

fn record_mcp_connect_result(server: &str, success: bool, error: Option<String>) {
    if let Ok(mut state) = MCP_RECONNECT_DIAGNOSTICS.lock() {
        let entry = state
            .entry(server.to_string())
            .or_insert_with(|| McpReconnectDiagnostic {
                server: server.to_string(),
                ..McpReconnectDiagnostic::default()
            });
        entry.attempts = entry.attempts.saturating_add(1);
        if success {
            entry.last_error = None;
            entry.next_backoff_secs = 0;
        } else {
            entry.failures = entry.failures.saturating_add(1);
            entry.last_error = error;
            entry.next_backoff_secs = reconnect_backoff_secs(entry.failures);
        }
    }
}

fn record_mcp_elicitation_declined(
    server: &str,
    kind: ElicitationRequestKind,
    message: String,
    url: Option<String>,
) {
    if let Ok(mut state) = MCP_ELICITATION_DIAGNOSTICS.lock() {
        let entry = state
            .entry(server.to_string())
            .or_insert_with(|| McpElicitationDiagnostic {
                server: server.to_string(),
                ..McpElicitationDiagnostic::default()
            });
        entry.requests = entry.requests.saturating_add(1);
        entry.declined = entry.declined.saturating_add(1);
        match kind {
            ElicitationRequestKind::Form => {
                entry.form_requests = entry.form_requests.saturating_add(1)
            }
            ElicitationRequestKind::Url => {
                entry.url_requests = entry.url_requests.saturating_add(1)
            }
        }
        entry.last_message = Some(message);
        entry.last_url = url;
    }
}

#[derive(Debug, Clone, Copy)]
enum ElicitationRequestKind {
    Form,
    Url,
}

#[derive(Debug, Clone)]
struct YodeMcpClientHandler {
    server_name: String,
}

impl YodeMcpClientHandler {
    fn new(server_name: impl Into<String>) -> Self {
        Self {
            server_name: server_name.into(),
        }
    }
}

impl ClientHandler for YodeMcpClientHandler {
    fn create_elicitation(
        &self,
        request: CreateElicitationRequestParams,
        _context: RequestContext<RoleClient>,
    ) -> impl std::future::Future<Output = Result<CreateElicitationResult, McpError>> + Send + '_
    {
        let (kind, message, url) = match request {
            CreateElicitationRequestParams::FormElicitationParams { message, .. } => {
                (ElicitationRequestKind::Form, message, None)
            }
            CreateElicitationRequestParams::UrlElicitationParams { message, url, .. } => {
                (ElicitationRequestKind::Url, message, Some(url))
            }
        };
        record_mcp_elicitation_declined(&self.server_name, kind, message, url);
        std::future::ready(Ok(CreateElicitationResult {
            action: ElicitationAction::Decline,
            content: None,
        }))
    }

    fn get_info(&self) -> ClientInfo {
        let mut capabilities = ClientCapabilities::default();
        capabilities.elicitation = Some(ElicitationCapability {
            form: Some(FormElicitationCapability {
                schema_validation: Some(false),
            }),
            url: Some(UrlElicitationCapability::default()),
        });

        let mut info = ClientInfo::default();
        info.capabilities = capabilities;
        info.client_info =
            Implementation::new("yode", env!("CARGO_PKG_VERSION")).with_title("Yode");
        info
    }
}

/// A connected MCP client managing one external server.
#[derive(Clone)]
pub struct McpClient {
    pub server_name: String,
    connection: McpConnection,
}

/// Shared MCP resource provider backed by connected MCP clients.
#[derive(Clone)]
pub struct McpClientResourceProvider {
    clients: Arc<Vec<McpClient>>,
}

impl McpClientResourceProvider {
    pub fn new(clients: Vec<McpClient>) -> Self {
        Self {
            clients: Arc::new(clients),
        }
    }

    fn client_for(&self, server: &str) -> Option<McpClient> {
        self.clients
            .iter()
            .find(|client| client.server_name == server)
            .cloned()
    }
}

impl McpResourceProvider for McpClientResourceProvider {
    fn list_resources(
        &self,
        server: Option<&str>,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<Vec<McpResource>>> + Send + '_>> {
        let clients = self.clients.clone();
        let server = server.map(str::to_string);
        Box::pin(async move {
            let mut resources = Vec::new();
            for client in clients.iter() {
                if server
                    .as_ref()
                    .is_some_and(|server| server != &client.server_name)
                {
                    continue;
                }
                for (name, uri, description) in client.list_resources().await? {
                    resources.push(McpResource {
                        server: client.server_name.clone(),
                        uri,
                        name,
                        description,
                    });
                }
            }
            Ok(resources)
        })
    }

    fn list_resource_templates(
        &self,
        server: Option<&str>,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<Vec<McpResourceTemplate>>> + Send + '_>>
    {
        let clients = self.clients.clone();
        let server = server.map(str::to_string);
        Box::pin(async move {
            let mut templates = Vec::new();
            for client in clients.iter() {
                if server
                    .as_ref()
                    .is_some_and(|server| server != &client.server_name)
                {
                    continue;
                }
                for (name, uri_template, description, mime_type) in
                    client.list_resource_templates().await?
                {
                    templates.push(McpResourceTemplate {
                        server: client.server_name.clone(),
                        uri_template,
                        name,
                        description,
                        mime_type,
                    });
                }
            }
            Ok(templates)
        })
    }

    fn read_resource(
        &self,
        server: &str,
        uri: &str,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<McpResourceRead>> + Send + '_>> {
        let client = self.client_for(server);
        let server = server.to_string();
        let uri = uri.to_string();
        Box::pin(async move {
            let Some(client) = client else {
                anyhow::bail!("MCP server '{}' is not connected", server);
            };
            client.read_resource(&uri).await
        })
    }
}

struct McpConnectionState {
    peer: Peer<RoleClient>,
    service: RunningService<RoleClient, YodeMcpClientHandler>,
}

#[derive(Clone)]
pub(crate) struct McpConnection {
    server_name: String,
    config: McpServerConfig,
    state: Arc<AsyncMutex<McpConnectionState>>,
}

impl McpClient {
    /// Connect to an MCP server via stdio transport.
    pub async fn connect(name: &str, config: &McpServerConfig) -> Result<Self> {
        if config.disabled {
            let message = format!("MCP server '{}' is disabled by config", name);
            record_mcp_connect_result(name, false, Some(message.clone()));
            return Err(anyhow::anyhow!(message));
        }
        info!(
            server = %name,
            transport = %config.transport.label(),
            command = %config.command,
            url = ?config.url,
            "Connecting to MCP server"
        );

        let state = start_mcp_service(name, config).await?;

        let peer_info = state.service.peer_info();
        if let Some(info) = peer_info {
            info!(
                server = %name,
                server_name = %info.server_info.name,
                "MCP server connected"
            );
        } else {
            info!(server = %name, "MCP server connected (no peer info)");
        }

        Ok(Self {
            server_name: name.to_string(),
            connection: McpConnection {
                server_name: name.to_string(),
                config: config.clone(),
                state: Arc::new(AsyncMutex::new(state)),
            },
        })
    }

    /// Discover tools from the connected server and register them as wrapped Tool implementations.
    pub async fn discover_and_register(&self, registry: &mut ToolRegistry) -> Result<usize> {
        let wrappers = self.discover_wrapped_tools().await?;
        let count = wrappers.len();
        for wrapper in wrappers {
            registry.register(wrapper);
        }
        Ok(count)
    }

    pub async fn discover_wrapped_tools(&self) -> Result<Vec<Arc<dyn Tool>>> {
        let tools_result = self.connection.list_tools().await?;
        let tools = tools_result.tools;
        let count = tools.len();

        info!(
            server = %self.server_name,
            tool_count = count,
            "Discovered MCP tools"
        );

        let mut wrappers: Vec<Arc<dyn Tool>> = Vec::with_capacity(count);
        for tool in tools {
            let wrapper = McpToolWrapper {
                tool_name: wrapper_tool_name(&self.server_name, &tool.name),
                original_name: tool.name.to_string(),
                description: tool
                    .description
                    .clone()
                    .map(|c| c.to_string())
                    .unwrap_or_default(),
                input_schema: mcp_input_schema_to_value(
                    &self.server_name,
                    &tool.name,
                    &tool.input_schema,
                ),
                annotations: annotations_from_mcp(tool.annotations.as_ref()),
                server_name: self.server_name.clone(),
                connection: self.connection.clone(),
            };
            wrappers.push(Arc::new(wrapper));
        }

        Ok(wrappers)
    }

    /// Call a tool on this MCP server.
    pub async fn call_tool(&self, tool_name: &str, arguments: serde_json::Value) -> Result<String> {
        let mut request = CallToolRequestParams::new(tool_name.to_string());
        if let Some(obj) = arguments.as_object() {
            request = request.with_arguments(obj.clone());
        }

        let result = self.connection.call_tool(request).await?;

        // Extract text content from the result
        let mut output = String::new();
        for content in &result.content {
            if let Some(text) = content.as_text() {
                if !output.is_empty() {
                    output.push('\n');
                }
                output.push_str(&text.text);
            }
        }

        Ok(output)
    }

    /// Gracefully shut down the connection.
    pub async fn shutdown(self) -> Result<()> {
        info!(server = %self.server_name, "Shutting down MCP client");
        let mut state = self.connection.state.lock().await;
        state.service.close().await?;
        Ok(())
    }

    /// List resources available on this MCP server.
    pub async fn list_resources(&self) -> Result<Vec<(String, String, Option<String>)>> {
        let result = self.connection.list_resources().await?;
        let resources = result
            .resources
            .iter()
            .map(|r| {
                let name = r.name.clone();
                let uri = r.uri.clone();
                let description = r.description.clone();
                (name, uri, description)
            })
            .collect();
        Ok(resources)
    }

    /// List resource templates available on this MCP server.
    pub async fn list_resource_templates(
        &self,
    ) -> Result<Vec<(String, String, Option<String>, Option<String>)>> {
        let result = self.connection.list_resource_templates().await?;
        let templates = result
            .resource_templates
            .iter()
            .map(|template| {
                let name = template.name.clone();
                let uri_template = template.uri_template.clone();
                let description = template.description.clone();
                let mime_type = template.mime_type.clone();
                (name, uri_template, description, mime_type)
            })
            .collect();
        Ok(templates)
    }

    /// Read a specific resource by URI.
    pub async fn read_resource(&self, uri: &str) -> Result<McpResourceRead> {
        let params = ReadResourceRequestParams::new(uri);
        let result = self.connection.read_resource(params).await?;
        Ok(render_resource_contents(&result.contents))
    }
}

fn render_resource_contents(contents: &[ResourceContents]) -> McpResourceRead {
    let mut output = String::new();
    let mut blobs = Vec::new();
    for content in contents {
        if !output.is_empty() {
            output.push('\n');
        }
        match content {
            ResourceContents::TextResourceContents { text, .. } => {
                output.push_str(text);
            }
            ResourceContents::BlobResourceContents {
                uri,
                mime_type,
                blob,
                ..
            } => {
                let mime = mime_type.as_deref().unwrap_or("application/octet-stream");
                let approx_bytes = approx_base64_decoded_len(blob);
                blobs.push(McpResourceBlob {
                    uri: uri.clone(),
                    mime_type: mime.to_string(),
                    base64: blob.clone(),
                    approx_bytes,
                });
                output.push_str(&format!(
                    "[binary resource: uri={} mime={} base64_chars={} approx_bytes={} data_uri_prefix={}]",
                    uri,
                    mime,
                    blob.len(),
                    approx_bytes,
                    data_uri_prefix(mime, blob)
                ));
            }
        }
    }
    McpResourceRead {
        content: output,
        blobs,
    }
}

fn approx_base64_decoded_len(value: &str) -> usize {
    let compact_len = value.chars().filter(|ch| !ch.is_ascii_whitespace()).count();
    if compact_len == 0 {
        return 0;
    }
    let padding = value
        .chars()
        .rev()
        .skip_while(|ch| ch.is_ascii_whitespace())
        .take_while(|ch| *ch == '=')
        .count()
        .min(2);
    compact_len
        .saturating_mul(3)
        .saturating_div(4)
        .saturating_sub(padding)
}

fn data_uri_prefix(mime: &str, blob: &str) -> String {
    let preview = blob
        .chars()
        .filter(|ch| !ch.is_ascii_whitespace())
        .take(48)
        .collect::<String>();
    let suffix = if blob.chars().filter(|ch| !ch.is_ascii_whitespace()).count() > 48 {
        "..."
    } else {
        ""
    };
    format!("data:{};base64,{}{}", mime, preview, suffix)
}

impl McpConnection {
    async fn current_peer(&self) -> Peer<RoleClient> {
        self.state.lock().await.peer.clone()
    }

    pub(crate) async fn list_tools(&self) -> Result<ListToolsResult> {
        let peer = self.current_peer().await;
        match peer.list_tools(Default::default()).await {
            Ok(result) => Ok(result),
            Err(err) => {
                warn!(
                    server = %self.server_name,
                    error = %err,
                    "MCP tool discovery failed; reconnecting server and retrying once"
                );
                record_mcp_connect_result(&self.server_name, false, Some(err.to_string()));
                let peer = self.reconnect().await?;
                Ok(peer.list_tools(Default::default()).await?)
            }
        }
    }

    pub(crate) async fn call_tool(
        &self,
        request: CallToolRequestParams,
    ) -> Result<rmcp::model::CallToolResult> {
        let peer = self.current_peer().await;
        match peer.call_tool(request.clone()).await {
            Ok(result) => Ok(result),
            Err(err) => {
                warn!(
                    server = %self.server_name,
                    error = %err,
                    "MCP tool call failed; reconnecting server and retrying once"
                );
                record_mcp_connect_result(&self.server_name, false, Some(err.to_string()));
                let peer = self.reconnect().await?;
                Ok(peer.call_tool(request).await?)
            }
        }
    }

    pub(crate) async fn list_resources(&self) -> Result<ListResourcesResult> {
        let peer = self.current_peer().await;
        match peer.list_resources(Default::default()).await {
            Ok(result) => Ok(result),
            Err(err) => {
                warn!(
                    server = %self.server_name,
                    error = %err,
                    "MCP resource discovery failed; reconnecting server and retrying once"
                );
                record_mcp_connect_result(&self.server_name, false, Some(err.to_string()));
                let peer = self.reconnect().await?;
                Ok(peer.list_resources(Default::default()).await?)
            }
        }
    }

    pub(crate) async fn list_resource_templates(&self) -> Result<ListResourceTemplatesResult> {
        let peer = self.current_peer().await;
        match peer.list_resource_templates(Default::default()).await {
            Ok(result) => Ok(result),
            Err(err) => {
                warn!(
                    server = %self.server_name,
                    error = %err,
                    "MCP resource template discovery failed; reconnecting server and retrying once"
                );
                record_mcp_connect_result(&self.server_name, false, Some(err.to_string()));
                let peer = self.reconnect().await?;
                Ok(peer.list_resource_templates(Default::default()).await?)
            }
        }
    }

    pub(crate) async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
    ) -> Result<ReadResourceResult> {
        let peer = self.current_peer().await;
        match peer.read_resource(request.clone()).await {
            Ok(result) => Ok(result),
            Err(err) => {
                warn!(
                    server = %self.server_name,
                    error = %err,
                    "MCP resource read failed; reconnecting server and retrying once"
                );
                record_mcp_connect_result(&self.server_name, false, Some(err.to_string()));
                let peer = self.reconnect().await?;
                Ok(peer.read_resource(request).await?)
            }
        }
    }

    async fn reconnect(&self) -> Result<Peer<RoleClient>> {
        let new_state = start_mcp_service(&self.server_name, &self.config).await?;
        let new_peer = new_state.peer.clone();
        let mut state = self.state.lock().await;
        let mut old_service = std::mem::replace(&mut state.service, new_state.service);
        state.peer = new_peer.clone();
        drop(state);
        if let Err(err) = old_service.close().await {
            warn!(
                server = %self.server_name,
                error = %err,
                "failed to close stale MCP service after reconnect"
            );
        }
        Ok(new_peer)
    }
}

async fn start_mcp_service(name: &str, config: &McpServerConfig) -> Result<McpConnectionState> {
    match config.transport {
        McpTransportConfig::Stdio => start_stdio_mcp_service(name, config).await,
        McpTransportConfig::Http | McpTransportConfig::Sse => {
            start_streamable_http_mcp_service(name, config).await
        }
        McpTransportConfig::Websocket => {
            let message = format!(
                "MCP transport '{}' for server '{}' is parsed but not yet executable; configure stdio/http/sse",
                config.transport.label(),
                name
            );
            record_mcp_connect_result(name, false, Some(message.clone()));
            Err(anyhow::anyhow!(message))
        }
    }
}

async fn start_stdio_mcp_service(
    name: &str,
    config: &McpServerConfig,
) -> Result<McpConnectionState> {
    if config.command.trim().is_empty() {
        let message = format!("MCP stdio server '{}' is missing command", name);
        record_mcp_connect_result(name, false, Some(message.clone()));
        return Err(anyhow::anyhow!(message));
    }

    let env_vars: HashMap<String, String> = config
        .env
        .iter()
        .map(|(k, v)| {
            // Expand $ENV_VAR references in values.
            let expanded = if let Some(stripped) = v.strip_prefix('$') {
                std::env::var(stripped).unwrap_or_default()
            } else {
                v.clone()
            };
            (k.clone(), expanded)
        })
        .collect();

    let args = config.args.clone();
    let command = config.command.clone();
    let service = match YodeMcpClientHandler::new(name)
        .serve(TokioChildProcess::new(Command::new(&command).configure(
            |cmd| {
                cmd.args(&args);
                for (k, v) in &env_vars {
                    cmd.env(k, v);
                }
            },
        ))?)
        .await
    {
        Ok(service) => {
            record_mcp_connect_result(name, true, None);
            service
        }
        Err(err) => {
            record_mcp_connect_result(name, false, Some(err.to_string()));
            return Err(err.into());
        }
    };
    let peer = service.peer().clone();
    Ok(McpConnectionState { peer, service })
}

async fn start_streamable_http_mcp_service(
    name: &str,
    config: &McpServerConfig,
) -> Result<McpConnectionState> {
    let Some(url) = config.url.as_ref().filter(|url| !url.trim().is_empty()) else {
        let message = format!(
            "MCP {} server '{}' is missing url",
            config.transport.label(),
            name
        );
        record_mcp_connect_result(name, false, Some(message.clone()));
        return Err(anyhow::anyhow!(message));
    };

    let mut transport_config = StreamableHttpClientTransportConfig::with_uri(url.clone());
    if let Some(token) = remote_auth_token(name, config)? {
        transport_config = transport_config.auth_header(token);
    }

    let service = match YodeMcpClientHandler::new(name)
        .serve(StreamableHttpClientTransport::from_config(transport_config))
        .await
    {
        Ok(service) => {
            record_mcp_connect_result(name, true, None);
            service
        }
        Err(err) => {
            record_mcp_connect_result(name, false, Some(err.to_string()));
            return Err(err.into());
        }
    };
    let peer = service.peer().clone();
    Ok(McpConnectionState { peer, service })
}

fn remote_auth_token(name: &str, config: &McpServerConfig) -> Result<Option<String>> {
    if let Some(token_env) = config
        .auth
        .as_ref()
        .and_then(|auth| auth.bearer_token_env.as_ref())
    {
        let token = std::env::var(token_env).map_err(|err| {
            let message = format!(
                "MCP {} server '{}' bearer token env '{}' is unavailable: {}",
                config.transport.label(),
                name,
                token_env,
                err
            );
            record_mcp_connect_result(name, false, Some(message.clone()));
            anyhow::anyhow!(message)
        })?;
        return Ok(Some(token));
    }

    if config
        .auth
        .as_ref()
        .and_then(|auth| auth.oauth.as_ref())
        .is_some()
    {
        return Ok(load_oauth_access_token(name));
    }

    Ok(None)
}

fn load_oauth_access_token(server: &str) -> Option<String> {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    load_oauth_access_token_from_home(&home, server)
}

fn load_oauth_access_token_from_home(home: &Path, server: &str) -> Option<String> {
    let path = home
        .join(".yode")
        .join("mcp-auth")
        .join(format!("{}.token.json", sanitize_server_name(server)));
    let content = match std::fs::read_to_string(&path) {
        Ok(content) => content,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return None,
        Err(err) => {
            warn!(
                server = %server,
                path = %path.display(),
                error = %err,
                "Failed to read MCP OAuth token file"
            );
            return None;
        }
    };
    let token: StoredMcpOAuthToken = match serde_json::from_str(&content) {
        Ok(token) => token,
        Err(err) => {
            warn!(
                server = %server,
                path = %path.display(),
                error = %err,
                "Failed to parse MCP OAuth token file"
            );
            return None;
        }
    };
    (!token.access_token.trim().is_empty()).then_some(token.access_token)
}

fn mcp_input_schema_to_value(
    server: &str,
    tool_name: &str,
    input_schema: &Arc<rmcp::model::JsonObject>,
) -> serde_json::Value {
    match serde_json::to_value(input_schema) {
        Ok(value) => value,
        Err(err) => {
            warn!(
                server = %server,
                tool = %tool_name,
                error = %err,
                "Failed to serialize MCP tool input schema"
            );
            serde_json::json!({"type": "object"})
        }
    }
}

fn sanitize_server_name(server: &str) -> String {
    server
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use rmcp::model::Tool;
    use serde_json::json;

    use super::tool_wrapper::wrapper_tool_name;
    use super::{
        approx_base64_decoded_len, data_uri_prefix, load_oauth_access_token_from_home,
        mcp_elicitation_diagnostics, mcp_input_schema_to_value, mcp_reconnect_diagnostics,
        record_mcp_connect_result, record_mcp_elicitation_declined, render_resource_contents,
        ElicitationRequestKind,
    };
    use crate::config::{McpServerConfig, McpTransportConfig};
    use rmcp::model::ResourceContents;
    use std::fs;

    #[test]
    fn reconnect_diagnostics_track_failures_and_backoff() {
        let server = "diagnostic-track-test";
        record_mcp_connect_result(server, false, Some("timeout".to_string()));
        record_mcp_connect_result(server, false, Some("timeout".to_string()));
        record_mcp_connect_result(server, true, None);

        let stats = mcp_reconnect_diagnostics();
        let diagnostic = stats.iter().find(|stat| stat.server == server).unwrap();
        assert_eq!(diagnostic.attempts, 3);
        assert_eq!(diagnostic.failures, 2);
        assert_eq!(diagnostic.next_backoff_secs, 0);
        assert_eq!(diagnostic.last_error, None);
    }

    #[test]
    fn elicitation_diagnostics_track_declined_requests() {
        let server = "elicitation-diagnostic-test";
        record_mcp_elicitation_declined(
            server,
            ElicitationRequestKind::Form,
            "Provide an API token".to_string(),
            None,
        );
        record_mcp_elicitation_declined(
            server,
            ElicitationRequestKind::Url,
            "Authorize in browser".to_string(),
            Some("https://example.com/auth".to_string()),
        );

        let stats = mcp_elicitation_diagnostics();
        let diagnostic = stats.iter().find(|stat| stat.server == server).unwrap();
        assert_eq!(diagnostic.requests, 2);
        assert_eq!(diagnostic.declined, 2);
        assert_eq!(diagnostic.form_requests, 1);
        assert_eq!(diagnostic.url_requests, 1);
        assert_eq!(
            diagnostic.last_message.as_deref(),
            Some("Authorize in browser")
        );
        assert_eq!(
            diagnostic.last_url.as_deref(),
            Some("https://example.com/auth")
        );
    }

    #[tokio::test]
    async fn websocket_transport_reports_clear_unsupported_error() {
        let server = "ws-unsupported-test";
        let config = McpServerConfig {
            transport: McpTransportConfig::Websocket,
            url: Some("wss://example.com/mcp".to_string()),
            ..McpServerConfig::default()
        };

        let err = match super::McpClient::connect(server, &config).await {
            Ok(_) => panic!("websocket transport should not connect without implementation"),
            Err(err) => err,
        };

        assert!(err.to_string().contains("not yet executable"));
        let stats = mcp_reconnect_diagnostics();
        let diagnostic = stats.iter().find(|stat| stat.server == server).unwrap();
        assert_eq!(diagnostic.failures, 1);
    }

    #[tokio::test]
    async fn http_transport_requires_url() {
        let server = "docs-missing-url-test";
        let config = McpServerConfig {
            transport: McpTransportConfig::Http,
            ..McpServerConfig::default()
        };

        let err = match super::McpClient::connect(server, &config).await {
            Ok(_) => panic!("http transport should require a url"),
            Err(err) => err,
        };

        assert!(err.to_string().contains("missing url"));
        let stats = mcp_reconnect_diagnostics();
        let diagnostic = stats.iter().find(|stat| stat.server == server).unwrap();
        assert_eq!(diagnostic.failures, 1);
    }

    #[tokio::test]
    async fn http_transport_reports_missing_bearer_token_env() {
        let server = "secure-docs-missing-token-test";
        let config = McpServerConfig {
            transport: McpTransportConfig::Http,
            url: Some("https://example.com/mcp".to_string()),
            auth: Some(crate::config::McpAuthConfig {
                bearer_token_env: Some("YODE_TEST_MISSING_MCP_TOKEN".to_string()),
                ..crate::config::McpAuthConfig::default()
            }),
            ..McpServerConfig::default()
        };

        let err = match super::McpClient::connect(server, &config).await {
            Ok(_) => panic!("http transport should require configured bearer env"),
            Err(err) => err,
        };

        assert!(err.to_string().contains("YODE_TEST_MISSING_MCP_TOKEN"));
        let stats = mcp_reconnect_diagnostics();
        let diagnostic = stats.iter().find(|stat| stat.server == server).unwrap();
        assert_eq!(diagnostic.failures, 1);
    }

    #[test]
    fn wrapper_name_matches_discovery_shape() {
        let input_schema: Arc<rmcp::model::JsonObject> =
            serde_json::from_value(json!({"type":"object"})).unwrap();
        let tool = Tool::new("search_issues", "desc", input_schema);
        assert_eq!(
            wrapper_tool_name("github", &tool.name),
            "mcp__github_search_issues"
        );
    }

    #[test]
    fn load_oauth_access_token_reads_saved_session() {
        let tempdir = tempfile::tempdir().unwrap();
        let token_path = tempdir
            .path()
            .join(".yode")
            .join("mcp-auth")
            .join("github.token.json");
        fs::create_dir_all(token_path.parent().unwrap()).unwrap();
        fs::write(&token_path, r#"{"access_token":"secret-token"}"#).unwrap();

        let token = load_oauth_access_token_from_home(tempdir.path(), "github");
        assert_eq!(token.as_deref(), Some("secret-token"));
    }

    #[test]
    fn load_oauth_access_token_ignores_missing_or_invalid_session() {
        let tempdir = tempfile::tempdir().unwrap();
        assert_eq!(
            load_oauth_access_token_from_home(tempdir.path(), "github"),
            None
        );

        let token_path = tempdir
            .path()
            .join(".yode")
            .join("mcp-auth")
            .join("github.token.json");
        fs::create_dir_all(token_path.parent().unwrap()).unwrap();
        fs::write(&token_path, r#"{"access_token":"#).unwrap();

        assert_eq!(
            load_oauth_access_token_from_home(tempdir.path(), "github"),
            None
        );
    }

    #[test]
    fn mcp_input_schema_serializes_to_value() {
        let input_schema: Arc<rmcp::model::JsonObject> =
            serde_json::from_value(json!({"type":"object"})).unwrap();
        assert_eq!(
            mcp_input_schema_to_value("github", "search_issues", &input_schema),
            json!({"type":"object"})
        );
    }

    #[test]
    fn render_resource_contents_preserves_blob_metadata() {
        let rendered = render_resource_contents(&[
            ResourceContents::TextResourceContents {
                uri: "file://note.txt".to_string(),
                mime_type: Some("text/plain".to_string()),
                text: "hello".to_string(),
                meta: None,
            },
            ResourceContents::BlobResourceContents {
                uri: "file://image.png".to_string(),
                mime_type: Some("image/png".to_string()),
                blob: "ZmFrZQ==".to_string(),
                meta: None,
            },
        ]);

        assert!(rendered.content.contains("hello"));
        assert!(rendered.content.contains("uri=file://image.png"));
        assert!(rendered.content.contains("mime=image/png"));
        assert!(rendered.content.contains("base64_chars=8"));
        assert!(rendered.content.contains("approx_bytes=4"));
        assert!(rendered
            .content
            .contains("data_uri_prefix=data:image/png;base64,ZmFrZQ=="));
        assert_eq!(rendered.blobs.len(), 1);
        assert_eq!(rendered.blobs[0].uri, "file://image.png");
        assert_eq!(rendered.blobs[0].mime_type, "image/png");
        assert_eq!(rendered.blobs[0].base64, "ZmFrZQ==");
        assert_eq!(rendered.blobs[0].approx_bytes, 4);
    }

    #[test]
    fn approx_base64_decoded_len_handles_padding() {
        assert_eq!(approx_base64_decoded_len("ZmFrZQ=="), 4);
        assert_eq!(approx_base64_decoded_len(" Zm\nFr\r\nZQ== "), 4);
        assert_eq!(approx_base64_decoded_len(""), 0);
    }

    #[test]
    fn data_uri_prefix_is_budget_bounded() {
        let rendered = data_uri_prefix(
            "image/png",
            "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789",
        );

        assert_eq!(
            rendered,
            "data:image/png;base64,abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUV..."
        );
    }
}
