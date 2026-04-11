use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;
use futures::future::join_all;
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

use crate::Cli;
use yode_core::config::Config;
use yode_core::context::AgentContext;
use yode_core::db::{Database, StoredMessage};
use yode_core::permission::PermissionManager;
use yode_core::session::Session;
use yode_core::skills::SkillRegistry;
use yode_llm::types::{ContentBlock, Message, Role, ToolCall};
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
    pub(crate) discovered_skill_count: usize,
    pub(crate) final_tool_count: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct StartupPhaseTiming {
    pub(crate) label: &'static str,
    pub(crate) duration_ms: u64,
}

#[derive(Debug, Clone)]
pub(crate) struct StartupProfiler {
    started_at: Instant,
    last_checkpoint: Instant,
    phases: Vec<StartupPhaseTiming>,
}

impl StartupProfiler {
    pub(crate) fn new() -> Self {
        let now = Instant::now();
        Self {
            started_at: now,
            last_checkpoint: now,
            phases: Vec::new(),
        }
    }

    pub(crate) fn checkpoint(&mut self, label: &'static str) {
        let now = Instant::now();
        self.phases.push(StartupPhaseTiming {
            label,
            duration_ms: now.duration_since(self.last_checkpoint).as_millis() as u64,
        });
        self.last_checkpoint = now;
    }

    fn total_ms(&self) -> u64 {
        self.started_at.elapsed().as_millis() as u64
    }

    pub(crate) fn summary(&self, mode: &'static str, tooling: &ToolingSetupMetrics) -> String {
        let phases = self
            .phases
            .iter()
            .map(|phase| format!("{}={}ms", phase.label, phase.duration_ms))
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "mode={} total={}ms tooling[builtin={}ms mcp_connect={}ms mcp_register={}ms skills={}ms total={}ms counts[builtin={} configured_mcp={} connected_mcp={} mcp_tools={} skills={} final_tools={}]] phases[{}]",
            mode,
            self.total_ms(),
            tooling.builtin_register_ms,
            tooling.mcp_connect_ms,
            tooling.mcp_register_ms,
            tooling.skill_discovery_ms,
            tooling.total_ms,
            tooling.builtin_tool_count,
            tooling.configured_mcp_server_count,
            tooling.connected_mcp_server_count,
            tooling.mcp_tool_count,
            tooling.discovered_skill_count,
            tooling.final_tool_count,
            phases
        )
    }

    pub(crate) fn log_summary(&self, mode: &'static str, tooling: &ToolingSetupMetrics) {
        info!(
            startup_mode = mode,
            total_ms = self.total_ms(),
            builtin_register_ms = tooling.builtin_register_ms,
            mcp_connect_ms = tooling.mcp_connect_ms,
            mcp_register_ms = tooling.mcp_register_ms,
            skill_discovery_ms = tooling.skill_discovery_ms,
            tooling_total_ms = tooling.total_ms,
            builtin_tool_count = tooling.builtin_tool_count,
            configured_mcp_server_count = tooling.configured_mcp_server_count,
            connected_mcp_server_count = tooling.connected_mcp_server_count,
            mcp_tool_count = tooling.mcp_tool_count,
            discovered_skill_count = tooling.discovered_skill_count,
            final_tool_count = tooling.final_tool_count,
            summary = %self.summary(mode, tooling),
            "Startup profile"
        );
    }
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
    let total_started = Instant::now();
    let mut tool_registry = ToolRegistry::new();
    let builtin_started = Instant::now();
    builtin::register_builtin_tools(&mut tool_registry);
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
    while let Some(joined) = mcp_connect_set.join_next().await {
        match joined {
            Ok((name, result)) => match result {
                Ok(client) => {
                    mcp_clients.push(client);
                }
                Err(err) => {
                    warn!(server = %name, error = %err, "Failed to connect to MCP server");
                }
            },
            Err(err) => {
                warn!(error = %err, "MCP connect task failed");
            }
        }
    }
    let mcp_connect_ms = mcp_started.elapsed().as_millis() as u64;
    let connected_mcp_server_count = mcp_clients.len();

    let mcp_register_started = Instant::now();
    let mut mcp_tool_count = 0usize;
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
                for wrapper in wrappers {
                    tool_registry.register(wrapper);
                }
                info!(server = %server_name, tools = count, "MCP server tools registered");
            }
            Err(err) => {
                warn!(server = %server_name, error = %err, "Failed to discover MCP tools");
            }
        }
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
        builtin::register_skill_tool(&mut tool_registry, store);
    }
    info!("Discovered {} skills", skill_registry.list().len());
    let skill_discovery_ms = skill_started.elapsed().as_millis() as u64;
    let discovered_skill_count = skill_registry.list().len();
    let final_tool_count = tool_registry.total_count();

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
            discovered_skill_count,
            final_tool_count,
        },
    })
}

pub(crate) fn configure_permissions(config: &Config) -> PermissionManager {
    let mut permissions =
        PermissionManager::from_confirmation_list(config.tools.require_confirmation.clone());

    if let Some(mode_str) = &config.permissions.default_mode {
        if let Ok(mode) = mode_str.parse::<yode_core::PermissionMode>() {
            permissions.set_mode(mode);
        }
    }

    use yode_core::permission::{PermissionRule, RuleBehavior, RuleSource};
    for entry in &config.permissions.always_allow {
        permissions.add_rule(PermissionRule {
            source: RuleSource::UserConfig,
            behavior: RuleBehavior::Allow,
            tool_name: entry.tool.clone(),
            pattern: entry.pattern.clone(),
        });
    }
    for entry in &config.permissions.always_deny {
        permissions.add_rule(PermissionRule {
            source: RuleSource::UserConfig,
            behavior: RuleBehavior::Deny,
            tool_name: entry.tool.clone(),
            pattern: entry.pattern.clone(),
        });
    }

    permissions
}

pub(crate) fn restore_or_create_context(
    cli: &Cli,
    db: &Database,
    workdir: PathBuf,
    provider_name: String,
    model: String,
) -> Result<(AgentContext, Option<Vec<Message>>)> {
    if let Some(resume_id) = &cli.resume {
        if let Some(session) = db.get_session(resume_id)? {
            info!("Resuming session: {}", resume_id);
            let context = AgentContext::resume(
                session.id.clone(),
                workdir,
                session.provider.clone(),
                session.model.clone(),
            );
            return Ok((context, Some(load_restored_messages(db, resume_id)?)));
        }

        eprintln!("会话 '{}' 未找到，创建新会话。", resume_id);
    }

    Ok((AgentContext::new(workdir, provider_name, model), None))
}

pub(crate) fn ensure_session_exists(db: &Database, context: &AgentContext) -> Result<()> {
    if context.is_resumed {
        return Ok(());
    }

    let session = Session {
        id: context.session_id.clone(),
        name: None,
        provider: context.provider.clone(),
        model: context.model.clone(),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    db.create_session(&session)?;
    Ok(())
}

pub(crate) async fn shutdown_mcp_clients(clients: Vec<yode_mcp::McpClient>) {
    for client in clients {
        if let Err(err) = client.shutdown().await {
            warn!(error = %err, "Error shutting down MCP client");
        }
    }
}

fn load_restored_messages(db: &Database, resume_id: &str) -> Result<Vec<Message>> {
    Ok(db
        .load_messages(resume_id)?
        .into_iter()
        .filter_map(stored_message_to_message)
        .collect())
}

fn stored_message_to_message(message: StoredMessage) -> Option<Message> {
    let role = match message.role.as_str() {
        "user" => Role::User,
        "assistant" => Role::Assistant,
        "tool" => Role::Tool,
        "system" => Role::System,
        _ => return None,
    };
    let tool_calls: Vec<ToolCall> = message
        .tool_calls_json
        .as_deref()
        .and_then(|json| serde_json::from_str(json).ok())
        .unwrap_or_default();
    let mut blocks = Vec::new();
    if let Some(reasoning) = &message.reasoning {
        blocks.push(ContentBlock::Thinking {
            thinking: reasoning.clone(),
            signature: None,
        });
    }
    if let Some(content) = &message.content {
        blocks.push(ContentBlock::Text {
            text: content.clone(),
        });
    }

    Some(
        Message {
            role,
            content: message.content,
            content_blocks: blocks,
            reasoning: message.reasoning,
            tool_calls,
            tool_call_id: message.tool_call_id,
            images: Vec::new(),
        }
        .normalized(),
    )
}
