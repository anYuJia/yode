use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;
use futures::future::join_all;
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

use yode_core::config::Config;
use yode_core::skills::SkillRegistry;
use yode_tools::builtin;
use yode_tools::registry::ToolRegistry;

pub(crate) struct ToolingBootstrap {
    pub(crate) tool_registry: Arc<ToolRegistry>,
    pub(crate) skill_registry: SkillRegistry,
    pub(crate) mcp_clients: Vec<yode_mcp::McpClient>,
    pub(crate) metrics: ToolingSetupMetrics,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ToolingSetupMetrics {
    pub(crate) builtin_register_ms: u64,
    pub(crate) mcp_connect_ms: u64,
    pub(crate) mcp_register_ms: u64,
    pub(crate) skill_discovery_ms: u64,
    pub(crate) total_ms: u64,
    pub(crate) builtin_tool_count: usize,
    pub(crate) configured_mcp_server_count: usize,
    pub(crate) connected_mcp_server_count: usize,
    pub(crate) mcp_tool_count: usize,
    pub(crate) mcp_connect_failures: Vec<String>,
    pub(crate) discovered_skill_count: usize,
    pub(crate) active_tool_count: usize,
    pub(crate) deferred_tool_count: usize,
    pub(crate) deferred_mcp_tool_count: usize,
    pub(crate) tool_search_enabled: bool,
    pub(crate) tool_search_reason: String,
    pub(crate) final_tool_count: usize,
}

pub(crate) fn init_logging() -> Result<()> {
    let log_dir = dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".yode")
        .join("logs");
    std::fs::create_dir_all(&log_dir).ok();

    let log_file = std::fs::File::create(log_dir.join("yode.log"))?;
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("yode=debug".parse()?))
        .with_writer(log_file)
        .with_ansi(false)
        .init();
    Ok(())
}

pub(crate) async fn setup_tooling(config: &Config, workdir: &Path) -> Result<ToolingBootstrap> {
    const SKILL_TOOL_COUNT: usize = 2;

    let total_started = Instant::now();
    let tool_registry = ToolRegistry::new();
    let builtin_started = Instant::now();
    builtin::register_builtin_tools(&tool_registry);
    let builtin_register_ms = builtin_started.elapsed().as_millis() as u64;
    let builtin_tool_count = tool_registry.total_count();

    let skill_paths = SkillRegistry::default_paths(workdir);
    let skill_started = Instant::now();
    let skill_discovery_task =
        tokio::task::spawn_blocking(move || SkillRegistry::discover(&skill_paths));

    let mcp_started = Instant::now();
    let mut mcp_connect_set = tokio::task::JoinSet::new();
    let configured_mcp_server_count = config.mcp.servers.len();
    for (name, server_config) in &config.mcp.servers {
        let name = name.clone();
        let server_config = server_config.clone();
        mcp_connect_set.spawn(async move {
            let mcp_config = yode_mcp::McpServerConfig {
                command: server_config.command,
                args: server_config.args,
                env: server_config.env,
            };
            let result = yode_mcp::McpClient::connect(&name, &mcp_config).await;
            (name, result)
        });
    }

    let mut mcp_clients: Vec<yode_mcp::McpClient> = Vec::new();
    let mut mcp_connect_failures = Vec::new();
    while let Some(joined) = mcp_connect_set.join_next().await {
        match joined {
            Ok((name, result)) => match result {
                Ok(client) => {
                    mcp_clients.push(client);
                }
                Err(err) => {
                    mcp_connect_failures.push(format!("{}: {}", name, err));
                    warn!(server = %name, error = %err, "Failed to connect to MCP server");
                }
            },
            Err(err) => {
                mcp_connect_failures.push(format!("join_error: {}", err));
                warn!(error = %err, "MCP connect task failed");
            }
        }
    }
    let mcp_connect_ms = mcp_started.elapsed().as_millis() as u64;
    let connected_mcp_server_count = mcp_clients.len();

    let mcp_register_started = Instant::now();
    let mut mcp_tool_count = 0usize;
    let mut discovered_mcp_wrappers = Vec::new();
    let discovery_results = join_all(mcp_clients.iter().map(|client| async move {
        let server_name = client.server_name.clone();
        let result = client.discover_wrapped_tools().await;
        (server_name, result)
    }))
    .await;
    for (server_name, result) in discovery_results {
        match result {
            Ok(wrappers) => {
                let count = wrappers.len();
                mcp_tool_count += count;
                discovered_mcp_wrappers.push((server_name, wrappers));
            }
            Err(err) => {
                warn!(server = %server_name, error = %err, "Failed to discover MCP tools");
            }
        }
    }
    let tool_search_enabled = mcp_tool_count > 0
        && tool_registry.should_enable_tool_search_with_additional(mcp_tool_count + SKILL_TOOL_COUNT);
    let tool_search_reason = if mcp_tool_count == 0 {
        "disabled:no_mcp_tools".to_string()
    } else if tool_search_enabled {
        format!(
            "enabled:projected_total_exceeds_threshold(active={} projected_additional={})",
            tool_registry.total_count(),
            mcp_tool_count + SKILL_TOOL_COUNT
        )
    } else {
        format!(
            "disabled:projected_total_below_threshold(active={} projected_additional={})",
            tool_registry.total_count(),
            mcp_tool_count + SKILL_TOOL_COUNT
        )
    };
    tool_registry.set_tool_search_state(tool_search_enabled, tool_search_reason.clone());
    for (server_name, wrappers) in discovered_mcp_wrappers {
        let count = wrappers.len();
        for wrapper in wrappers {
            if tool_search_enabled {
                tool_registry.register_deferred(wrapper);
            } else {
                tool_registry.register(wrapper);
            }
        }
        info!(
            server = %server_name,
            tools = count,
            registration_mode = if tool_search_enabled { "deferred" } else { "active" },
            "MCP server tools registered"
        );
    }
    let mcp_register_ms = mcp_register_started.elapsed().as_millis() as u64;

    let skill_registry = skill_discovery_task.await?;
    {
        use yode_tools::builtin::skill::SkillStore;

        let mut store = SkillStore::new();
        for skill in skill_registry.list() {
            store.add(
                skill.name.clone(),
                skill.description.clone(),
                skill.content.clone(),
            );
        }
        let store = Arc::new(tokio::sync::Mutex::new(store));
        builtin::register_skill_tool(&tool_registry, store);
    }
    info!("Discovered {} skills", skill_registry.list().len());
    let skill_discovery_ms = skill_started.elapsed().as_millis() as u64;
    let discovered_skill_count = skill_registry.list().len();
    let inventory = tool_registry.inventory();
    let final_tool_count = inventory.total_count;

    Ok(ToolingBootstrap {
        tool_registry: Arc::new(tool_registry),
        skill_registry,
        mcp_clients,
        metrics: ToolingSetupMetrics {
            builtin_register_ms,
            mcp_connect_ms,
            mcp_register_ms,
            skill_discovery_ms,
            total_ms: total_started.elapsed().as_millis() as u64,
            builtin_tool_count,
            configured_mcp_server_count,
            connected_mcp_server_count,
            mcp_tool_count,
            mcp_connect_failures,
            discovered_skill_count,
            active_tool_count: inventory.active_count,
            deferred_tool_count: inventory.deferred_count,
            deferred_mcp_tool_count: inventory.mcp_deferred_count,
            tool_search_enabled,
            tool_search_reason,
            final_tool_count,
        },
    })
}
