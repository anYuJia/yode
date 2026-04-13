use crate::commands::context::CommandContext;
use crate::commands::{Command, CommandCategory, CommandMeta, CommandOutput, CommandResult};
use std::collections::{BTreeMap, BTreeSet};
use super::mcp_workspace::{
    auth_session_summary, browser_mcp_capability_summary, latency_sparkline,
    reconnect_backoff_timeline, remote_tool_source_badge, render_browser_access_workspace,
    resource_cache_activity_summary, write_browser_access_state_artifact,
};

pub struct McpCommand {
    meta: CommandMeta,
}

impl McpCommand {
    pub fn new() -> Self {
        Self {
            meta: CommandMeta {
                name: "mcp",
                description: "Summarize MCP servers, auth readiness, and registered tools",
                aliases: &[],
                args: vec![],
                category: CommandCategory::Tools,
                hidden: false,
            },
        }
    }
}

impl Command for McpCommand {
    fn meta(&self) -> &CommandMeta {
        &self.meta
    }

    fn execute(&self, _args: &str, ctx: &mut CommandContext<'_>) -> CommandResult {
        let config = yode_core::config::Config::load().ok();
        let configured_servers = config
            .as_ref()
            .map(|cfg| cfg.mcp.servers.clone())
            .unwrap_or_default();
        let latency_stats = yode_mcp::mcp_tool_latency_stats();
        let reconnect_stats = yode_mcp::mcp_reconnect_diagnostics();
        let mut by_server = BTreeMap::<String, Vec<String>>::new();
        for tool in ctx.tools.definitions() {
            if let Some((server, original_name)) = parse_mcp_tool_name(&tool.name) {
                by_server
                    .entry(server.to_string())
                    .or_default()
                    .push(original_name.to_string());
            }
        }

        let server_names = configured_servers
            .keys()
            .cloned()
            .chain(by_server.keys().cloned())
            .collect::<BTreeSet<_>>();
        if server_names.is_empty() {
            return Ok(CommandOutput::Message(
                "No MCP servers are configured or currently registered.".to_string(),
            ));
        }

        let mut lines = vec![format!("MCP servers ({}):", server_names.len())];
        for server in server_names {
            let mut tools = by_server.remove(&server).unwrap_or_default();
            tools.sort();
            let preview = tools.iter().take(6).cloned().collect::<Vec<_>>().join(", ");
            let more = tools.len().saturating_sub(6);
            let latency = server_latency_summary(&latency_stats, &server);
            let reconnect = server_reconnect_summary(&reconnect_stats, &server);
            let config_state = if let Some(server_config) = configured_servers.get(&server) {
                format!(
                    "configured, auth={}, session={}",
                    auth_status_label(server_config),
                    auth_session_summary(server_config)
                )
            } else {
                "registered-only, auth=unknown".to_string()
            };
            lines.push(format!(
                "  - {} {} [{} | {} tool(s) | latency={} {} | reconnect={} | timeline={}] {}{}",
                remote_tool_source_badge(&format!("mcp__{}_tool", server)),
                server,
                config_state,
                tools.len(),
                latency,
                latency_sparkline(&latency_stats, &server),
                reconnect,
                reconnect_backoff_timeline(&reconnect_stats, &server),
                preview,
                if more > 0 {
                    format!(" (+{} more)", more)
                } else {
                    String::new()
                }
            ));
        }
        lines.push(format!(
            "  Cache stats: {}",
            resource_cache_activity_summary()
        ));
        let browser_tools_present = ctx
            .tools
            .definitions()
            .into_iter()
            .any(|definition| matches!(definition.name.as_str(), "web_search" | "web_fetch" | "web_browser"));
        lines.push(format!(
            "  Capability merge: {}",
            browser_mcp_capability_summary(browser_tools_present, configured_servers.len())
        ));
        if let Some(path) = write_browser_access_state_artifact(
            &std::path::PathBuf::from(&ctx.session.working_dir),
            &ctx.session.session_id,
            browser_tools_present,
            configured_servers.len(),
        ) {
            lines.push(format!("  Browser state artifact: {}", path));
            if let Some(preview) = render_browser_access_workspace(std::path::Path::new(&path)) {
                lines.push(format!("  Browser state preview: {}", preview.replace('\n', " | ")));
            }
        }
        Ok(CommandOutput::Messages(lines))
    }
}

pub fn parse_mcp_tool_name(name: &str) -> Option<(&str, &str)> {
    let rest = name.strip_prefix("mcp__")?;
    let (server, tool) = rest.split_once('_')?;
    Some((server, tool))
}

fn auth_status_label(config: &yode_core::config::McpServerConfig) -> String {
    if config.env.is_empty() {
        return "n/a".to_string();
    }

    let mut missing = Vec::new();
    let mut referenced = false;
    let mut inline = false;
    for value in config.env.values() {
        if let Some(env_name) = value.strip_prefix('$') {
            referenced = true;
            if std::env::var(env_name)
                .ok()
                .map(|value| value.trim().is_empty())
                .unwrap_or(true)
            {
                missing.push(env_name.to_string());
            }
        } else if value.trim().is_empty() {
            missing.push("<inline-empty>".to_string());
        } else {
            inline = true;
        }
    }

    if !missing.is_empty() {
        return format!("missing {}", missing.join(","));
    }
    if referenced && inline {
        "ready(env+inline)".to_string()
    } else if referenced {
        "ready".to_string()
    } else {
        "inline".to_string()
    }
}

fn server_latency_summary(stats: &[yode_mcp::McpToolLatencyEntry], server: &str) -> String {
    let mut matching = stats
        .iter()
        .filter(|entry| entry.server == server)
        .cloned()
        .collect::<Vec<_>>();
    if matching.is_empty() {
        return "no-calls".to_string();
    }

    matching.sort_by(|a, b| b.last_ms.cmp(&a.last_ms));
    let total_calls = matching.iter().map(|entry| entry.calls).sum::<u64>();
    let avg_ms = matching
        .iter()
        .map(|entry| entry.avg_ms.saturating_mul(entry.calls))
        .sum::<u64>()
        .checked_div(total_calls)
        .unwrap_or(0);
    let hottest = matching
        .iter()
        .take(3)
        .map(|entry| format!("{}:{}ms", entry.tool, entry.last_ms))
        .collect::<Vec<_>>()
        .join(", ");
    format!("avg {}ms / {}", avg_ms, hottest)
}

fn server_reconnect_summary(stats: &[yode_mcp::McpReconnectDiagnostic], server: &str) -> String {
    let Some(entry) = stats.iter().find(|entry| entry.server == server) else {
        return "no-attempts".to_string();
    };
    if entry.failures == 0 {
        return format!("stable ({} attempts)", entry.attempts);
    }
    format!(
        "failures={} next={}s{}",
        entry.failures,
        entry.next_backoff_secs,
        entry
            .last_error
            .as_deref()
            .map(|error| format!(" last={}", error))
            .unwrap_or_default()
    )
}

#[cfg(test)]
mod tests {
    use super::{
        auth_status_label, parse_mcp_tool_name, server_latency_summary, server_reconnect_summary,
    };
    use yode_core::config::McpServerConfig;

    #[test]
    fn parse_mcp_tool_name_extracts_server_and_tool() {
        assert_eq!(
            parse_mcp_tool_name("mcp__github_list_prs"),
            Some(("github", "list_prs"))
        );
        assert_eq!(parse_mcp_tool_name("read_file"), None);
    }

    #[test]
    fn auth_status_label_reports_missing_and_ready_envs() {
        let ready = McpServerConfig {
            command: "node".to_string(),
            args: vec![],
            env: std::collections::HashMap::from([(
                "TOKEN".to_string(),
                "inline-secret".to_string(),
            )]),
        };
        assert_eq!(auth_status_label(&ready), "inline");

        let missing = McpServerConfig {
            command: "node".to_string(),
            args: vec![],
            env: std::collections::HashMap::from([(
                "TOKEN".to_string(),
                "$YODE_MISSING_TOKEN".to_string(),
            )]),
        };
        assert!(auth_status_label(&missing).contains("missing YODE_MISSING_TOKEN"));
    }

    #[test]
    fn server_latency_summary_aggregates_recent_stats() {
        let rendered = server_latency_summary(
            &[yode_mcp::McpToolLatencyEntry {
                server: "github".to_string(),
                tool: "list_prs".to_string(),
                calls: 2,
                errors: 0,
                avg_ms: 25,
                max_ms: 40,
                last_ms: 40,
            }],
            "github",
        );

        assert!(rendered.contains("avg 25ms"));
        assert!(rendered.contains("list_prs:40ms"));
    }

    #[test]
    fn server_reconnect_summary_formats_backoff_state() {
        let rendered = server_reconnect_summary(
            &[yode_mcp::McpReconnectDiagnostic {
                server: "github".to_string(),
                attempts: 3,
                failures: 2,
                last_error: Some("timeout".to_string()),
                next_backoff_secs: 4,
            }],
            "github",
        );

        assert!(rendered.contains("failures=2"));
        assert!(rendered.contains("next=4s"));
        assert!(rendered.contains("timeout"));
    }
}
