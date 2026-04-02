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
use yode_core::setup::{has_api_keys_configured, run_setup_interactive};
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

    /// Non-interactive chat: send a message and print the response
    #[arg(long = "chat", short = 'C')]
    chat_message: Option<String>,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(clap::Subcommand)]
enum Commands {
    /// 管理 LLM 提供商 (Add, List, Remove, SetDefault)
    Provider {
        #[command(subcommand)]
        action: ProviderAction,
    },
}

#[derive(clap::Subcommand)]
enum ProviderAction {
    /// 新增或覆盖自定义提供商
    Add,
    /// 列出所有配置的提供商
    List,
    /// 删除某个提供商
    Remove { name: String },
    /// 设置新的默认提供商
    SetDefault { name: String },
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

    // Check if API keys are configured, if not run setup
    if !has_api_keys_configured() {
        run_setup_interactive()?;
    }

    // Load config
    let mut config =
        Config::load_from(cli.config.as_deref()).context("Failed to load configuration")?;

    if let Some(command) = cli.command {
        match command {
            Commands::Provider { action } => {
                match action {
                    ProviderAction::Add => {
                        run_setup_interactive()?;
                    }
                    ProviderAction::List => {
                        println!("已配置的提供商列表:");
                        for (name, p) in &config.llm.providers {
                            let is_default = if name == &config.llm.default_provider { " (当前默认)" } else { "" };
                            println!("- {}{} [格式: {}, Base URL: {}]", name, is_default, p.format, p.base_url.as_deref().unwrap_or(""));
                        }
                    }
                    ProviderAction::Remove { name } => {
                        if config.llm.providers.remove(&name).is_some() {
                            config.save()?;
                            println!("已删除提供商: {}", name);
                        } else {
                            println!("未找到名为 '{}' 的提供商", name);
                        }
                    }
                    ProviderAction::SetDefault { name } => {
                        if config.llm.providers.contains_key(&name) {
                            config.llm.default_provider = name.clone();
                            config.save()?;
                            println!("已将 '{}' 设置为默认提供商", name);
                        } else {
                            println!("未找到名为 '{}' 的提供商", name);
                        }
                    }
                }
                return Ok(());
            }
        }
    }

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

    // Register all configured providers
    for (name, p_config) in &config.llm.providers {
        let env_prefix = name.to_uppercase().replace("-", "_");
        let api_key = match env::var(format!("{}_API_KEY", env_prefix))
            .ok()
            .or_else(|| p_config.api_key.clone())
            .or_else(|| {
                // Fallback to legacy global env vars if none found for custom provider
                if p_config.format == "openai" {
                    env::var("OPENAI_API_KEY").ok()
                } else {
                    env::var("ANTHROPIC_API_KEY").or_else(|_| env::var("ANTHROPIC_AUTH_TOKEN")).ok()
                }
            }) {
            Some(k) => k,
            None => {
                warn!("Provider '{}' is configured but missing an API key, skipping.", name);
                continue;
            }
        };

        let default_base = if p_config.format == "openai" {
            "https://api.openai.com/v1"
        } else {
            "https://api.anthropic.com"
        };

        let base_url = match env::var(format!("{}_BASE_URL", env_prefix))
            .ok()
            .or_else(|| p_config.base_url.clone()) {
            Some(u) => if u.is_empty() { default_base.to_string() } else { u },
            None => default_base.to_string(),
        };

        if p_config.format == "openai" {
            provider_registry.register(Arc::new(OpenAiProvider::new(name, api_key, base_url)));
        } else {
            provider_registry.register(Arc::new(AnthropicProvider::new(name, api_key, base_url)));
        }
    }

    // Auto-detect provider and model from env vars or CLI args
    let provider_name = cli.provider.unwrap_or_else(|| config.llm.default_provider.clone());

    // Build provider → models map from config
    let all_provider_models: std::collections::HashMap<String, Vec<String>> = config.llm.providers.iter()
        .filter(|(name, _)| provider_registry.get(name).is_some())
        .map(|(name, p_config)| (name.clone(), p_config.models.clone()))
        .collect();

    // Resolve model: CLI arg > config default > first model in provider's list
    let provider_models = all_provider_models.get(&provider_name).cloned().unwrap_or_default();
    let model = {
        let requested = cli.model.unwrap_or_else(|| config.llm.default_model.clone());
        if !provider_models.is_empty() && !provider_models.contains(&requested) {
            let first = provider_models[0].clone();
            warn!(
                "Model '{}' not in provider '{}' model list, using '{}' instead. Available: {:?}",
                requested, provider_name, first, provider_models
            );
            eprintln!(
                "⚠ Model '{}' not available for provider '{}', using '{}' instead.",
                requested, provider_name, first
            );
            first
        } else {
            requested
        }
    };

    let provider_registry = Arc::new(provider_registry);

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
                        images: Vec::new(),
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

    // If --chat, run a single non-interactive turn and exit
    if let Some(ref chat_msg) = cli.chat_message {
        use yode_core::engine::{AgentEngine, ConfirmResponse, EngineEvent};

        let mut engine = AgentEngine::new(
            Arc::clone(&provider),
            Arc::clone(&tool_registry),
            permissions,
            context,
        );
        engine.set_database(db);
        if let Some(msgs) = restored_messages {
            engine.restore_messages(msgs);
        }

        let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel();
        let (confirm_tx, confirm_rx) = tokio::sync::mpsc::unbounded_channel();

        // 用流式模式执行（兼容只支持流式的第三方 API）
        let chat_msg_owned = chat_msg.clone();
        let engine_handle = tokio::spawn(async move {
            engine.run_turn_streaming(&chat_msg_owned, event_tx, confirm_rx, None).await
        });

        // 实时打印输出
        let mut full_text = String::new();
        while let Some(event) = event_rx.recv().await {
            match event {
                EngineEvent::TextDelta(delta) => {
                    print!("{}", delta);
                    full_text.push_str(&delta);
                }
                EngineEvent::TextComplete(_) => {}
                EngineEvent::ToolCallStart { name, arguments, .. } => {
                    eprintln!("\x1b[90m⚡ {}({})\x1b[0m", name, truncate_str(&arguments, 80));
                }
                EngineEvent::ToolConfirmRequired { id, name, .. } => {
                    // 非交互模式自动允许工具执行
                    eprintln!("\x1b[33m🔑 自动确认工具: {}\x1b[0m", name);
                    let _ = confirm_tx.send(ConfirmResponse::Allow);
                    let _ = id;
                }
                EngineEvent::ToolResult { name, result, .. } => {
                    if result.is_error {
                        eprintln!("\x1b[31m✗ {} 失败: {}\x1b[0m", name, truncate_str(&result.content, 200));
                    } else {
                        eprintln!("\x1b[90m✓ {} 完成 ({} 字节)\x1b[0m", name, result.content.len());
                    }
                }
                EngineEvent::Error(e) => {
                    eprintln!("\x1b[31m错误: {}\x1b[0m", e);
                }
                EngineEvent::Done => break,
                _ => {}
            }
        }

        // 确保输出换行
        if !full_text.is_empty() && !full_text.ends_with('\n') {
            println!();
        }

        // 等待引擎完成
        if let Err(e) = engine_handle.await? {
            eprintln!("\x1b[31m引擎错误: {}\x1b[0m", e);
        }

        // 关闭 MCP 客户端
        for client in mcp_clients {
            if let Err(e) = client.shutdown().await {
                warn!(error = %e, "Error shutting down MCP client");
            }
        }
        return Ok(());
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
    yode_tui::app::run(
        provider,
        Arc::clone(&provider_registry),
        tool_registry,
        permissions,
        context,
        db,
        restored_messages,
        skill_cmds,
        all_provider_models,
    ).await?;

    // Shut down MCP clients
    for client in mcp_clients {
        if let Err(e) = client.shutdown().await {
            warn!(error = %e, "Error shutting down MCP client");
        }
    }

    info!("Yode exiting.");
    Ok(())
}

/// 截断字符串用于终端显示
fn truncate_str(s: &str, max: usize) -> String {
    let s = s.replace('\n', " ");
    if s.len() > max {
        format!("{}...", &s[..max])
    } else {
        s
    }
}
