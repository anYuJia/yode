mod cli_commands;
mod provider_bootstrap;

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
    /// 检查并安装更新
    Update {
        #[command(subcommand)]
        action: Option<UpdateAction>,
    },
    /// 生成 Shell 补全脚本
    Completions {
        /// Shell 类型 (bash, zsh, fish, powershell)
        shell: clap_complete::Shell,
    },
    /// 运行环境健康检查
    Doctor,
}

#[derive(clap::Subcommand)]
enum UpdateAction {
    /// 检查并下载更新
    Check,
    /// 查看更新配置状态
    Status,
    /// 发布前检查（工作树、版本、编译、核心测试）
    Preflight,
    /// 生成从某个 tag 到 HEAD 的发布说明草稿
    Notes {
        /// 起始 tag，默认使用最新本地 release tag
        #[arg(long)]
        from: Option<String>,
        /// 最多显示多少条提交
        #[arg(long, default_value_t = 50)]
        limit: usize,
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

    // Check for pending updates and apply them before doing anything else
    let config_dir = dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".yode");
    let updater = yode_core::updater::Updater::new(
        config_dir.clone(),
        true,  // auto_check doesn't matter for apply
        false, // auto_download doesn't matter for apply
    );

    if updater.has_pending_update() {
        match updater.apply_downloaded_update() {
            Ok(true) => {
                info!("Update applied, restarting...");
                let args: Vec<String> = std::env::args().collect();
                let exe = std::env::current_exe()?;

                #[cfg(unix)]
                {
                    use std::os::unix::process::CommandExt;
                    let mut cmd = std::process::Command::new(exe);
                    cmd.args(&args[1..]);
                    let err = cmd.exec();
                    return Err(anyhow::anyhow!("Failed to restart after update: {}", err));
                }

                #[cfg(not(unix))]
                {
                    std::process::Command::new(exe).args(&args[1..]).spawn()?;
                    std::process::exit(0);
                }
            }
            Ok(false) => {}
            Err(e) => {
                warn!("Failed to apply update: {}", e);
                eprintln!("⚠ Failed to apply update: {}", e);
            }
        }
    }

    let cli = Cli::parse();

    // Check if API keys are configured, if not run setup
    if !has_api_keys_configured() {
        run_setup_interactive()?;
    }

    // Load config
    let mut config =
        Config::load_from(cli.config.as_deref()).context("Failed to load configuration")?;

    if let Some(command) = cli.command {
        cli_commands::handle_cli_command(command, &mut config).await?;
        return Ok(());
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
            store.add(
                skill.name.clone(),
                skill.description.clone(),
                skill.content.clone(),
            );
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

    let provider_bootstrap = provider_bootstrap::bootstrap_provider_registry(
        cli.provider.clone(),
        cli.model.clone(),
        &config,
    )?;
    let provider_registry = provider_bootstrap.provider_registry;
    let provider_name = provider_bootstrap.provider_name;
    let _provider_models = provider_bootstrap.provider_models;
    let all_provider_models = provider_bootstrap.all_provider_models;
    let provider = provider_bootstrap.provider;
    let model = provider_bootstrap.model;

    // Setup permissions from config
    let mut permissions =
        PermissionManager::from_confirmation_list(config.tools.require_confirmation.clone());

    // Apply permission mode from config
    if let Some(ref mode_str) = config.permissions.default_mode {
        if let Ok(mode) = mode_str.parse::<yode_core::PermissionMode>() {
            permissions.set_mode(mode);
        }
    }

    // Load permission rules from config
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
                    let mut blocks = Vec::new();
                    if let Some(ref r) = m.reasoning {
                        blocks.push(yode_llm::types::ContentBlock::Thinking {
                            thinking: r.clone(),
                            signature: None,
                        });
                    }
                    if let Some(ref t) = m.content {
                        blocks.push(yode_llm::types::ContentBlock::Text { text: t.clone() });
                    }

                    Some(
                        Message {
                            role,
                            content: m.content,
                            content_blocks: blocks,
                            reasoning: m.reasoning,
                            tool_calls,
                            tool_call_id: m.tool_call_id,
                            images: Vec::new(),
                        }
                        .normalized(),
                    )
                })
                .collect();
            (ctx, Some(messages))
        } else {
            eprintln!("会话 '{}' 未找到，创建新会话。", resume_id);
            (
                AgentContext::new(workdir, provider_name.clone(), model.clone()),
                None,
            )
        }
    } else {
        (
            AgentContext::new(workdir, provider_name.clone(), model.clone()),
            None,
        )
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

        // Apply cost budget from config
        if let Some(budget) = config.cost.max_budget_usd {
            if budget > 0.0 {
                engine.cost_tracker_mut().set_budget_limit(budget);
            }
        }

        // Setup hook manager from config
        if !config.hooks.hooks.is_empty() {
            use yode_core::hooks::{HookDefinition, HookManager};
            let mut hook_mgr = HookManager::new(std::env::current_dir().unwrap_or_default());
            for h in &config.hooks.hooks {
                hook_mgr.register(HookDefinition {
                    command: h.command.clone(),
                    events: h.events.clone(),
                    tool_filter: h.tool_filter.clone(),
                    timeout_secs: h.timeout_secs,
                    can_block: h.can_block,
                });
            }
            engine.set_hook_manager(hook_mgr);
        }

        if let Some(msgs) = restored_messages {
            engine.restore_messages(msgs);
        }
        engine
            .initialize_session_hooks(if engine.context().is_resumed {
                "resume"
            } else {
                "startup"
            })
            .await;

        let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel();
        let (confirm_tx, confirm_rx) = tokio::sync::mpsc::unbounded_channel();

        // 用流式模式执行（兼容只支持流式的第三方 API）
        let chat_msg_owned = chat_msg.clone();
        let engine_handle = tokio::spawn(async move {
            let result = engine
                .run_turn_streaming(
                    &chat_msg_owned,
                    yode_core::context::QuerySource::User,
                    event_tx,
                    confirm_rx,
                    None,
                )
                .await;
            engine.finalize_session_hooks("chat_exit").await;
            result
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
                EngineEvent::ToolCallStart {
                    name, arguments, ..
                } => {
                    eprintln!(
                        "\x1b[90m⚡ {}({})\x1b[0m",
                        name,
                        truncate_str(&arguments, 80)
                    );
                }
                EngineEvent::ToolConfirmRequired { id, name, .. } => {
                    // 非交互模式自动允许工具执行
                    eprintln!("\x1b[33m🔑 自动确认工具: {}\x1b[0m", name);
                    let _ = confirm_tx.send(ConfirmResponse::Allow);
                    let _ = id;
                }
                EngineEvent::ToolResult { name, result, .. } => {
                    if result.is_error {
                        eprintln!(
                            "\x1b[31m✗ {} 失败: {}\x1b[0m",
                            name,
                            truncate_str(&result.content, 200)
                        );
                    } else {
                        eprintln!(
                            "\x1b[90m✓ {} 完成 ({} 字节)\x1b[0m",
                            name,
                            result.content.len()
                        );
                    }
                }
                EngineEvent::Error(e) => {
                    eprintln!("\x1b[31m错误: {}\x1b[0m", e);
                }
                EngineEvent::Retrying {
                    error_message,
                    attempt,
                    max_attempts,
                    delay_secs,
                } => {
                    eprintln!("\x1b[31m⎿  {}\x1b[0m", error_message);
                    eprintln!(
                        "\x1b[33m   Retrying in {} seconds… (attempt {}/{})\x1b[0m",
                        delay_secs, attempt, max_attempts
                    );
                }
                EngineEvent::SessionMemoryUpdated {
                    path,
                    generated_summary,
                } => {
                    eprintln!(
                        "\x1b[90m🧠 Session memory updated ({}) -> {}\x1b[0m",
                        if generated_summary {
                            "summary"
                        } else {
                            "snapshot"
                        },
                        path
                    );
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
    )
    .await?;

    // Shut down MCP clients
    for client in mcp_clients {
        if let Err(e) = client.shutdown().await {
            warn!(error = %e, "Error shutting down MCP client");
        }
    }

    info!("Yode exiting.");
    Ok(())
}

fn check_workspace_package_versions() -> Result<()> {
    let output = std::process::Command::new("cargo")
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .output()
        .context("failed to run cargo metadata")?;
    if !output.status.success() {
        anyhow::bail!(
            "cargo metadata failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let metadata: serde_json::Value = serde_json::from_slice(&output.stdout)
        .context("failed to parse cargo metadata output")?;
    let packages = metadata
        .get("packages")
        .and_then(|value| value.as_array())
        .ok_or_else(|| anyhow::anyhow!("cargo metadata missing packages array"))?;
    let mismatches = packages
        .iter()
        .filter_map(|package| {
            let name = package.get("name").and_then(|value| value.as_str())?;
            if name != "yode" && !name.starts_with("yode-") {
                return None;
            }
            let version = package.get("version").and_then(|value| value.as_str())?;
            (version != yode_core::updater::CURRENT_VERSION)
                .then(|| format!("{}={}", name, version))
        })
        .collect::<Vec<_>>();
    if !mismatches.is_empty() {
        anyhow::bail!(
            "expected {}, mismatches: {}",
            yode_core::updater::CURRENT_VERSION,
            mismatches.join(", ")
        );
    }
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
