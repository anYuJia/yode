use std::env;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::Parser;
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

use yode_core::config::Config;
use yode_core::context::AgentContext;
use yode_core::db::Database;
use yode_core::permission::PermissionManager;
use yode_core::session::Session;
use yode_core::skills::SkillRegistry;
use yode_llm::providers::anthropic::AnthropicProvider;
use yode_llm::providers::openai::OpenAiProvider;
use yode_llm::registry::ProviderRegistry;
use yode_llm::types::{Message, Role, ToolCall};
use yode_tools::builtin;
use yode_tools::registry::ToolRegistry;

#[derive(Parser)]
#[command(name = "yode", version, about = "Yode - AI 编程助手")]
struct Cli {
    /// LLM provider to use (openai, anthropic, ollama)
    #[arg(short, long)]
    provider: Option<String>,

    /// Model name
    #[arg(short, long)]
    model: Option<String>,

    /// Config file path
    #[arg(short, long)]
    config: Option<PathBuf>,

    /// Working directory
    #[arg(short, long)]
    workdir: Option<PathBuf>,

    /// Resume a previous session by ID
    #[arg(long)]
    resume: Option<String>,

    /// Run as MCP server (stdio), exposing built-in tools
    #[arg(long)]
    serve_mcp: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging (to file, not terminal)
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

    info!("Yode starting...");

    let cli = Cli::parse();

    // Load config
    let config =
        Config::load_from(cli.config.as_deref()).context("Failed to load configuration")?;

    // Working directory (needed early for skill discovery)
    let workdir = cli
        .workdir
        .unwrap_or_else(|| env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    // Setup tools
    let mut tool_registry = ToolRegistry::new();
    builtin::register_builtin_tools(&mut tool_registry);

    // Connect to configured MCP servers and register their tools
    let mut mcp_clients = Vec::new();
    for (name, server_config) in &config.mcp.servers {
        // Convert core config to mcp config
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
                    Err(e) => {
                        warn!(server = %name, error = %e, "Failed to discover MCP tools");
                    }
                }
                mcp_clients.push(client);
            }
            Err(e) => {
                warn!(server = %name, error = %e, "Failed to connect to MCP server");
            }
        }
    }

    // Discover and register skills
    let skill_paths = SkillRegistry::default_paths(&workdir);
    let skill_registry = SkillRegistry::discover(&skill_paths);
    let _skill_store = {
        use yode_tools::builtin::skill::SkillStore;
        let mut store = SkillStore::new();
        for skill in skill_registry.list() {
            store.add(skill.name.clone(), skill.description.clone(), skill.content.clone());
        }
        let store = std::sync::Arc::new(tokio::sync::Mutex::new(store));
        builtin::register_skill_tool(&mut tool_registry, store.clone());
        store
    };
    info!("Discovered {} skills", skill_registry.list().len());

    let tool_registry = Arc::new(tool_registry);

    // If --serve-mcp, run as MCP server and exit
    if cli.serve_mcp {
        info!("Running in MCP server mode");
        yode_mcp::run_mcp_server(Arc::clone(&tool_registry)).await?;
        return Ok(());
    }

    // Open database
    let db_path = config.session_db_path();
    let db = Database::open(&db_path).context("Failed to open session database")?;

    // Setup LLM provider registry
    let mut provider_registry = ProviderRegistry::new();

    // Register OpenAI provider if API key is available
    if let Ok(api_key) = env::var("OPENAI_API_KEY") {
        let base_url = config
            .llm
            .openai
            .as_ref()
            .map(|c| c.base_url.clone())
            .unwrap_or_else(|| "https://api.openai.com/v1".to_string());
        provider_registry.register(Arc::new(OpenAiProvider::new(api_key, base_url)));
    }

    // Register Anthropic provider if API key is available
    let anthropic_key = env::var("ANTHROPIC_API_KEY")
        .or_else(|_| env::var("ANTHROPIC_AUTH_TOKEN"))
        .ok();
    if let Some(api_key) = anthropic_key {
        let base_url = env::var("ANTHROPIC_BASE_URL").unwrap_or_else(|_| {
            config
                .llm
                .anthropic
                .as_ref()
                .map(|c| c.base_url.clone())
                .unwrap_or_else(|| "https://api.anthropic.com".to_string())
        });
        provider_registry.register(Arc::new(AnthropicProvider::new(api_key, base_url)));
    }

    // Auto-detect provider and model from env vars or CLI args
    let provider_name = cli.provider.unwrap_or_else(|| {
        if env::var("ANTHROPIC_AUTH_TOKEN").is_ok() || env::var("ANTHROPIC_API_KEY").is_ok() {
            "anthropic".to_string()
        } else {
            config.llm.default_provider.clone()
        }
    });

    let model = cli.model.unwrap_or_else(|| {
        env::var("ANTHROPIC_MODEL")
            .ok()
            .filter(|_| provider_name == "anthropic")
            .unwrap_or_else(|| config.llm.default_model.clone())
    });

    let provider = provider_registry.get(&provider_name).context(format!(
        "Provider '{}' not available. Set the appropriate API key environment variable.\n\
         - OpenAI: OPENAI_API_KEY\n\
         - Anthropic: ANTHROPIC_API_KEY or ANTHROPIC_AUTH_TOKEN",
        provider_name
    ))?;

    // Setup permissions
    let permissions = PermissionManager::new(config.tools.require_confirmation.clone());

    // Create or resume context
    let (context, restored_messages) = if let Some(ref resume_id) = cli.resume {
        // Try to resume session
        if let Some(session) = db.get_session(resume_id)? {
            info!("Resuming session: {}", resume_id);
            let ctx = AgentContext::resume(
                session.id.clone(),
                workdir,
                session.provider.clone(),
                session.model.clone(),
            );
            // Load stored messages
            let stored = db.load_messages(resume_id)?;
            let messages: Vec<Message> = stored
                .into_iter()
                .filter_map(|m| {
                    let role = match m.role.as_str() {
                        "user" => Role::User,
                        "assistant" => Role::Assistant,
                        "tool" => Role::Tool,
                        "system" => Role::System,
                        _ => return None,
                    };
                    let tool_calls: Vec<ToolCall> = m
                        .tool_calls_json
                        .as_deref()
                        .and_then(|json| serde_json::from_str(json).ok())
                        .unwrap_or_default();
                    Some(Message {
                        role,
                        content: m.content,
                        tool_calls,
                        tool_call_id: m.tool_call_id,
                    })
                })
                .collect();
            (ctx, Some(messages))
        } else {
            eprintln!("会话 '{}' 未找到，创建新会话。", resume_id);
            (AgentContext::new(workdir, provider_name.clone(), model.clone()), None)
        }
    } else {
        (AgentContext::new(workdir, provider_name.clone(), model.clone()), None)
    };

    // Create session in database if new
    if !context.is_resumed {
        let session = Session {
            id: context.session_id.clone(),
            name: None,
            provider: context.provider.clone(),
            model: context.model.clone(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        db.create_session(&session)?;
    }

    info!(
        "Starting TUI with provider={}, model={}, session={}",
        context.provider, context.model, context.session_id
    );

    // Run TUI
    let skill_cmds: Vec<(String, String)> = skill_registry
        .list()
        .iter()
        .map(|s| (s.name.clone(), s.description.clone()))
        .collect();
    yode_tui::app::run(provider, tool_registry, permissions, context, db, restored_messages, skill_cmds).await?;

    // Shut down MCP clients
    for client in mcp_clients {
        if let Err(e) = client.shutdown().await {
            warn!(error = %e, "Error shutting down MCP client");
        }
    }

    info!("Yode exiting.");
    Ok(())
}
