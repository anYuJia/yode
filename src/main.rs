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
use yode_llm::providers::gemini::GeminiProvider;
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
        match command {
            Commands::Provider { action } => {
                match action {
                    ProviderAction::Add => {
                        run_setup_interactive()?;
                    }
                    ProviderAction::List => {
                        println!("已配置的提供商列表:");
                        for (name, p) in &config.llm.providers {
                            let is_default = if name == &config.llm.default_provider {
                                " (当前默认)"
                            } else {
                                ""
                            };
                            println!(
                                "- {}{} [格式: {}, Base URL: {}]",
                                name,
                                is_default,
                                p.format,
                                p.base_url.as_deref().unwrap_or("")
                            );
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
            Commands::Update { action } => {
                let action = action.unwrap_or(UpdateAction::Check);
                match action {
                    UpdateAction::Check => {
                        println!("正在检查更新...");
                        let config_dir = dirs::home_dir()
                            .unwrap_or_else(|| PathBuf::from("."))
                            .join(".yode");
                        let updater = yode_core::updater::Updater::new(
                            config_dir,
                            true,
                            config.update.auto_download,
                        );
                        match updater.check_for_updates().await {
                            Ok(Some(result)) => {
                                println!("✨ 发现新版本: {}", result.latest_version);
                                println!("   当前版本: {}", yode_core::updater::CURRENT_VERSION);
                                println!("\n发布日志:\n{}", result.release_notes);

                                if config.update.auto_download {
                                    println!("\n正在下载更新...");
                                    match updater.download_update(&result).await {
                                        Ok(_) => {
                                            println!("✓ 更新已下载。请重启 yode 以完成安装。");
                                        }
                                        Err(e) => {
                                            println!("✗ 下载失败: {}", e);
                                        }
                                    }
                                } else {
                                    println!("\n你可以运行以下命令更新:");
                                    println!("  curl -fsSL https://raw.githubusercontent.com/anYuJia/yode/main/install.sh | bash");
                                }
                            }
                            Ok(None) => {
                                println!(
                                    "✓ 当前已是最新版本 ({})",
                                    yode_core::updater::CURRENT_VERSION
                                );
                            }
                            Err(e) => {
                                println!("✗ 检查更新失败: {}", e);
                            }
                        }
                    }
                    UpdateAction::Status => {
                        println!("更新配置状态:");
                        println!("  自动检查: {}", config.update.auto_check);
                        println!("  自动下载: {}", config.update.auto_download);
                        println!("  当前版本: {}", yode_core::updater::CURRENT_VERSION);
                        match yode_core::updater::latest_local_release_tag() {
                            Some(tag) => {
                                let status = if yode_core::updater::release_version_matches_tag(
                                    &tag,
                                    yode_core::updater::CURRENT_VERSION,
                                ) {
                                    "匹配"
                                } else {
                                    "不匹配"
                                };
                                println!("  最新本地 tag: {} ({})", tag, status);
                            }
                            None => {
                                println!("  最新本地 tag: 未找到");
                            }
                        }
                    }
                    UpdateAction::Preflight => {
                        println!("正在运行发布前检查...");
                        let mut has_failure = false;

                        let git_status = std::process::Command::new("git")
                            .args(["status", "--porcelain"])
                            .output();
                        match git_status {
                            Ok(output) if output.status.success() => {
                                let dirty = !String::from_utf8_lossy(&output.stdout).trim().is_empty();
                                if dirty {
                                    has_failure = true;
                                    println!("  [!!] 工作树不干净，请先提交或清理改动");
                                } else {
                                    println!("  [ok] 工作树干净");
                                }
                            }
                            _ => {
                                has_failure = true;
                                println!("  [!!] 无法检查 git 工作树状态");
                            }
                        }

                        match yode_core::updater::latest_local_release_tag() {
                            Some(tag)
                                if yode_core::updater::release_version_matches_tag(
                                    &tag,
                                    yode_core::updater::CURRENT_VERSION,
                                ) =>
                            {
                                println!(
                                    "  [ok] 版本与最新 tag 一致: {} == {}",
                                    yode_core::updater::CURRENT_VERSION,
                                    tag
                                );
                            }
                            Some(tag) => {
                                has_failure = true;
                                println!(
                                    "  [!!] 版本与最新 tag 不一致: Cargo={} latest-tag={}",
                                    yode_core::updater::CURRENT_VERSION,
                                    tag
                                );
                            }
                            None => {
                                println!("  [--] 未找到本地 release tag，跳过版本对比");
                            }
                        }

                        match check_workspace_package_versions() {
                            Ok(()) => println!("  [ok] workspace package versions consistent"),
                            Err(err) => {
                                has_failure = true;
                                println!("  [!!] workspace package version check failed: {}", err);
                            }
                        }

                        for (label, mut command) in [
                            (
                                "cargo check",
                                {
                                    let mut cmd = std::process::Command::new("cargo");
                                    cmd.arg("check");
                                    cmd
                                },
                            ),
                            (
                                "cargo test -p yode-tools",
                                {
                                    let mut cmd = std::process::Command::new("cargo");
                                    cmd.args(["test", "-p", "yode-tools"]);
                                    cmd
                                },
                            ),
                        ] {
                            match command.status() {
                                Ok(status) if status.success() => {
                                    println!("  [ok] {}", label);
                                }
                                Ok(_) => {
                                    has_failure = true;
                                    println!("  [!!] {} 失败", label);
                                }
                                Err(err) => {
                                    has_failure = true;
                                    println!("  [!!] 无法运行 {}: {}", label, err);
                                }
                            }
                        }

                        if has_failure {
                            anyhow::bail!("发布前检查失败");
                        }

                        println!("  [ok] 发布前检查通过");
                    }
                    UpdateAction::Notes { from, limit } => {
                        let base = from
                            .or_else(yode_core::updater::latest_local_release_tag)
                            .unwrap_or_else(|| "HEAD~20".to_string());
                        let range = format!("{}..HEAD", base);
                        let output = std::process::Command::new("git")
                            .args([
                                "log",
                                "--pretty=format:- %s",
                                "--no-merges",
                                &format!("--max-count={}", limit),
                                &range,
                            ])
                            .output();
                        match output {
                            Ok(output) if output.status.success() => {
                                let notes = String::from_utf8_lossy(&output.stdout);
                                println!("# Release notes draft\n");
                                println!("Range: {}\n", range);
                                if notes.trim().is_empty() {
                                    println!("No commits found.");
                                } else {
                                    println!("{}", notes);
                                }
                            }
                            Ok(output) => {
                                anyhow::bail!(
                                    "failed to generate release notes: {}",
                                    String::from_utf8_lossy(&output.stderr)
                                );
                            }
                            Err(err) => {
                                anyhow::bail!("failed to run git log: {}", err);
                            }
                        }
                    }
                }
                return Ok(());
            }
            Commands::Completions { shell } => {
                use clap::CommandFactory;
                let mut cmd = Cli::command();
                let bin_name = cmd.get_name().to_string();
                clap_complete::generate(shell, &mut cmd, bin_name, &mut std::io::stdout());
                return Ok(());
            }
            Commands::Doctor => {
                println!("正在进行环境健康检查...");
                // Note: CLI doctor uses simplified logic or we can bridge to command
                let git_v = std::process::Command::new("git").arg("--version").output();
                match git_v {
                    Ok(o) if o.status.success() => {
                        println!(
                            "  [ok] git available: {}",
                            String::from_utf8_lossy(&o.stdout).trim()
                        );
                    }
                    _ => println!("  [!!] git not found"),
                }

                for runtime in ["node", "python3", "go", "cargo"] {
                    let out = std::process::Command::new(runtime)
                        .arg("--version")
                        .output();
                    if let Ok(o) = out {
                        println!(
                            "  [ok] {} available: {}",
                            runtime,
                            String::from_utf8_lossy(&o.stdout).trim()
                        );
                    } else {
                        println!("  [--] {} not found", runtime);
                    }
                }

                if config.llm.providers.is_empty() {
                    println!("  [!!] No LLM providers configured.");
                } else {
                    println!("  [ok] {} providers configured", config.llm.providers.len());
                }

                match yode_core::updater::latest_local_release_tag() {
                    Some(tag)
                        if yode_core::updater::release_version_matches_tag(
                            &tag,
                            yode_core::updater::CURRENT_VERSION,
                        ) =>
                    {
                        println!(
                            "  [ok] Version matches latest local tag: {} == {}",
                            yode_core::updater::CURRENT_VERSION,
                            tag
                        );
                    }
                    Some(tag) => {
                        println!(
                            "  [!!] Version/tag mismatch: Cargo={} latest-tag={}",
                            yode_core::updater::CURRENT_VERSION,
                            tag
                        );
                    }
                    None => {
                        println!("  [--] Could not determine latest local release tag");
                    }
                }

                println!(
                    "\nPlatform: {} {}",
                    std::env::consts::OS,
                    std::env::consts::ARCH
                );
                println!("Version:  v{}", env!("CARGO_PKG_VERSION"));
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

    // Setup LLM provider registry
    let provider_registry = ProviderRegistry::new();

    // Register all configured providers
    for (name, p_config) in &config.llm.providers {
        let env_prefix = name.to_uppercase().replace("-", "_");
        let api_key = match env::var(format!("{}_API_KEY", env_prefix))
            .ok()
            .or_else(|| p_config.api_key.clone())
            .or_else(|| {
                // Fallback: check known provider env keys
                if let Some(info) = yode_llm::find_provider_info(name) {
                    info.env_keys.iter().find_map(|k| env::var(k).ok())
                } else if p_config.format == "openai" {
                    env::var("OPENAI_API_KEY").ok()
                } else if p_config.format == "anthropic" {
                    env::var("ANTHROPIC_API_KEY")
                        .or_else(|_| env::var("ANTHROPIC_AUTH_TOKEN"))
                        .ok()
                } else if p_config.format == "gemini" {
                    env::var("GOOGLE_API_KEY")
                        .or_else(|_| env::var("GEMINI_API_KEY"))
                        .ok()
                } else {
                    None
                }
            }) {
            Some(k) => k,
            None => {
                // Ollama doesn't need an API key
                if name == "ollama" || p_config.format == "ollama" {
                    String::new()
                } else {
                    warn!(
                        "Provider '{}' is configured but missing an API key, skipping.",
                        name
                    );
                    continue;
                }
            }
        };

        let known = yode_llm::find_provider_info(name);
        let default_base =
            known
                .map(|k| k.default_base_url)
                .unwrap_or(match p_config.format.as_str() {
                    "openai" => "https://api.openai.com/v1",
                    "anthropic" => "https://api.anthropic.com",
                    "gemini" => "https://generativelanguage.googleapis.com/v1beta",
                    _ => "https://api.openai.com/v1",
                });

        let base_url = match env::var(format!("{}_BASE_URL", env_prefix))
            .ok()
            .or_else(|| p_config.base_url.clone())
        {
            Some(u) => {
                if u.is_empty() {
                    default_base.to_string()
                } else {
                    u
                }
            }
            None => default_base.to_string(),
        };

        match p_config.format.as_str() {
            "anthropic" => {
                provider_registry
                    .register(Arc::new(AnthropicProvider::new(name, &api_key, &base_url)));
            }
            "gemini" => {
                let mut p = GeminiProvider::new(&api_key);
                if base_url != "https://generativelanguage.googleapis.com/v1beta" {
                    p = p.with_base_url(&base_url);
                }
                provider_registry.register(Arc::new(p));
            }
            _ => {
                // OpenAI-compatible (openai, ollama, groq, mistral, deepseek, xai, etc.)
                provider_registry
                    .register(Arc::new(OpenAiProvider::new(name, &api_key, &base_url)));
            }
        }
    }

    // Auto-detect providers from environment if not explicitly configured
    for info in yode_llm::detect_available_providers() {
        if provider_registry.contains(info.name) {
            continue;
        }
        let api_key = info
            .env_keys
            .iter()
            .find_map(|k| env::var(k).ok())
            .unwrap_or_default();
        match info.format {
            "anthropic" => {
                provider_registry.register(Arc::new(AnthropicProvider::new(
                    info.name,
                    &api_key,
                    info.default_base_url,
                )));
            }
            "gemini" => {
                provider_registry.register(Arc::new(GeminiProvider::new(&api_key)));
            }
            _ => {
                provider_registry.register(Arc::new(OpenAiProvider::new(
                    info.name,
                    &api_key,
                    info.default_base_url,
                )));
            }
        }
    }

    // Auto-detect provider and model from env vars or CLI args
    let provider_name = cli
        .provider
        .unwrap_or_else(|| config.llm.default_provider.clone());

    // Build provider → models map from config
    let all_provider_models: std::collections::HashMap<String, Vec<String>> = config
        .llm
        .providers
        .iter()
        .filter(|(name, _)| provider_registry.get(name).is_some())
        .map(|(name, p_config)| (name.clone(), p_config.models.clone()))
        .collect();

    // Resolve model: CLI arg > config default > first model in provider's list
    let provider_models = all_provider_models
        .get(&provider_name)
        .cloned()
        .unwrap_or_default();
    let model = {
        let requested = cli
            .model
            .unwrap_or_else(|| config.llm.default_model.clone());
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
