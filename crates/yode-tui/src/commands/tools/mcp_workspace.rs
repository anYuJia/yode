use std::path::Path;

use yode_core::config::McpServerConfig;
use yode_mcp::{McpReconnectDiagnostic, McpToolLatencyEntry};
use yode_tools::mcp_resource_cache_stats;

pub(crate) fn auth_session_summary(config: &McpServerConfig) -> String {
    let env_count = config.env.len();
    let referenced = config
        .env
        .values()
        .filter(|value| value.starts_with('$'))
        .count();
    format!("env={} refs={} cmd={}", env_count, referenced, config.command)
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
    let payload = serde_json::json!({
        "browser_tools_present": browser_tools_present,
        "configured_mcp_servers": configured_servers,
        "resource_cache": resource_cache_activity_summary(),
        "capability_summary": browser_mcp_capability_summary(
            browser_tools_present,
            configured_servers,
        ),
    });
    std::fs::write(&path, serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string())).ok()?;
    Some(path.display().to_string())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use yode_core::config::McpServerConfig;
    use yode_mcp::{McpReconnectDiagnostic, McpToolLatencyEntry};

    use super::{
        auth_session_summary, browser_mcp_capability_summary, latency_sparkline,
        reconnect_backoff_timeline, remote_tool_source_badge,
        write_browser_access_state_artifact,
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
        });
        assert!(summary.contains("env=2"));
        assert!(summary.contains("refs=1"));
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
}
