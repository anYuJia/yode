use std::path::Path;

use yode_core::config::McpServerConfig;
use yode_mcp::{McpReconnectDiagnostic, McpToolLatencyEntry};
use yode_tools::{mcp_resource_artifact_retention, mcp_resource_cache_stats};

use crate::commands::workspace_text::{workspace_bullets, WorkspaceText};
use crate::mcp_resource_artifacts::{
    mcp_resource_decode_warning_count, mcp_resource_manifest_summary,
};

pub(crate) fn auth_session_summary(config: &McpServerConfig) -> String {
    let env_count = config.env.len();
    let referenced = config
        .env
        .values()
        .filter(|value| value.starts_with('$'))
        .count();
    let auth = config
        .auth
        .as_ref()
        .map(|auth| {
            if auth.bearer_token_env.is_some() {
                "bearer-env"
            } else if auth.oauth.is_some() {
                "oauth"
            } else {
                "auth-empty"
            }
        })
        .unwrap_or("none");
    let endpoint = match config.transport {
        yode_core::config::McpTransportConfig::Stdio => config.command.as_str(),
        _ => config.url.as_deref().unwrap_or("missing-url"),
    };
    format!(
        "transport={} env={} refs={} auth={} endpoint={}",
        config.transport.label(),
        env_count,
        referenced,
        auth,
        endpoint
    )
}

pub(crate) fn latency_sparkline(stats: &[McpToolLatencyEntry], server: &str) -> String {
    let mut samples = stats
        .iter()
        .filter(|entry| entry.server == server)
        .map(|entry| entry.last_ms.min(200))
        .collect::<Vec<_>>();
    if samples.is_empty() {
        return "no-calls".to_string();
    }
    samples.sort();
    samples
        .iter()
        .take(4)
        .map(|sample| match *sample {
            0..=20 => '▁',
            21..=50 => '▂',
            51..=90 => '▃',
            91..=140 => '▅',
            _ => '▇',
        })
        .collect()
}

pub(crate) fn resource_cache_activity_summary() -> String {
    let stats = mcp_resource_cache_stats();
    format!(
        "list {}/{} · read {}/{} · cached list={} read={}",
        stats.list_hits,
        stats.list_misses,
        stats.read_hits,
        stats.read_misses,
        stats.cached_list_entries,
        stats.cached_read_entries
    )
}

pub(crate) fn mcp_resource_artifact_summary(project_root: &Path) -> String {
    let dir = project_root
        .join(".yode")
        .join("status")
        .join("mcp-resources");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return "none".to_string();
    };

    let mut total = 0usize;
    let mut manifests = 0usize;
    let mut base64_files = 0usize;
    let mut decoded_files = 0usize;
    let mut decode_warnings = 0usize;
    let mut latest: Option<(std::time::SystemTime, String)> = None;
    let mut latest_manifest_summary: Option<(std::time::SystemTime, String)> = None;

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        total += 1;
        match path.extension().and_then(|ext| ext.to_str()).unwrap_or("") {
            "md" => {
                manifests += 1;
                decode_warnings =
                    decode_warnings.saturating_add(mcp_resource_decode_warning_count(&path));
            }
            "b64" => base64_files += 1,
            _ => decoded_files += 1,
        }
        let modified = entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("unknown")
            .to_string();
        if latest
            .as_ref()
            .map(|(current, _)| modified > *current)
            .unwrap_or(true)
        {
            latest = Some((modified, name));
        }
        if path.extension().and_then(|ext| ext.to_str()) == Some("md") {
            if let Some(summary) = mcp_resource_manifest_summary(&path, false, " ") {
                if latest_manifest_summary
                    .as_ref()
                    .map(|(current, _)| modified > *current)
                    .unwrap_or(true)
                {
                    latest_manifest_summary = Some((modified, summary));
                }
            }
        }
    }

    if total == 0 {
        return "none".to_string();
    }
    let latest_name = latest
        .map(|(_, name)| name)
        .unwrap_or_else(|| "unknown".to_string());
    let latest_manifest_summary = latest_manifest_summary.map(|(_, summary)| summary);
    format!(
        "files={} manifest={} base64={} decoded={} decode_warnings={} retention={} latest={}{}",
        total,
        manifests,
        base64_files,
        decoded_files,
        decode_warnings,
        mcp_resource_artifact_retention(),
        latest_name,
        latest_manifest_summary
            .map(|summary| format!(" ({})", summary))
            .unwrap_or_default()
    )
}

pub(crate) fn reconnect_backoff_timeline(stats: &[McpReconnectDiagnostic], server: &str) -> String {
    let Some(entry) = stats.iter().find(|entry| entry.server == server) else {
        return "no-attempts".to_string();
    };
    format!(
        "attempts={} -> failures={} -> next={}s",
        entry.attempts, entry.failures, entry.next_backoff_secs
    )
}

pub(crate) fn remote_tool_source_badge(tool_name: &str) -> &'static str {
    if tool_name.starts_with("mcp__") {
        "[mcp]"
    } else if matches!(tool_name, "web_search" | "web_fetch" | "web_browser") {
        "[browser]"
    } else {
        "[local]"
    }
}

pub(crate) fn browser_mcp_capability_summary(
    browser_tools_present: bool,
    configured_servers: usize,
) -> String {
    format!(
        "browser_tools={} / configured_mcp_servers={}",
        browser_tools_present, configured_servers
    )
}

pub(crate) fn browser_execution_outcome_summary(
    action: Option<&str>,
    message: Option<&str>,
) -> String {
    match (action, message) {
        (Some(action), Some(message)) => format!("{} -> {}", action, message),
        (Some(action), None) => action.to_string(),
        (None, Some(message)) => message.to_string(),
        (None, None) => "no recent browser outcomes".to_string(),
    }
}

pub(crate) fn compact_browser_state_payload(
    browser_tools_present: bool,
    configured_servers: usize,
) -> serde_json::Value {
    serde_json::json!({
        "browser_tools_present": browser_tools_present,
        "configured_mcp_servers": configured_servers,
        "resource_cache": resource_cache_activity_summary(),
        "capability_summary": browser_mcp_capability_summary(
            browser_tools_present,
            configured_servers,
        ),
        "last_browser_outcome": browser_execution_outcome_summary(None, None),
    })
}

pub(crate) fn write_browser_access_state_artifact(
    project_root: &Path,
    session_id: &str,
    browser_tools_present: bool,
    configured_servers: usize,
) -> Option<String> {
    let dir = project_root.join(".yode").join("remote");
    std::fs::create_dir_all(&dir).ok()?;
    let short_session = session_id.chars().take(8).collect::<String>();
    let path = dir.join(format!("{}-browser-access-state.json", short_session));
    let payload = compact_browser_state_payload(browser_tools_present, configured_servers);
    std::fs::write(
        &path,
        serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string()),
    )
    .ok()?;
    Some(path.display().to_string())
}

pub(crate) fn render_browser_access_workspace(path: &Path) -> Option<String> {
    let content = std::fs::read_to_string(path).ok()?;
    let payload: serde_json::Value = serde_json::from_str(&content).ok()?;
    Some(
        WorkspaceText::new("Browser access workspace")
            .subtitle(path.display().to_string())
            .field(
                "Capability summary",
                payload
                    .get("capability_summary")
                    .and_then(|value| value.as_str())
                    .unwrap_or("unknown"),
            )
            .section(
                "State",
                workspace_bullets([
                    format!(
                        "browser_tools_present={}",
                        payload
                            .get("browser_tools_present")
                            .and_then(|value| value.as_bool())
                            .unwrap_or(false)
                    ),
                    format!(
                        "configured_mcp_servers={}",
                        payload
                            .get("configured_mcp_servers")
                            .and_then(|value| value.as_u64())
                            .unwrap_or(0)
                    ),
                    payload
                        .get("resource_cache")
                        .and_then(|value| value.as_str())
                        .unwrap_or("resource cache unavailable")
                        .to_string(),
                    payload
                        .get("last_browser_outcome")
                        .and_then(|value| value.as_str())
                        .unwrap_or("no recent browser outcomes")
                        .to_string(),
                ]),
            )
            .render(),
    )
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use yode_core::config::McpServerConfig;
    use yode_mcp::{McpReconnectDiagnostic, McpToolLatencyEntry};

    use super::{
        auth_session_summary, browser_execution_outcome_summary, browser_mcp_capability_summary,
        compact_browser_state_payload, latency_sparkline, mcp_resource_artifact_summary,
        reconnect_backoff_timeline, remote_tool_source_badge, write_browser_access_state_artifact,
    };

    #[test]
    fn auth_summary_reports_env_and_command() {
        let summary = auth_session_summary(&McpServerConfig {
            command: "node".to_string(),
            args: vec![],
            env: HashMap::from([
                ("TOKEN".to_string(), "$TOKEN".to_string()),
                ("MODE".to_string(), "inline".to_string()),
            ]),
            ..McpServerConfig::default()
        });
        assert!(summary.contains("env=2"));
        assert!(summary.contains("refs=1"));
        assert!(summary.contains("transport=stdio"));
        assert!(summary.contains("endpoint=node"));
    }

    #[test]
    fn latency_and_reconnect_helpers_render_compact_strings() {
        let sparkline = latency_sparkline(
            &[McpToolLatencyEntry {
                server: "github".to_string(),
                tool: "list_prs".to_string(),
                calls: 1,
                errors: 0,
                avg_ms: 40,
                max_ms: 40,
                last_ms: 40,
            }],
            "github",
        );
        assert!(!sparkline.is_empty());

        let reconnect = reconnect_backoff_timeline(
            &[McpReconnectDiagnostic {
                server: "github".to_string(),
                attempts: 3,
                failures: 2,
                last_error: Some("timeout".to_string()),
                next_backoff_secs: 4,
            }],
            "github",
        );
        assert!(reconnect.contains("next=4s"));
    }

    #[test]
    fn capability_summary_and_badges_render() {
        assert_eq!(remote_tool_source_badge("mcp__github_list_prs"), "[mcp]");
        assert_eq!(remote_tool_source_badge("web_browser"), "[browser]");
        assert!(browser_mcp_capability_summary(true, 2).contains("configured_mcp_servers=2"));
        assert_eq!(
            browser_execution_outcome_summary(Some("navigate"), Some("ok")),
            "navigate -> ok"
        );
    }

    #[test]
    fn writes_browser_access_state_artifact() {
        let dir = std::env::temp_dir().join(format!("yode-browser-state-{}", uuid::Uuid::new_v4()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = write_browser_access_state_artifact(&dir, "session-1234", true, 2).unwrap();
        let content = std::fs::read_to_string(path).unwrap();
        assert!(content.contains("\"browser_tools_present\": true"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn compact_browser_payload_contains_summary_fields() {
        let payload = compact_browser_state_payload(true, 2);
        assert_eq!(
            payload
                .get("configured_mcp_servers")
                .and_then(|value| value.as_u64()),
            Some(2)
        );
        assert!(payload.get("resource_cache").is_some());
    }

    #[test]
    fn mcp_resource_artifact_summary_counts_saved_files() {
        let dir = std::env::temp_dir().join(format!("yode-mcp-artifacts-{}", uuid::Uuid::new_v4()));
        let artifact_dir = dir.join(".yode/status/mcp-resources");
        std::fs::create_dir_all(&artifact_dir).unwrap();
        std::fs::write(
            artifact_dir.join("session-mcp-resource-demo.md"),
            "# MCP Resource Blob Artifact\n\n- Server: demo\n- URI: mcp://image\n- Blob count: 1\n- Retention: keep newest 120 artifact files\n\n## Blob 1\n\n- Decode warning: invalid base64\n",
        )
        .unwrap();
        std::fs::write(
            artifact_dir.join("session-mcp-resource-demo.b64"),
            "ZmFrZQ==",
        )
        .unwrap();
        std::fs::write(artifact_dir.join("session-mcp-resource-demo.png"), b"fake").unwrap();

        let summary = mcp_resource_artifact_summary(&dir);
        assert!(summary.contains("files=3"));
        assert!(summary.contains("manifest=1"));
        assert!(summary.contains("base64=1"));
        assert!(summary.contains("decoded=1"));
        assert!(summary.contains("decode_warnings=1"));
        assert!(summary.contains("retention="));
        assert!(summary.contains("server=demo"));
        assert!(summary.contains("uri=mcp://image"));
        assert!(summary.contains("blobs=1"));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
