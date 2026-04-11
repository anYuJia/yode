mod app_bootstrap;
mod chat_mode;
mod cli_commands;
mod provider_bootstrap;

use std::env;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::Parser;
use tracing::{info, warn};

use yode_core::config::Config;
use yode_core::db::Database;
use yode_core::setup::{has_api_keys_configured, run_setup_interactive};

use crate::app_bootstrap::{
    configure_permissions, ensure_session_exists, init_logging, restore_or_create_context,
    setup_tooling, shutdown_mcp_clients, StartupProfiler,
};

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
    init_logging()?;
    info!("Yode starting...");
    let mut startup_profiler = StartupProfiler::new();

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
    startup_profiler.checkpoint("cli_parse");

    // Check if API keys are configured, if not run setup
    if !has_api_keys_configured() {
        run_setup_interactive()?;
    }

    // Load config
    let mut config =
        Config::load_from(cli.config.as_deref()).context("Failed to load configuration")?;
    startup_profiler.checkpoint("config_load");

    if let Some(command) = cli.command {
        cli_commands::handle_cli_command(command, &mut config).await?;
        return Ok(());
    }

    let workdir = cli
        .workdir
        .clone()
        .unwrap_or_else(|| env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let tooling = setup_tooling(&config, &workdir).await?;
    startup_profiler.checkpoint("tooling_setup");

    // If --serve-mcp, run as MCP server and exit
    if cli.serve_mcp {
        startup_profiler.checkpoint("ready_serve_mcp");
        startup_profiler.log_summary("serve_mcp", &tooling.metrics);
        info!("Running in MCP server mode");
        yode_mcp::run_mcp_server(Arc::clone(&tooling.tool_registry)).await?;
        return Ok(());
    }

    let db_path = config.session_db_path();
    let db_open_task = tokio::task::spawn_blocking(move || Database::open(&db_path));

    let provider_bootstrap = provider_bootstrap::bootstrap_provider_registry(
        cli.provider.clone(),
        cli.model.clone(),
        &config,
    )?;
    startup_profiler.checkpoint("provider_bootstrap");
    let provider_registry = provider_bootstrap.provider_registry;
    let provider_name = provider_bootstrap.provider_name;
    let _provider_models = provider_bootstrap.provider_models;
    let all_provider_models = provider_bootstrap.all_provider_models;
    let provider = provider_bootstrap.provider;
    let model = provider_bootstrap.model;
    let provider_metrics = provider_bootstrap.metrics;

    let db = db_open_task
        .await
        .context("Database open task failed")?
        .context("Failed to open session database")?;
    startup_profiler.checkpoint("db_ready");

    let permissions = configure_permissions(&config);
    startup_profiler.checkpoint("permission_setup");
    let (context, restored_messages) =
        restore_or_create_context(&cli, &db, workdir, provider_name.clone(), model.clone())?;
    ensure_session_exists(&db, &context)?;
    startup_profiler.checkpoint("session_bootstrap");

    // If --chat, run a single non-interactive turn and exit
    if let Some(chat_message) = cli.chat_message.as_deref() {
        startup_profiler.checkpoint("ready_chat");
        startup_profiler.log_summary("chat", &tooling.metrics);
        return chat_mode::run_noninteractive_chat(
            chat_message,
            provider,
            Arc::clone(&tooling.tool_registry),
            permissions,
            context,
            db,
            restored_messages,
            &config,
            tooling.mcp_clients,
        )
        .await;
    }

    info!(
        "Starting TUI with provider={}, model={}, session={}",
        context.provider, context.model, context.session_id
    );
    startup_profiler.checkpoint("ready_tui");
    let startup_summary = format!(
        "{} {}",
        startup_profiler.summary("tui", &tooling.metrics),
        provider_metrics.summary()
    );
    startup_profiler.log_summary("tui", &tooling.metrics);

    let skill_cmds: Vec<(String, String)> = tooling
        .skill_registry
        .list()
        .iter()
        .map(|s| (s.name.clone(), s.description.clone()))
        .collect();
    yode_tui::app::run(
        provider,
        Arc::clone(&provider_registry),
        tooling.tool_registry,
        permissions,
        context,
        db,
        restored_messages,
        skill_cmds,
        all_provider_models,
        Some(startup_summary),
    )
    .await?;

    shutdown_mcp_clients(tooling.mcp_clients).await;

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
    let metadata: serde_json::Value =
        serde_json::from_slice(&output.stdout).context("failed to parse cargo metadata output")?;
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
