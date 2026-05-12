use super::mcp_workspace::{
    auth_session_summary, browser_mcp_capability_summary, latency_sparkline,
    mcp_resource_artifact_summary, reconnect_backoff_timeline, remote_tool_source_badge,
    render_browser_access_workspace, resource_cache_activity_summary,
    write_browser_access_state_artifact,
};
use crate::commands::context::CommandContext;
use crate::commands::info::startup_artifacts::{
    latest_managed_mcp_inventory, latest_settings_scopes,
};
use crate::commands::{
    ArgCompletionSource, ArgDef, Command, CommandCategory, CommandMeta, CommandOutput,
    CommandResult,
};
use std::collections::{BTreeMap, BTreeSet};

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
                args: vec![ArgDef {
                    name: "view".to_string(),
                    required: false,
                    hint: "resources cleanup [keep=N|all]".to_string(),
                    completions: ArgCompletionSource::Static(vec![
                        "resources cleanup".to_string(),
                        "resources cleanup keep=20".to_string(),
                        "resources cleanup all".to_string(),
                    ]),
                }],
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

    fn execute(&self, args: &str, ctx: &mut CommandContext<'_>) -> CommandResult {
        let args = args.trim();
        if let Some(cleanup_args) = resource_cleanup_args(args) {
            return cleanup_mcp_resources(cleanup_args, ctx);
        }

        let config = yode_core::config::Config::load().ok();
        let configured_servers = config
            .as_ref()
            .map(|cfg| cfg.mcp.servers.clone())
            .unwrap_or_default();
        let latency_stats = yode_mcp::mcp_tool_latency_stats();
        let reconnect_stats = yode_mcp::mcp_reconnect_diagnostics();
        let elicitation_stats = yode_mcp::mcp_elicitation_diagnostics();
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
            let elicitation = server_elicitation_summary(&elicitation_stats, &server);
            let config_state = if let Some(server_config) = configured_servers.get(&server) {
                format!(
                    "configured, transport={}({}), endpoint={}, auth={}, session={}",
                    server_config.transport.label(),
                    transport_execution_label(server_config.transport),
                    mcp_endpoint_label(server_config),
                    auth_status_label(&server, server_config),
                    auth_session_summary(server_config)
                )
            } else {
                "registered-only, auth=unknown".to_string()
            };
            lines.push(format!(
                "  - {} {} [{} | {} tool(s) | latency={} {} | reconnect={} | elicitation={} | timeline={}] {}{}",
                remote_tool_source_badge(&format!("mcp__{}_tool", server)),
                server,
                config_state,
                tools.len(),
                latency,
                latency_sparkline(&latency_stats, &server),
                reconnect,
                elicitation,
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
        let project_root = std::path::PathBuf::from(&ctx.session.working_dir);
        lines.push(format!(
            "  Resource artifacts: {}",
            mcp_resource_artifact_summary(&project_root)
        ));
        let browser_tools_present = ctx.tools.definitions().into_iter().any(|definition| {
            matches!(
                definition.name.as_str(),
                "web_search" | "web_fetch" | "web_browser"
            )
        });
        lines.push(format!(
            "  Capability merge: {}",
            browser_mcp_capability_summary(browser_tools_present, configured_servers.len())
        ));
        if let Some(scopes) = latest_settings_scopes(&project_root) {
            let scope_summary = scopes
                .scopes
                .iter()
                .map(|scope| {
                    format!(
                        "{}(exists={} mode={} rules={} mcp={} path={})",
                        scope.scope,
                        scope.exists,
                        scope
                            .permission_default_mode
                            .as_deref()
                            .unwrap_or("inherit"),
                        scope.permission_rule_count,
                        scope.mcp_server_count,
                        scope.path
                    )
                })
                .collect::<Vec<_>>()
                .join(" | ");
            lines.push(format!(
                "  Settings scopes: {} [{}]",
                scope_summary,
                scopes.path.display()
            ));
        }
        if let Some(managed_inventory) = latest_managed_mcp_inventory(&project_root) {
            lines.push(format!(
                "  Managed MCP: effective={} configured={} connected={} tools={} failures={} search={} reason={} deferred={} deferred_total={} activations={} last={} [{}]",
                managed_inventory.effective_server_count,
                managed_inventory.configured_server_count,
                managed_inventory.connected_server_count,
                managed_inventory.mcp_tool_count,
                managed_inventory.failure_count,
                if let Some(tool_search) = crate::commands::info::startup_artifacts::latest_tool_search_activation(&project_root) {
                    tool_search.tool_search_enabled
                } else {
                    false
                },
                crate::commands::info::startup_artifacts::latest_tool_search_activation(&project_root)
                    .map(|summary| summary.tool_search_reason)
                    .unwrap_or_else(|| "none".to_string()),
                crate::commands::info::startup_artifacts::latest_tool_search_activation(&project_root)
                    .map(|summary| summary.deferred_mcp_tool_count)
                    .unwrap_or(0),
                crate::commands::info::startup_artifacts::latest_tool_search_activation(&project_root)
                    .map(|summary| summary.deferred_tool_count)
                    .unwrap_or(0),
                crate::commands::info::startup_artifacts::latest_tool_search_activation(&project_root)
                    .map(|summary| summary.activation_count)
                    .unwrap_or(0),
                crate::commands::info::startup_artifacts::latest_tool_search_activation(&project_root)
                    .and_then(|summary| summary.last_activated_tool)
                    .unwrap_or_else(|| "none".to_string()),
                managed_inventory.path.display()
            ));
        }
        if !configured_servers.is_empty() && latency_stats.is_empty() {
            lines.push(
                "  Remediation: configured MCP servers have no recent tool latency; inspect `/inspect artifact latest-managed-mcp-inventory` and `/inspect artifact latest-settings-scopes`."
                    .to_string(),
            );
        }
        if reconnect_stats.iter().any(|entry| entry.failures > 0) {
            lines.push(
                "  Remediation: reconnect failures detected; inspect `/inspect artifact latest-mcp-failures` and compare scope policy in `/inspect artifact latest-settings-scopes`."
                    .to_string(),
            );
        }
        if let Some(path) = write_browser_access_state_artifact(
            &project_root,
            &ctx.session.session_id,
            browser_tools_present,
            configured_servers.len(),
        ) {
            lines.push(format!("  Browser state artifact: {}", path));
            if let Some(preview) = render_browser_access_workspace(std::path::Path::new(&path)) {
                lines.push(format!(
                    "  Browser state preview: {}",
                    preview.replace('\n', " | ")
                ));
            }
        }
        Ok(CommandOutput::Messages(lines))
    }
}

fn resource_cleanup_args(args: &str) -> Option<&str> {
    if args == "resources cleanup" {
        return Some("");
    }
    args.strip_prefix("resources cleanup ")
}

fn cleanup_mcp_resources(args: &str, ctx: &CommandContext<'_>) -> CommandResult {
    let keep = parse_resource_cleanup_keep(args.trim())?;
    let project_root = std::path::PathBuf::from(&ctx.session.working_dir);
    let summary = yode_tools::cleanup_mcp_resource_artifacts(&project_root, keep)
        .map_err(|err| err.to_string())?;
    Ok(CommandOutput::Message(format!(
        "MCP resource artifact cleanup: removed={} kept={} retention={} dir={}",
        summary.removed,
        summary.kept,
        keep,
        summary.dir.display()
    )))
}

fn parse_resource_cleanup_keep(args: &str) -> Result<usize, String> {
    if args.is_empty() {
        return Ok(yode_tools::mcp_resource_artifact_retention());
    }
    if args == "all" || args == "keep=0" {
        return Ok(0);
    }
    let value = args
        .strip_prefix("keep=")
        .ok_or_else(|| "Usage: /mcp resources cleanup [keep=N|all]".to_string())?;
    value
        .parse::<usize>()
        .map_err(|_| "Usage: /mcp resources cleanup [keep=N|all]".to_string())
}

pub fn parse_mcp_tool_name(name: &str) -> Option<(&str, &str)> {
    let rest = name.strip_prefix("mcp__")?;
    let (server, tool) = rest.split_once('_')?;
    Some((server, tool))
}

fn auth_status_label(server: &str, config: &yode_core::config::McpServerConfig) -> String {
    if let Some(auth) = &config.auth {
        if let Some(env_name) = auth.bearer_token_env.as_deref() {
            return if std::env::var(env_name)
                .ok()
                .map(|value| value.trim().is_empty())
                .unwrap_or(true)
            {
                format!("missing {}", env_name)
            } else {
                format!("bearer-env {}", env_name)
            };
        }
        if auth.oauth.is_some() {
            return if oauth_token_saved(server) {
                "oauth-token saved".to_string()
            } else {
                "oauth-token missing".to_string()
            };
        }
    }

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

fn oauth_token_saved(server: &str) -> bool {
    let path = dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".yode")
        .join("mcp-auth")
        .join(format!("{}.token.json", sanitize_server_name(server)));
    let Ok(contents) = std::fs::read_to_string(path) else {
        return false;
    };
    serde_json::from_str::<serde_json::Value>(&contents)
        .ok()
        .and_then(|value| {
            value
                .get("access_token")
                .and_then(|token| token.as_str())
                .map(|token| !token.trim().is_empty())
        })
        .unwrap_or(false)
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

fn mcp_endpoint_label(config: &yode_core::config::McpServerConfig) -> String {
    match config.transport {
        yode_core::config::McpTransportConfig::Stdio => {
            if config.command.trim().is_empty() {
                "missing-command".to_string()
            } else {
                config.command.clone()
            }
        }
        _ => config
            .url
            .clone()
            .unwrap_or_else(|| "missing-url".to_string()),
    }
}

fn transport_execution_label(transport: yode_core::config::McpTransportConfig) -> &'static str {
    match transport {
        yode_core::config::McpTransportConfig::Stdio => "executable",
        yode_core::config::McpTransportConfig::Sse
        | yode_core::config::McpTransportConfig::Http => "executable",
        yode_core::config::McpTransportConfig::Websocket => "parsed-only",
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

    matching.sort_by_key(|b| std::cmp::Reverse(b.last_ms));
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

fn server_elicitation_summary(
    stats: &[yode_mcp::McpElicitationDiagnostic],
    server: &str,
) -> String {
    let Some(entry) = stats.iter().find(|entry| entry.server == server) else {
        return "none".to_string();
    };
    let mut detail = format!(
        "requests={} declined={} form={} url={}",
        entry.requests, entry.declined, entry.form_requests, entry.url_requests
    );
    if let Some(message) = entry
        .last_message
        .as_deref()
        .filter(|message| !message.is_empty())
    {
        detail.push_str(&format!(" last={}", message.replace('\n', " ")));
    }
    if let Some(url) = entry.last_url.as_deref().filter(|url| !url.is_empty()) {
        detail.push_str(&format!(" url={}", url));
    }
    detail
}

#[cfg(test)]
mod tests {
    use super::{
        auth_status_label, oauth_token_saved, parse_mcp_tool_name, parse_resource_cleanup_keep,
        resource_cleanup_args, sanitize_server_name, server_elicitation_summary,
        server_latency_summary, server_reconnect_summary, transport_execution_label,
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
            ..McpServerConfig::default()
        };
        assert_eq!(auth_status_label("demo", &ready), "inline");

        let missing = McpServerConfig {
            command: "node".to_string(),
            args: vec![],
            env: std::collections::HashMap::from([(
                "TOKEN".to_string(),
                "$YODE_MISSING_TOKEN".to_string(),
            )]),
            ..McpServerConfig::default()
        };
        assert!(auth_status_label("demo", &missing).contains("missing YODE_MISSING_TOKEN"));
    }

    #[test]
    fn auth_status_label_reports_remote_bearer_env() {
        let configured = McpServerConfig {
            transport: yode_core::config::McpTransportConfig::Sse,
            url: Some("https://example.com/mcp".to_string()),
            auth: Some(yode_core::config::McpAuthConfig {
                bearer_token_env: Some("PATH".to_string()),
                ..yode_core::config::McpAuthConfig::default()
            }),
            ..McpServerConfig::default()
        };

        assert_eq!(auth_status_label("demo", &configured), "bearer-env PATH");
    }

    #[test]
    fn auth_status_label_reports_missing_oauth_token() {
        let configured = McpServerConfig {
            transport: yode_core::config::McpTransportConfig::Http,
            url: Some("https://example.com/mcp".to_string()),
            auth: Some(yode_core::config::McpAuthConfig {
                oauth: Some(yode_core::config::McpOAuthConfig {
                    client_id: Some("client".to_string()),
                    authorization_url: Some("https://example.com/auth".to_string()),
                    token_url: Some("https://example.com/token".to_string()),
                    scopes: vec![],
                }),
                ..yode_core::config::McpAuthConfig::default()
            }),
            ..McpServerConfig::default()
        };

        assert_eq!(
            auth_status_label("missing-token-test", &configured),
            "oauth-token missing"
        );
    }

    #[test]
    fn oauth_token_helpers_match_saved_token_shape() {
        assert_eq!(sanitize_server_name("github/prod"), "github_prod");
        assert!(!oauth_token_saved("definitely-missing-token"));
    }

    #[test]
    fn transport_execution_label_marks_supported_remote_transports_executable() {
        assert_eq!(
            transport_execution_label(yode_core::config::McpTransportConfig::Stdio),
            "executable"
        );
        assert_eq!(
            transport_execution_label(yode_core::config::McpTransportConfig::Sse),
            "executable"
        );
        assert_eq!(
            transport_execution_label(yode_core::config::McpTransportConfig::Http),
            "executable"
        );
        assert_eq!(
            transport_execution_label(yode_core::config::McpTransportConfig::Websocket),
            "parsed-only"
        );
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

    #[test]
    fn server_elicitation_summary_formats_declined_requests() {
        let rendered = server_elicitation_summary(
            &[yode_mcp::McpElicitationDiagnostic {
                server: "github".to_string(),
                requests: 2,
                form_requests: 1,
                url_requests: 1,
                declined: 2,
                last_message: Some("Authorize access".to_string()),
                last_url: Some("https://example.com/auth".to_string()),
            }],
            "github",
        );

        assert!(rendered.contains("requests=2"));
        assert!(rendered.contains("declined=2"));
        assert!(rendered.contains("form=1"));
        assert!(rendered.contains("url=1"));
        assert!(rendered.contains("Authorize access"));
    }

    #[test]
    fn resource_cleanup_args_parse_keep_policy() {
        assert!(parse_resource_cleanup_keep("").unwrap() > 0);
        assert_eq!(parse_resource_cleanup_keep("all").unwrap(), 0);
        assert_eq!(parse_resource_cleanup_keep("keep=0").unwrap(), 0);
        assert_eq!(parse_resource_cleanup_keep("keep=7").unwrap(), 7);
        assert!(parse_resource_cleanup_keep("bad").is_err());
    }

    #[test]
    fn resource_cleanup_subcommand_requires_word_boundary() {
        assert_eq!(resource_cleanup_args("resources cleanup"), Some(""));
        assert_eq!(resource_cleanup_args("resources cleanup all"), Some("all"));
        assert_eq!(
            resource_cleanup_args("resources cleanup keep=7"),
            Some("keep=7")
        );
        assert_eq!(resource_cleanup_args("resources cleanupall"), None);
        assert_eq!(resource_cleanup_args("resources clean"), None);
    }
}
