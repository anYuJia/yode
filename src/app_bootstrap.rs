use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
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
    let mut tool_registry = ToolRegistry::new();
    builtin::register_builtin_tools(&mut tool_registry);

    let mut mcp_clients = Vec::new();
    for (name, server_config) in &config.mcp.servers {
        let mcp_config = yode_mcp::McpServerConfig {
            command: server_config.command.clone(),
            args: server_config.args.clone(),
            env: server_config.env.clone(),
        };
        match yode_mcp::McpClient::connect(name, &mcp_config).await {
            Ok(client) => {
                match client.discover_and_register(&mut tool_registry).await {
                    Ok(count) => {
                        info!(server = %name, tools = count, "MCP server tools registered");
                    }
                    Err(err) => {
                        warn!(server = %name, error = %err, "Failed to discover MCP tools");
                    }
                }
                mcp_clients.push(client);
            }
            Err(err) => {
                warn!(server = %name, error = %err, "Failed to connect to MCP server");
            }
        }
    }

    let skill_paths = SkillRegistry::default_paths(workdir);
    let skill_registry = SkillRegistry::discover(&skill_paths);
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

    Ok(ToolingBootstrap {
        tool_registry: Arc::new(tool_registry),
        skill_registry,
        mcp_clients,
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
