use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

use crate::tool::{
    McpResourceBlob, McpResourceRead, McpResourceTemplate, Tool, ToolCapabilities, ToolContext,
    ToolResult,
};

pub mod auth;
pub use auth::McpAuthTool;

pub struct ListMcpResourcesTool;
pub struct ListMcpResourceTemplatesTool;
pub struct ReadMcpResourceTool;
pub struct CleanupMcpResourceArtifactsTool;

const DEFAULT_MCP_RESOURCE_ARTIFACT_RETENTION: usize = 120;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct McpResourceCacheStats {
    pub list_hits: u64,
    pub list_misses: u64,
    pub read_hits: u64,
    pub read_misses: u64,
    pub cached_list_entries: usize,
    pub cached_read_entries: usize,
}

#[derive(Debug, Default)]
struct McpResourceCacheState {
    lists: HashMap<String, Vec<crate::tool::McpResource>>,
    reads: HashMap<(String, String), McpResourceRead>,
    stats: McpResourceCacheStats,
}

static MCP_RESOURCE_CACHE: LazyLock<Mutex<McpResourceCacheState>> =
    LazyLock::new(|| Mutex::new(McpResourceCacheState::default()));

pub fn mcp_resource_cache_stats() -> McpResourceCacheStats {
    MCP_RESOURCE_CACHE
        .lock()
        .map(|cache| cache.stats.clone())
        .unwrap_or_default()
}

#[async_trait]
impl Tool for ListMcpResourcesTool {
    fn name(&self) -> &str {
        "list_mcp_resources"
    }

    fn user_facing_name(&self) -> &str {
        ""
    }

    fn activity_description(&self, params: &Value) -> String {
        let server = params
            .get("server")
            .and_then(|v| v.as_str())
            .unwrap_or("all servers");
        format!("Listing MCP resources from: {}", server)
    }

    fn description(&self) -> &str {
        "List available resources from configured MCP servers. Use this to find shared context or data provided by servers."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "server": {
                    "type": "string",
                    "description": "Optional server name to filter resources by. Omit to list all."
                },
                "refresh": {
                    "type": "boolean",
                    "description": "Bypass the local MCP resource cache and refresh from the server.",
                    "default": false
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
        let provider = ctx
            .mcp_resources
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("MCP resource provider not available"))?;

        let server = params.get("server").and_then(|v| v.as_str());
        let refresh = params
            .get("refresh")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        let cache_key = server.unwrap_or("*").to_string();
        if !refresh {
            if let Ok(mut cache) = MCP_RESOURCE_CACHE.lock() {
                if let Some(resources) = cache.lists.get(&cache_key).cloned() {
                    cache.stats.list_hits = cache.stats.list_hits.saturating_add(1);
                    return render_list_resources(resources);
                }
                cache.stats.list_misses = cache.stats.list_misses.saturating_add(1);
            }
        }

        let resources = provider.list_resources(server).await?;
        if let Ok(mut cache) = MCP_RESOURCE_CACHE.lock() {
            cache.lists.insert(cache_key, resources.clone());
            cache.stats.cached_list_entries = cache.lists.len();
        }

        render_list_resources(resources)
    }
}

fn render_list_resources(resources: Vec<crate::tool::McpResource>) -> Result<ToolResult> {
    if resources.is_empty() {
        return Ok(ToolResult::success("No MCP resources found.".to_string()));
    }

    let mut output = String::from("Available MCP resources:\n\n");
    for resource in &resources {
        output.push_str(&format!(
            "- [{}] {}: {}{}\n",
            resource.server,
            resource.name,
            resource.uri,
            resource
                .description
                .as_ref()
                .map(|d| format!(" - {}", d))
                .unwrap_or_default()
        ));
    }

    Ok(ToolResult::success(output))
}

#[async_trait]
impl Tool for ListMcpResourceTemplatesTool {
    fn name(&self) -> &str {
        "list_mcp_resource_templates"
    }

    fn user_facing_name(&self) -> &str {
        ""
    }

    fn activity_description(&self, params: &Value) -> String {
        let server = params
            .get("server")
            .and_then(|v| v.as_str())
            .unwrap_or("all servers");
        format!("Listing MCP resource templates from: {}", server)
    }

    fn description(&self) -> &str {
        "List resource templates provided by MCP servers. Parameterized resource templates can provide context after filling URI template parameters."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "server": {
                    "type": "string",
                    "description": "Optional server name to filter resource templates by. Omit to list all."
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
        let provider = ctx
            .mcp_resources
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("MCP resource provider not available"))?;

        let server = params.get("server").and_then(|v| v.as_str());
        let templates = provider.list_resource_templates(server).await?;
        render_list_resource_templates(templates)
    }
}

fn render_list_resource_templates(templates: Vec<McpResourceTemplate>) -> Result<ToolResult> {
    if templates.is_empty() {
        return Ok(ToolResult::success(
            "No MCP resource templates found.".to_string(),
        ));
    }

    let mut output = String::from("Available MCP resource templates:\n\n");
    for template in &templates {
        output.push_str(&format!(
            "- [{}] {}: {}{}{}\n",
            template.server,
            template.name,
            template.uri_template,
            template
                .mime_type
                .as_ref()
                .map(|mime| format!(" ({})", mime))
                .unwrap_or_default(),
            template
                .description
                .as_ref()
                .map(|description| format!(" - {}", description))
                .unwrap_or_default()
        ));
    }

    Ok(ToolResult::success_with_metadata(
        output,
        json!({ "resource_templates": templates }),
    ))
}

#[async_trait]
impl Tool for ReadMcpResourceTool {
    fn name(&self) -> &str {
        "read_mcp_resource"
    }

    fn user_facing_name(&self) -> &str {
        ""
    }

    fn aliases(&self) -> Vec<String> {
        vec!["ReadMcpResource".to_string()]
    }

    fn activity_description(&self, params: &Value) -> String {
        let uri = params.get("uri").and_then(|v| v.as_str()).unwrap_or("");
        format!("Reading MCP resource: {}", uri)
    }

    fn description(&self) -> &str {
        "Read a specific resource from an MCP server."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "server": {
                    "type": "string",
                    "description": "The MCP server name"
                },
                "uri": {
                    "type": "string",
                    "description": "The resource URI to read"
                },
                "refresh": {
                    "type": "boolean",
                    "description": "Bypass the local MCP resource cache and refresh from the server.",
                    "default": false
                }
            },
            "required": ["server", "uri"]
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
        let provider = ctx
            .mcp_resources
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("MCP resource provider not available"))?;

        let server = params
            .get("server")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("'server' parameter is required"))?;
        let uri = params
            .get("uri")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("'uri' parameter is required"))?;
        if let Some(policy) = ctx.mcp_resource_policy.as_ref() {
            if let Err(reason) = policy.allows(server, uri) {
                return Ok(ToolResult::error_typed(
                    reason,
                    crate::tool::ToolErrorType::Permission,
                    false,
                    Some(
                        "Update mcp.resource_allow/resource_deny in config or choose an allowed MCP resource."
                            .to_string(),
                    ),
                ));
            }
        }
        let refresh = params
            .get("refresh")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);

        let cache_key = (server.to_string(), uri.to_string());
        if !refresh {
            if let Ok(mut cache) = MCP_RESOURCE_CACHE.lock() {
                if let Some(content) = cache.reads.get(&cache_key).cloned() {
                    cache.stats.read_hits = cache.stats.read_hits.saturating_add(1);
                    return render_read_resource_result(server, uri, content, ctx);
                }
                cache.stats.read_misses = cache.stats.read_misses.saturating_add(1);
            }
        }

        let content = provider.read_resource(server, uri).await?;
        if let Ok(mut cache) = MCP_RESOURCE_CACHE.lock() {
            cache.reads.insert(cache_key, content.clone());
            cache.stats.cached_read_entries = cache.reads.len();
        }
        render_read_resource_result(server, uri, content, ctx)
    }
}

fn render_read_resource_result(
    server: &str,
    uri: &str,
    read: McpResourceRead,
    ctx: &ToolContext,
) -> Result<ToolResult> {
    let mut content = read.content;
    let artifact_write = persist_mcp_resource_blob_artifacts(server, uri, &read.blobs, ctx)?;
    if !artifact_write.paths.is_empty() {
        content.push_str("\n\nArtifacts:\n");
        for artifact in &artifact_write.paths {
            content.push_str(&format!("- {}\n", artifact.display()));
        }
    }

    if artifact_write.paths.is_empty() {
        Ok(ToolResult::success(content))
    } else {
        Ok(ToolResult::success_with_metadata(
            content,
            json!({
                "mcp_resource": {
                    "server": server,
                    "uri": uri,
                    "blob_count": read.blobs.len(),
                    "decoded_count": artifact_write.decoded_count,
                    "decode_warning_count": artifact_write.decode_warning_count,
                    "retention": artifact_write.retention,
                    "manifest": artifact_write
                        .manifest_path
                        .as_ref()
                        .map(|path| path.display().to_string()),
                    "artifacts": artifact_write.paths
                        .iter()
                        .map(|path| path.display().to_string())
                        .collect::<Vec<_>>()
                }
            }),
        ))
    }
}

fn persist_mcp_resource_blob_artifacts(
    server: &str,
    uri: &str,
    blobs: &[McpResourceBlob],
    ctx: &ToolContext,
) -> Result<McpResourceArtifactWrite> {
    if blobs.is_empty() {
        return Ok(McpResourceArtifactWrite::default());
    }
    let Some(working_dir) = ctx.working_dir.as_deref() else {
        return Ok(McpResourceArtifactWrite::default());
    };

    let session_id = ctx.session_id.as_deref().unwrap_or("session");
    let short_session = short_session_id(session_id);
    let timestamp = unix_timestamp_secs();
    let retention = mcp_resource_artifact_retention();
    let dir = working_dir
        .join(".yode")
        .join("status")
        .join("mcp-resources");
    fs::create_dir_all(&dir)?;

    let slug = sanitize_artifact_segment(&format!("{}-{}", server, uri));
    let mut paths = Vec::new();
    let mut manifest = String::new();
    manifest.push_str("# MCP Resource Blob Artifact\n\n");
    manifest.push_str(&format!("- Server: {}\n", server));
    manifest.push_str(&format!("- URI: {}\n", uri));
    manifest.push_str(&format!("- Session: {}\n", session_id));
    manifest.push_str(&format!("- Blob count: {}\n", blobs.len()));
    manifest.push_str(&format!(
        "- Retention: keep newest {} artifact files\n",
        retention
    ));
    manifest.push_str("- Cleanup: /mcp resources cleanup [keep=N|all]\n\n");
    let mut decoded_count = 0usize;
    let mut decode_warning_count = 0usize;

    for (index, blob) in blobs.iter().enumerate() {
        let base64_path = dir.join(format!(
            "{}-mcp-resource-{}-{:02}-{}.b64",
            short_session, slug, index, timestamp
        ));
        fs::write(&base64_path, &blob.base64)?;
        manifest.push_str(&format!("## Blob {}\n\n", index + 1));
        manifest.push_str(&format!("- URI: {}\n", blob.uri));
        manifest.push_str(&format!("- MIME: {}\n", blob.mime_type));
        manifest.push_str(&format!("- Approx bytes: {}\n", blob.approx_bytes));
        manifest.push_str(&format!("- Base64 file: {}\n", base64_path.display()));
        paths.push(base64_path);

        match decode_base64_standard(&blob.base64) {
            Ok(bytes) => {
                let decoded_path = dir.join(format!(
                    "{}-mcp-resource-{}-{:02}-{}.{}",
                    short_session,
                    slug,
                    index,
                    timestamp,
                    resource_extension(&blob.mime_type, &blob.uri)
                ));
                fs::write(&decoded_path, bytes)?;
                manifest.push_str(&format!("- Decoded file: {}\n\n", decoded_path.display()));
                paths.push(decoded_path);
                decoded_count = decoded_count.saturating_add(1);
            }
            Err(err) => {
                manifest.push_str(&format!("- Decode warning: {}\n\n", err));
                decode_warning_count = decode_warning_count.saturating_add(1);
            }
        }
    }

    let manifest_path = dir.join(format!(
        "{}-mcp-resource-{}-{}.md",
        short_session, slug, timestamp
    ));
    fs::write(&manifest_path, manifest)?;
    paths.push(manifest_path.clone());
    let _ = prune_mcp_resource_artifacts(&dir, retention, paths.as_slice());
    Ok(McpResourceArtifactWrite {
        paths,
        manifest_path: Some(manifest_path),
        retention,
        decoded_count,
        decode_warning_count,
    })
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct McpResourceArtifactWrite {
    paths: Vec<PathBuf>,
    manifest_path: Option<PathBuf>,
    retention: usize,
    decoded_count: usize,
    decode_warning_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpResourceArtifactCleanup {
    pub dir: PathBuf,
    pub removed: usize,
    pub kept: usize,
}

pub fn cleanup_mcp_resource_artifacts(
    project_root: &Path,
    keep: usize,
) -> Result<McpResourceArtifactCleanup> {
    let dir = project_root
        .join(".yode")
        .join("status")
        .join("mcp-resources");
    if !dir.exists() {
        return Ok(McpResourceArtifactCleanup {
            dir,
            removed: 0,
            kept: 0,
        });
    }

    let total = fs::read_dir(&dir)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .count();
    let removed = prune_mcp_resource_artifacts(&dir, keep, &[])?;
    Ok(McpResourceArtifactCleanup {
        dir,
        removed,
        kept: total.saturating_sub(removed),
    })
}

fn prune_mcp_resource_artifacts(dir: &Path, keep: usize, protected: &[PathBuf]) -> Result<usize> {
    let protected = protected
        .iter()
        .filter_map(|path| path.canonicalize().ok())
        .collect::<std::collections::HashSet<_>>();
    let mut entries = fs::read_dir(dir)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .map(|path| {
            let modified = path
                .metadata()
                .and_then(|metadata| metadata.modified())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            (path, modified)
        })
        .collect::<Vec<_>>();
    if entries.len() <= keep {
        return Ok(0);
    }
    entries.sort_by(|(left_path, left_modified), (right_path, right_modified)| {
        right_modified
            .cmp(left_modified)
            .then_with(|| right_path.file_name().cmp(&left_path.file_name()))
    });

    let mut removed = 0usize;
    for (path, _) in entries.into_iter().skip(keep) {
        if path
            .canonicalize()
            .ok()
            .is_some_and(|path| protected.contains(&path))
        {
            continue;
        }
        if fs::remove_file(&path).is_ok() {
            removed = removed.saturating_add(1);
        }
    }
    Ok(removed)
}

pub fn mcp_resource_artifact_retention() -> usize {
    retention_from_env(
        std::env::var("YODE_MCP_RESOURCE_ARTIFACT_RETENTION")
            .ok()
            .as_deref(),
    )
}

fn retention_from_env(value: Option<&str>) -> usize {
    value
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_MCP_RESOURCE_ARTIFACT_RETENTION)
}

#[async_trait]
impl Tool for CleanupMcpResourceArtifactsTool {
    fn name(&self) -> &str {
        "cleanup_mcp_resource_artifacts"
    }

    fn user_facing_name(&self) -> &str {
        ""
    }

    fn aliases(&self) -> Vec<String> {
        vec!["CleanupMcpResourceArtifacts".to_string()]
    }

    fn activity_description(&self, params: &Value) -> String {
        let keep = cleanup_keep_from_params(params).unwrap_or_else(mcp_resource_artifact_retention);
        format!("Cleaning MCP resource artifacts, keeping {}", keep)
    }

    fn description(&self) -> &str {
        "Clean local MCP resource blob artifacts under .yode/status/mcp-resources. Use this when binary MCP resource artifacts are taking too much disk space."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "keep": {
                    "type": "integer",
                    "minimum": 0,
                    "description": "Number of newest artifact files to keep. Defaults to YODE_MCP_RESOURCE_ARTIFACT_RETENTION or 120. Use 0 to remove all."
                }
            }
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
        let working_dir = ctx
            .working_dir
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("working directory not available"))?;
        let keep =
            cleanup_keep_from_params(&params).unwrap_or_else(mcp_resource_artifact_retention);
        let cleanup = cleanup_mcp_resource_artifacts(working_dir, keep)?;
        Ok(ToolResult::success_with_metadata(
            format!(
                "MCP resource artifact cleanup complete: removed={} kept={} retention={} dir={}",
                cleanup.removed,
                cleanup.kept,
                keep,
                cleanup.dir.display()
            ),
            json!({
                "mcp_resource_artifact_cleanup": {
                    "dir": cleanup.dir.display().to_string(),
                    "removed": cleanup.removed,
                    "kept": cleanup.kept,
                    "retention": keep
                }
            }),
        ))
    }
}

fn cleanup_keep_from_params(params: &Value) -> Option<usize> {
    params
        .get("keep")
        .and_then(|value| value.as_u64())
        .map(|value| value as usize)
}

fn resource_extension(mime_type: &str, uri: &str) -> &'static str {
    match mime_type {
        "image/png" => "png",
        "image/jpeg" | "image/jpg" => "jpg",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "image/svg+xml" => "svg",
        "application/pdf" => "pdf",
        "application/json" => "json",
        "text/plain" => "txt",
        "text/markdown" => "md",
        _ => uri_extension(uri).unwrap_or("bin"),
    }
}

fn uri_extension(uri: &str) -> Option<&'static str> {
    let without_query = uri.split(['?', '#']).next().unwrap_or(uri);
    let extension = without_query.rsplit_once('.')?.1;
    match extension.to_ascii_lowercase().as_str() {
        "png" => Some("png"),
        "jpg" | "jpeg" => Some("jpg"),
        "gif" => Some("gif"),
        "webp" => Some("webp"),
        "svg" => Some("svg"),
        "pdf" => Some("pdf"),
        "json" => Some("json"),
        "txt" => Some("txt"),
        "md" => Some("md"),
        _ => None,
    }
}

fn decode_base64_standard(value: &str) -> Result<Vec<u8>> {
    let compact = value
        .chars()
        .filter(|ch| !ch.is_ascii_whitespace())
        .collect::<Vec<_>>();
    if compact.is_empty() {
        return Ok(Vec::new());
    }
    if compact.len() % 4 != 0 {
        return Err(anyhow::anyhow!("invalid base64 length {}", compact.len()));
    }

    let mut output = Vec::with_capacity(compact.len() / 4 * 3);
    for chunk in compact.chunks(4) {
        let mut values = [0u8; 4];
        let mut padding = 0usize;
        for (index, ch) in chunk.iter().copied().enumerate() {
            if ch == '=' {
                padding += 1;
                values[index] = 0;
            } else if padding > 0 {
                return Err(anyhow::anyhow!("invalid base64 padding"));
            } else {
                values[index] = base64_value(ch)
                    .ok_or_else(|| anyhow::anyhow!("invalid base64 character '{}'", ch))?;
            }
        }
        if padding > 2 {
            return Err(anyhow::anyhow!("invalid base64 padding"));
        }

        output.push((values[0] << 2) | (values[1] >> 4));
        if padding < 2 {
            output.push((values[1] << 4) | (values[2] >> 2));
        }
        if padding == 0 {
            output.push((values[2] << 6) | values[3]);
        }
    }
    Ok(output)
}

fn base64_value(ch: char) -> Option<u8> {
    match ch {
        'A'..='Z' => Some(ch as u8 - b'A'),
        'a'..='z' => Some(ch as u8 - b'a' + 26),
        '0'..='9' => Some(ch as u8 - b'0' + 52),
        '+' => Some(62),
        '/' => Some(63),
        _ => None,
    }
}

fn short_session_id(session_id: &str) -> String {
    session_id.chars().take(8).collect()
}

fn unix_timestamp_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn sanitize_artifact_segment(value: &str) -> String {
    let sanitized = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .chars()
        .take(64)
        .collect::<String>();
    if sanitized.is_empty() {
        "resource".to_string()
    } else {
        sanitized
    }
}

#[cfg(test)]
pub(crate) fn reset_mcp_resource_cache() {
    if let Ok(mut cache) = MCP_RESOURCE_CACHE.lock() {
        *cache = McpResourceCacheState::default();
    }
}

#[cfg(test)]
mod tests {
    use super::{
        cleanup_mcp_resource_artifacts, decode_base64_standard, mcp_resource_cache_stats,
        prune_mcp_resource_artifacts, reset_mcp_resource_cache, resource_extension,
        retention_from_env, CleanupMcpResourceArtifactsTool, ListMcpResourceTemplatesTool,
        ListMcpResourcesTool, ReadMcpResourceTool,
    };
    use crate::tool::{
        McpResource, McpResourceBlob, McpResourcePolicy, McpResourceProvider, McpResourceRead,
        McpResourceTemplate, Tool, ToolContext,
    };
    use anyhow::Result;
    use serde_json::json;
    use std::pin::Pin;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::sync::LazyLock;

    static CACHE_TEST_LOCK: LazyLock<tokio::sync::Mutex<()>> =
        LazyLock::new(|| tokio::sync::Mutex::new(()));

    struct MockMcpProvider {
        list_calls: AtomicUsize,
        read_calls: AtomicUsize,
        include_blob: bool,
    }

    impl MockMcpProvider {
        fn new() -> Self {
            Self {
                list_calls: AtomicUsize::new(0),
                read_calls: AtomicUsize::new(0),
                include_blob: false,
            }
        }

        fn with_blob() -> Self {
            Self {
                list_calls: AtomicUsize::new(0),
                read_calls: AtomicUsize::new(0),
                include_blob: true,
            }
        }
    }

    impl McpResourceProvider for MockMcpProvider {
        fn list_resources(
            &self,
            server: Option<&str>,
        ) -> Pin<Box<dyn std::future::Future<Output = Result<Vec<McpResource>>> + Send + '_>>
        {
            let server = server.unwrap_or("all").to_string();
            self.list_calls.fetch_add(1, Ordering::SeqCst);
            Box::pin(async move {
                Ok(vec![McpResource {
                    server,
                    uri: "mcp://resource".to_string(),
                    name: "resource".to_string(),
                    description: Some("demo".to_string()),
                }])
            })
        }

        fn read_resource(
            &self,
            server: &str,
            uri: &str,
        ) -> Pin<Box<dyn std::future::Future<Output = Result<McpResourceRead>> + Send + '_>>
        {
            let response = format!("{}:{}", server, uri);
            self.read_calls.fetch_add(1, Ordering::SeqCst);
            let blobs = if self.include_blob {
                vec![McpResourceBlob {
                    uri: uri.to_string(),
                    mime_type: "image/png".to_string(),
                    base64: "ZmFrZQ==".to_string(),
                    approx_bytes: 4,
                }]
            } else {
                Vec::new()
            };
            Box::pin(async move {
                Ok(McpResourceRead {
                    content: response,
                    blobs,
                })
            })
        }

        fn list_resource_templates(
            &self,
            server: Option<&str>,
        ) -> Pin<Box<dyn std::future::Future<Output = Result<Vec<McpResourceTemplate>>> + Send + '_>>
        {
            let server = server.unwrap_or("all").to_string();
            Box::pin(async move {
                Ok(vec![McpResourceTemplate {
                    server,
                    uri_template: "mcp://resource/{id}".to_string(),
                    name: "resource-template".to_string(),
                    description: Some("demo template".to_string()),
                    mime_type: Some("text/plain".to_string()),
                }])
            })
        }
    }

    #[tokio::test]
    async fn list_mcp_resources_uses_cache_on_repeated_calls() {
        let _guard = CACHE_TEST_LOCK.lock().await;
        reset_mcp_resource_cache();
        let provider = Arc::new(MockMcpProvider::new());
        let mut ctx = ToolContext::empty();
        ctx.mcp_resources = Some(provider.clone());

        let tool = ListMcpResourcesTool;
        let first = tool.execute(json!({"server": "demo"}), &ctx).await.unwrap();
        let second = tool.execute(json!({"server": "demo"}), &ctx).await.unwrap();

        assert!(!first.is_error);
        assert!(!second.is_error);
        assert_eq!(provider.list_calls.load(Ordering::SeqCst), 1);
        let stats = mcp_resource_cache_stats();
        assert_eq!(stats.list_misses, 1);
        assert_eq!(stats.list_hits, 1);
        assert_eq!(stats.cached_list_entries, 1);
    }

    #[tokio::test]
    async fn read_mcp_resource_uses_cache_on_repeated_calls() {
        let _guard = CACHE_TEST_LOCK.lock().await;
        reset_mcp_resource_cache();
        let provider = Arc::new(MockMcpProvider::new());
        let mut ctx = ToolContext::empty();
        ctx.mcp_resources = Some(provider.clone());

        let tool = ReadMcpResourceTool;
        let first = tool
            .execute(json!({"server": "demo", "uri": "mcp://resource"}), &ctx)
            .await
            .unwrap();
        let second = tool
            .execute(json!({"server": "demo", "uri": "mcp://resource"}), &ctx)
            .await
            .unwrap();

        assert!(!first.is_error);
        assert!(!second.is_error);
        assert_eq!(provider.read_calls.load(Ordering::SeqCst), 1);
        let stats = mcp_resource_cache_stats();
        assert_eq!(stats.read_misses, 1);
        assert_eq!(stats.read_hits, 1);
        assert_eq!(stats.cached_read_entries, 1);
    }

    #[tokio::test]
    async fn read_mcp_resource_enforces_explicit_policy() {
        let _guard = CACHE_TEST_LOCK.lock().await;
        reset_mcp_resource_cache();
        let provider = Arc::new(MockMcpProvider::new());
        let mut ctx = ToolContext::empty();
        ctx.mcp_resources = Some(provider);
        ctx.mcp_resource_policy = Some(Arc::new(McpResourcePolicy {
            allow: vec!["github:repo://allowed/*".to_string()],
            deny: vec!["github:repo://allowed/private".to_string()],
        }));

        let tool = ReadMcpResourceTool;
        let denied = tool
            .execute(json!({"server":"github","uri":"repo://other/readme"}), &ctx)
            .await
            .unwrap();
        assert!(denied.is_error);
        assert!(denied.content.contains("not allowed"));

        let explicit_deny = tool
            .execute(
                json!({"server":"github","uri":"repo://allowed/private"}),
                &ctx,
            )
            .await
            .unwrap();
        assert!(explicit_deny.is_error);
        assert!(explicit_deny.content.contains("denied by policy"));

        let allowed = tool
            .execute(
                json!({"server":"github","uri":"repo://allowed/readme"}),
                &ctx,
            )
            .await
            .unwrap();
        assert!(!allowed.is_error);
    }

    #[tokio::test]
    async fn list_mcp_resources_refresh_bypasses_cache() {
        let _guard = CACHE_TEST_LOCK.lock().await;
        reset_mcp_resource_cache();
        let provider = Arc::new(MockMcpProvider::new());
        let mut ctx = ToolContext::empty();
        ctx.mcp_resources = Some(provider.clone());

        let tool = ListMcpResourcesTool;
        let _ = tool.execute(json!({"server": "demo"}), &ctx).await.unwrap();
        let _ = tool
            .execute(json!({"server": "demo", "refresh": true}), &ctx)
            .await
            .unwrap();

        assert_eq!(provider.list_calls.load(Ordering::SeqCst), 2);
        let stats = mcp_resource_cache_stats();
        assert_eq!(stats.list_misses, 1);
        assert_eq!(stats.list_hits, 0);
    }

    #[tokio::test]
    async fn list_mcp_resource_templates_renders_templates() {
        let provider = Arc::new(MockMcpProvider::new());
        let mut ctx = ToolContext::empty();
        ctx.mcp_resources = Some(provider);

        let tool = ListMcpResourceTemplatesTool;
        let result = tool
            .execute(json!({"server": "demo"}), &ctx)
            .await
            .unwrap();

        assert!(!result.is_error);
        assert!(result.content.contains("mcp://resource/{id}"));
        assert_eq!(
            result.metadata.as_ref().unwrap()["resource_templates"][0]["name"],
            json!("resource-template")
        );
    }

    #[tokio::test]
    async fn read_mcp_resource_refresh_bypasses_cache() {
        let _guard = CACHE_TEST_LOCK.lock().await;
        reset_mcp_resource_cache();
        let provider = Arc::new(MockMcpProvider::new());
        let mut ctx = ToolContext::empty();
        ctx.mcp_resources = Some(provider.clone());

        let tool = ReadMcpResourceTool;
        let _ = tool
            .execute(json!({"server": "demo", "uri": "mcp://resource"}), &ctx)
            .await
            .unwrap();
        let _ = tool
            .execute(
                json!({"server": "demo", "uri": "mcp://resource", "refresh": true}),
                &ctx,
            )
            .await
            .unwrap();

        assert_eq!(provider.read_calls.load(Ordering::SeqCst), 2);
        let stats = mcp_resource_cache_stats();
        assert_eq!(stats.read_misses, 1);
        assert_eq!(stats.read_hits, 0);
    }

    #[tokio::test]
    async fn read_mcp_resource_writes_blob_artifacts_when_context_has_working_dir() {
        let _guard = CACHE_TEST_LOCK.lock().await;
        reset_mcp_resource_cache();
        let dir = tempfile::tempdir().unwrap();
        let provider = Arc::new(MockMcpProvider::with_blob());
        let mut ctx = ToolContext::empty();
        ctx.mcp_resources = Some(provider);
        ctx.working_dir = Some(dir.path().to_path_buf());
        ctx.session_id = Some("session-abcdef".to_string());

        let tool = ReadMcpResourceTool;
        let result = tool
            .execute(json!({"server": "demo", "uri": "mcp://image"}), &ctx)
            .await
            .unwrap();

        assert!(!result.is_error);
        assert!(result.content.contains("Artifacts:"));
        let artifact_dir = dir.path().join(".yode/status/mcp-resources");
        let entries = std::fs::read_dir(&artifact_dir)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect::<Vec<_>>();
        assert_eq!(entries.len(), 3);
        assert!(entries
            .iter()
            .any(|path| path.extension().is_some_and(|ext| ext == "b64")));
        let decoded_path = entries
            .iter()
            .find(|path| path.extension().is_some_and(|ext| ext == "png"))
            .unwrap();
        assert_eq!(std::fs::read(decoded_path).unwrap(), b"fake");
        assert!(entries
            .iter()
            .any(|path| path.extension().is_some_and(|ext| ext == "md")));
        let manifest_path = entries
            .iter()
            .find(|path| path.extension().is_some_and(|ext| ext == "md"))
            .unwrap();
        let manifest = std::fs::read_to_string(manifest_path).unwrap();
        assert!(manifest.contains("Retention: keep newest"));
        assert!(manifest.contains("Cleanup: /mcp resources cleanup"));
        let metadata = result.metadata.as_ref().unwrap();
        let resource = metadata.get("mcp_resource").unwrap();
        assert_eq!(
            resource
                .get("decoded_count")
                .and_then(|value| value.as_u64()),
            Some(1)
        );
        assert_eq!(
            resource
                .get("decode_warning_count")
                .and_then(|value| value.as_u64()),
            Some(0)
        );
        assert!(resource.get("retention").is_some());
        assert!(resource.get("manifest").is_some());
    }

    #[test]
    fn decode_base64_standard_decodes_with_whitespace() {
        assert_eq!(decode_base64_standard(" Zm\nFrZQ== ").unwrap(), b"fake");
        assert!(decode_base64_standard("bad").is_err());
        assert!(decode_base64_standard("Zm=F").is_err());
    }

    #[test]
    fn resource_extension_prefers_mime_then_uri() {
        assert_eq!(resource_extension("image/png", "mcp://asset.bin"), "png");
        assert_eq!(
            resource_extension("application/octet-stream", "mcp://asset.pdf?x=1"),
            "pdf"
        );
        assert_eq!(
            resource_extension("application/octet-stream", "mcp://asset"),
            "bin"
        );
    }

    #[test]
    fn mcp_resource_artifact_retention_env_parser_defaults_safely() {
        assert_eq!(retention_from_env(None), 120);
        assert_eq!(retention_from_env(Some("0")), 120);
        assert_eq!(retention_from_env(Some("invalid")), 120);
        assert_eq!(retention_from_env(Some("7")), 7);
    }

    #[test]
    fn prune_mcp_resource_artifacts_removes_old_unprotected_files() {
        let dir = tempfile::tempdir().unwrap();
        let first = dir.path().join("a-mcp-resource.md");
        let second = dir.path().join("b-mcp-resource.b64");
        let protected = dir.path().join("c-mcp-resource.png");
        std::fs::write(&first, "first").unwrap();
        std::fs::write(&second, "second").unwrap();
        std::fs::write(&protected, "protected").unwrap();

        let removed =
            prune_mcp_resource_artifacts(dir.path(), 1, std::slice::from_ref(&protected)).unwrap();

        assert_eq!(removed, 2);
        assert!(!first.exists());
        assert!(!second.exists());
        assert!(protected.exists());
    }

    #[test]
    fn cleanup_mcp_resource_artifacts_keeps_requested_count() {
        let dir = tempfile::tempdir().unwrap();
        let artifact_dir = dir.path().join(".yode/status/mcp-resources");
        std::fs::create_dir_all(&artifact_dir).unwrap();
        std::fs::write(artifact_dir.join("a.md"), "a").unwrap();
        std::fs::write(artifact_dir.join("b.b64"), "b").unwrap();
        std::fs::write(artifact_dir.join("c.png"), "c").unwrap();

        let cleanup = cleanup_mcp_resource_artifacts(dir.path(), 1).unwrap();

        assert_eq!(cleanup.removed, 2);
        assert_eq!(cleanup.kept, 1);
        assert_eq!(
            std::fs::read_dir(&artifact_dir).unwrap().flatten().count(),
            1
        );
    }

    #[tokio::test]
    async fn cleanup_mcp_resource_artifacts_tool_removes_all_when_keep_zero() {
        let dir = tempfile::tempdir().unwrap();
        let artifact_dir = dir.path().join(".yode/status/mcp-resources");
        std::fs::create_dir_all(&artifact_dir).unwrap();
        std::fs::write(artifact_dir.join("a.md"), "a").unwrap();
        std::fs::write(artifact_dir.join("b.b64"), "b").unwrap();

        let mut ctx = ToolContext::empty();
        ctx.working_dir = Some(dir.path().to_path_buf());
        let tool = CleanupMcpResourceArtifactsTool;
        let caps = tool.capabilities();
        assert!(caps.requires_confirmation);
        assert!(!caps.read_only);

        let result = tool.execute(json!({"keep": 0}), &ctx).await.unwrap();

        assert!(!result.is_error);
        assert!(result.content.contains("removed=2"));
        assert_eq!(
            std::fs::read_dir(&artifact_dir).unwrap().flatten().count(),
            0
        );
        assert!(result.metadata.is_some());
    }
}
