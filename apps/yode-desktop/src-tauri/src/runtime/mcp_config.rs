use std::collections::{HashMap, HashSet};

use anyhow::Result;

use crate::protocol::{DesktopMcpServer, DesktopMcpServerStatus};

pub(super) fn desktop_mcp_servers_from_config(
    config: &yode_core::config::Config,
) -> Vec<DesktopMcpServer> {
    let mut servers = config
        .mcp
        .servers
        .iter()
        .map(|(name, server)| DesktopMcpServer {
            name: name.clone(),
            transport: server.transport.label().to_string(),
            command: (!server.command.is_empty()).then(|| server.command.clone()),
            args: server.args.clone(),
            url: server.url.clone(),
            env: server.env.clone(),
            disabled: server.disabled,
        })
        .collect::<Vec<_>>();
    servers.sort_by(|a, b| a.name.cmp(&b.name));
    servers
}

pub(super) fn desktop_mcp_servers_to_config(
    servers: &[DesktopMcpServer],
    existing: &HashMap<String, yode_core::config::McpServerConfig>,
) -> Result<HashMap<String, yode_core::config::McpServerConfig>> {
    let mut map = HashMap::new();
    for server in servers {
        map.insert(
            server.name.clone(),
            desktop_mcp_server_to_config(server, existing.get(&server.name))?,
        );
    }
    Ok(map)
}

pub(super) fn desktop_mcp_server_to_config(
    server: &DesktopMcpServer,
    existing: Option<&yode_core::config::McpServerConfig>,
) -> Result<yode_core::config::McpServerConfig> {
    Ok(yode_core::config::McpServerConfig {
        disabled: server.disabled,
        transport: parse_mcp_transport(&server.transport)?,
        command: server.command.clone().unwrap_or_default(),
        args: server.args.clone(),
        env: server.env.clone(),
        url: server.url.clone().filter(|url| !url.trim().is_empty()),
        // 前端表单不含 auth 字段，保存时必须保留既有配置中的 auth，
        // 否则每次保存设置都会丢失 MCP OAuth / bearer_token_env。
        auth: existing.and_then(|entry| entry.auth.clone()),
    })
}

pub(super) fn validate_desktop_mcp_servers(servers: &[DesktopMcpServer]) -> Result<()> {
    let mut names = HashSet::new();
    for server in servers {
        let name = server.name.trim();
        if name.is_empty() {
            anyhow::bail!("MCP 服务器名称不能为空。");
        }
        if !name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.'))
        {
            anyhow::bail!("MCP 服务器名称只能包含字母、数字、_、- 或 .。");
        }
        if !names.insert(name.to_string()) {
            anyhow::bail!("MCP 服务器名称 '{}' 已存在。", name);
        }
        let transport = parse_mcp_transport(&server.transport)?;
        match transport {
            yode_core::config::McpTransportConfig::Stdio => {
                if server
                    .command
                    .as_deref()
                    .unwrap_or_default()
                    .trim()
                    .is_empty()
                {
                    anyhow::bail!("stdio MCP 服务器 '{}' 需要执行指令。", name);
                }
            }
            _ => {
                if server.url.as_deref().unwrap_or_default().trim().is_empty() {
                    anyhow::bail!("远程 MCP 服务器 '{}' 需要 URL。", name);
                }
            }
        }
    }
    Ok(())
}

fn parse_mcp_transport(transport: &str) -> Result<yode_core::config::McpTransportConfig> {
    match transport.to_ascii_lowercase().as_str() {
        "stdio" => Ok(yode_core::config::McpTransportConfig::Stdio),
        "sse" => Ok(yode_core::config::McpTransportConfig::Sse),
        "http" => Ok(yode_core::config::McpTransportConfig::Http),
        "websocket" => Ok(yode_core::config::McpTransportConfig::Websocket),
        other => anyhow::bail!("不支持的 MCP transport: {}", other),
    }
}

pub(super) fn core_mcp_server_to_runtime(
    server_config: &yode_core::config::McpServerConfig,
) -> yode_mcp::McpServerConfig {
    yode_mcp::McpServerConfig {
        disabled: server_config.disabled,
        transport: match server_config.transport {
            yode_core::config::McpTransportConfig::Stdio => yode_mcp::McpTransportConfig::Stdio,
            yode_core::config::McpTransportConfig::Sse => yode_mcp::McpTransportConfig::Sse,
            yode_core::config::McpTransportConfig::Http => yode_mcp::McpTransportConfig::Http,
            yode_core::config::McpTransportConfig::Websocket => {
                yode_mcp::McpTransportConfig::Websocket
            }
        },
        command: server_config.command.clone(),
        args: server_config.args.clone(),
        env: server_config.env.clone(),
        url: server_config.url.clone(),
        auth: server_config
            .auth
            .as_ref()
            .map(|auth| yode_mcp::McpAuthConfig {
                oauth: auth.oauth.as_ref().map(|oauth| yode_mcp::McpOAuthConfig {
                    client_id: oauth.client_id.clone(),
                    authorization_url: oauth.authorization_url.clone(),
                    token_url: oauth.token_url.clone(),
                    scopes: oauth.scopes.clone(),
                }),
                bearer_token_env: auth.bearer_token_env.clone(),
            }),
    }
}

pub(super) fn mcp_statuses_from_servers(
    servers: &[DesktopMcpServer],
    tested: Option<&DesktopMcpServerStatus>,
) -> Vec<DesktopMcpServerStatus> {
    servers
        .iter()
        .map(|server| {
            if let Some(status) = tested.filter(|status| status.name == server.name) {
                return status.clone();
            }
            if server.disabled {
                DesktopMcpServerStatus {
                    name: server.name.clone(),
                    state: "disabled".to_string(),
                    detail: "服务器已禁用。".to_string(),
                    tool_count: 0,
                    resource_count: 0,
                    template_count: 0,
                }
            } else {
                DesktopMcpServerStatus {
                    name: server.name.clone(),
                    state: "configured".to_string(),
                    detail: "已保存到配置；可测试连接或重载运行时。".to_string(),
                    tool_count: 0,
                    resource_count: 0,
                    template_count: 0,
                }
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use yode_core::config::{McpAuthConfig, McpServerConfig};

    use super::{desktop_mcp_server_to_config, desktop_mcp_servers_to_config};
    use crate::protocol::DesktopMcpServer;

    fn server_with_auth() -> DesktopMcpServer {
        DesktopMcpServer {
            name: "docs".to_string(),
            transport: "stdio".to_string(),
            command: Some("npx".to_string()),
            args: vec!["-y".to_string(), "mcp-docs".to_string()],
            url: None,
            env: HashMap::from([("FOO".to_string(), "bar".to_string())]),
            disabled: false,
        }
    }

    fn existing_config() -> HashMap<String, McpServerConfig> {
        HashMap::from([(
            "docs".to_string(),
            McpServerConfig {
                disabled: false,
                transport: yode_core::config::McpTransportConfig::Stdio,
                command: "npx".to_string(),
                args: vec![],
                env: HashMap::new(),
                url: None,
                auth: Some(McpAuthConfig {
                    oauth: None,
                    bearer_token_env: Some("DOCS_TOKEN".to_string()),
                }),
            },
        )])
    }

    #[test]
    fn mcp_save_preserves_existing_auth() {
        let converted =
            desktop_mcp_server_to_config(&server_with_auth(), existing_config().get("docs"))
                .unwrap();
        assert_eq!(
            converted
                .auth
                .as_ref()
                .and_then(|auth| auth.bearer_token_env.as_deref()),
            Some("DOCS_TOKEN")
        );
        assert_eq!(converted.env.get("FOO").map(String::as_str), Some("bar"));
    }

    #[test]
    fn mcp_save_keeps_other_servers_untouched() {
        let mut existing = existing_config();
        existing.insert(
            "other".to_string(),
            McpServerConfig {
                disabled: true,
                transport: yode_core::config::McpTransportConfig::Http,
                command: String::new(),
                args: vec![],
                env: HashMap::new(),
                url: Some("https://example.com/mcp".to_string()),
                auth: None,
            },
        );
        let saved = desktop_mcp_servers_to_config(&[server_with_auth()], &existing).unwrap();
        // 未被前端表单覆盖的服务器从配置中移除，但既有服务器保留 auth
        assert_eq!(saved.len(), 1);
        assert!(saved["docs"].auth.is_some());
    }

    #[test]
    fn mcp_save_without_existing_entry_has_no_auth() {
        let converted = desktop_mcp_server_to_config(&server_with_auth(), None).unwrap();
        assert!(converted.auth.is_none());
    }
}
