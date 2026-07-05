use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;

use anyhow::Result;

use yode_core::config::Config;
use yode_tools::registry::ToolRegistry;
use yode_tools::tool::McpResourceProvider;

use super::{configuration_runtime::save_config_to_path_async, DesktopRuntime};
use crate::protocol::{DesktopMcpServer, DesktopMcpServerStatus, DesktopMcpState};

impl DesktopRuntime {
    pub fn mcp_servers_state(&self) -> Result<DesktopMcpState> {
        let config = self
            .config
            .lock()
            .map_err(|_| anyhow::anyhow!("config lock poisoned"))?;
        let servers = desktop_mcp_servers_from_config(&config);
        let statuses = mcp_statuses_from_servers(&servers, None);
        Ok(DesktopMcpState {
            config_path: self.user_config_path().display().to_string(),
            servers,
            statuses,
        })
    }

    pub async fn mcp_servers_save(
        &self,
        servers: Vec<DesktopMcpServer>,
    ) -> Result<DesktopMcpState> {
        validate_desktop_mcp_servers(&servers)?;
        let config_to_save = {
            let mut config = self
                .config
                .lock()
                .map_err(|_| anyhow::anyhow!("config lock poisoned"))?;
            config.mcp.servers = desktop_mcp_servers_to_config(&servers)?;
            config.clone()
        };
        save_config_to_path_async(&config_to_save, &self.user_config_path()).await?;
        self.reload_desktop_tooling().await?;
        self.mcp_servers_state()
    }

    pub fn mcp_server_test(&self, server: DesktopMcpServer) -> Result<DesktopMcpServerStatus> {
        validate_desktop_mcp_servers(std::slice::from_ref(&server))?;
        let config = desktop_mcp_server_to_config(&server)?;
        let mcp_config = core_mcp_server_to_runtime(&config);
        tauri::async_runtime::block_on(async move {
            if server.disabled {
                return Ok(DesktopMcpServerStatus {
                    name: server.name,
                    state: "disabled".to_string(),
                    detail: "服务器已禁用。".to_string(),
                    tool_count: 0,
                    resource_count: 0,
                    template_count: 0,
                });
            }
            match yode_mcp::McpClient::connect(&server.name, &mcp_config).await {
                Ok(client) => {
                    let tools = client.discover_wrapped_tools().await;
                    let resources = client.list_resources().await;
                    let templates = client.list_resource_templates().await;
                    if let Err(err) = client.shutdown().await {
                        tracing::warn!(
                            server = %server.name,
                            error = %err,
                            "Failed to shutdown MCP test client"
                        );
                    }
                    Ok(mcp_test_status_from_discovery_results(
                        server.name,
                        tools
                            .map(|items| items.len())
                            .map_err(|err| err.to_string()),
                        resources
                            .map(|items| items.len())
                            .map_err(|err| err.to_string()),
                        templates
                            .map(|items| items.len())
                            .map_err(|err| err.to_string()),
                    ))
                }
                Err(err) => Ok(DesktopMcpServerStatus {
                    name: server.name,
                    state: "failed".to_string(),
                    detail: err.to_string(),
                    tool_count: 0,
                    resource_count: 0,
                    template_count: 0,
                }),
            }
        })
    }

    pub async fn mcp_servers_reload(&self) -> Result<DesktopMcpState> {
        self.reload_desktop_tooling().await?;
        self.mcp_servers_state()
    }

    async fn reload_desktop_tooling(&self) -> Result<()> {
        let config = self
            .config
            .lock()
            .map_err(|_| anyhow::anyhow!("config lock poisoned"))?
            .clone();
        let (tool_registry, mcp_resource_provider) =
            setup_desktop_tooling(&config, &self.workspace_path).await;
        *self
            .tool_registry
            .lock()
            .map_err(|_| anyhow::anyhow!("tool registry lock poisoned"))? = tool_registry;
        *self
            .mcp_resource_provider
            .lock()
            .map_err(|_| anyhow::anyhow!("mcp resource provider lock poisoned"))? =
            mcp_resource_provider;
        Ok(())
    }
}

fn mcp_test_status_from_discovery_results(
    name: String,
    tool_count: std::result::Result<usize, String>,
    resource_count: std::result::Result<usize, String>,
    template_count: std::result::Result<usize, String>,
) -> DesktopMcpServerStatus {
    let mut failures = Vec::new();
    let tool_count = match tool_count {
        Ok(count) => count,
        Err(err) => {
            failures.push(format!("工具枚举失败: {err}"));
            0
        }
    };
    let resource_count = match resource_count {
        Ok(count) => count,
        Err(err) => {
            failures.push(format!("资源枚举失败: {err}"));
            0
        }
    };
    let template_count = match template_count {
        Ok(count) => count,
        Err(err) => {
            failures.push(format!("资源模板枚举失败: {err}"));
            0
        }
    };

    let detail = if failures.is_empty() {
        format!(
            "连接成功，发现 {} 个工具、{} 个资源、{} 个资源模板。",
            tool_count, resource_count, template_count
        )
    } else {
        format!("连接成功，但部分能力枚举失败。{}", failures.join("；"))
    };

    DesktopMcpServerStatus {
        name,
        state: if failures.is_empty() {
            "ready".to_string()
        } else {
            "degraded".to_string()
        },
        detail,
        tool_count,
        resource_count,
        template_count,
    }
}

pub(super) async fn setup_desktop_tooling(
    config: &Config,
    workdir: &Path,
) -> (Arc<ToolRegistry>, Option<Arc<dyn McpResourceProvider>>) {
    let tool_registry = ToolRegistry::new();
    yode_tools::builtin::register_builtin_tools(&tool_registry);

    let mut mcp_clients = Vec::new();
    for (name, server_config) in &config.mcp.servers {
        if server_config.disabled {
            continue;
        }
        let mcp_config = core_mcp_server_to_runtime(server_config);

        match tauri::async_runtime::block_on(async {
            yode_mcp::McpClient::connect(name, &mcp_config).await
        }) {
            Ok(client) => {
                match tauri::async_runtime::block_on(async {
                    client.discover_wrapped_tools().await
                }) {
                    Ok(wrappers) => {
                        for wrapper in wrappers {
                            tool_registry.register(wrapper);
                        }
                    }
                    Err(err) => tracing::warn!(
                        server = %name,
                        error = %err,
                        "Failed to discover MCP tools while loading desktop runtime"
                    ),
                }
                mcp_clients.push(client);
            }
            Err(err) => tracing::warn!(
                server = %name,
                error = %err,
                "Failed to connect MCP server while loading desktop runtime"
            ),
        }
    }

    let skill_paths = yode_core::skills::SkillRegistry::default_paths_async(workdir).await;
    let skill_registry = yode_core::skills::SkillRegistry::discover_async(&skill_paths).await;
    use yode_tools::builtin::skill::{SkillContextMode, SkillEntry, SkillStore};
    let mut store = SkillStore::new();
    for skill in skill_registry.list() {
        let context = match skill.metadata.context {
            yode_core::skills::SkillContextMode::Inline => SkillContextMode::Inline,
            yode_core::skills::SkillContextMode::Fork => SkillContextMode::Fork,
        };
        store.add_entry(SkillEntry {
            name: skill.name.clone(),
            description: skill.description.clone(),
            content: skill.content.clone(),
            allowed_tools: skill.metadata.allowed_tools.clone(),
            paths: skill.metadata.paths.clone(),
            trigger_examples: skill.metadata.trigger_examples.clone(),
            context,
            model: skill.metadata.model.clone(),
            effort: skill.metadata.effort.clone(),
        });
    }
    let store = Arc::new(tokio::sync::Mutex::new(store));
    yode_tools::builtin::register_skill_tool(&tool_registry, store);

    let mcp_resource_provider = if !mcp_clients.is_empty() {
        Some(
            Arc::new(yode_mcp::McpClientResourceProvider::new(mcp_clients))
                as Arc<dyn McpResourceProvider>,
        )
    } else {
        None
    };

    (Arc::new(tool_registry), mcp_resource_provider)
}

fn desktop_mcp_servers_from_config(config: &Config) -> Vec<DesktopMcpServer> {
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

fn desktop_mcp_servers_to_config(
    servers: &[DesktopMcpServer],
) -> Result<HashMap<String, yode_core::config::McpServerConfig>> {
    let mut map = HashMap::new();
    for server in servers {
        map.insert(server.name.clone(), desktop_mcp_server_to_config(server)?);
    }
    Ok(map)
}

fn desktop_mcp_server_to_config(
    server: &DesktopMcpServer,
) -> Result<yode_core::config::McpServerConfig> {
    Ok(yode_core::config::McpServerConfig {
        disabled: server.disabled,
        transport: parse_mcp_transport(&server.transport)?,
        command: server.command.clone().unwrap_or_default(),
        args: server.args.clone(),
        env: server.env.clone(),
        url: server.url.clone().filter(|url| !url.trim().is_empty()),
        auth: None,
    })
}

fn validate_desktop_mcp_servers(servers: &[DesktopMcpServer]) -> Result<()> {
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

fn core_mcp_server_to_runtime(
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

fn mcp_statuses_from_servers(
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
    use super::*;

    #[test]
    fn mcp_test_status_reports_partial_discovery_failures() {
        let status = mcp_test_status_from_discovery_results(
            "demo".to_string(),
            Ok(2),
            Err("resources unavailable".to_string()),
            Ok(1),
        );

        assert_eq!(status.name, "demo");
        assert_eq!(status.state, "degraded");
        assert_eq!(status.tool_count, 2);
        assert_eq!(status.resource_count, 0);
        assert_eq!(status.template_count, 1);
        assert!(status.detail.contains("连接成功，但部分能力枚举失败"));
        assert!(status
            .detail
            .contains("资源枚举失败: resources unavailable"));
    }

    #[test]
    fn mcp_test_status_reports_ready_when_all_discovery_succeeds() {
        let status =
            mcp_test_status_from_discovery_results("demo".to_string(), Ok(3), Ok(2), Ok(1));

        assert_eq!(status.state, "ready");
        assert_eq!(
            status.detail,
            "连接成功，发现 3 个工具、2 个资源、1 个资源模板。"
        );
    }
}
