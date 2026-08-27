use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Child;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use tokio::sync::mpsc::UnboundedSender;

use yode_core::config::Config;
use yode_core::db::Database;
use yode_core::engine::ConfirmResponse;
use yode_core::permission::PermissionRule;
use yode_llm::registry::ProviderRegistry;
use yode_tools::registry::ToolRegistry;
use yode_tools::tool::McpResourceProvider;

use crate::browser_settings::{apply_browser_settings_env, browser_settings_from_desktop_settings};
use crate::desktop_settings_store::read_desktop_settings_async;
use crate::git_settings::{apply_git_settings_env, git_settings_from_desktop_settings};
use crate::license_notices::read_license_notices;
use crate::protocol::{
    Bootstrap, GeneralSettings, LicenseNotice, PermissionModeState, RuntimeState, SessionRunState,
};

mod browser_settings_runtime;
mod computer_use_settings_runtime;
mod configuration_runtime;
mod edit_diff_runtime;
mod engine_setup;
mod git_settings_runtime;
mod hooks_settings_runtime;
mod import_runtime;
mod mcp_config;
mod mcp_runtime;
mod personalization_runtime;
mod provider_runtime;
mod session_runtime;
mod settings_runtime;
mod settings_system;
mod terminal_helpers;
mod terminal_runtime;
#[cfg(test)]
mod tests;
mod turn_events;
mod turn_loop;
mod turn_permissions;
mod turn_runtime;
mod worktree_runtime;

use self::configuration_runtime::load_desktop_config;
use self::mcp_runtime::setup_desktop_tooling;
use self::provider_runtime::bootstrap_providers;
use self::settings_runtime::default_general_settings;
use self::terminal_runtime::{PtySessionState, TerminalSessionState};
use self::turn_runtime::SessionOperationMap;

pub struct DesktopRuntime {
    config: Mutex<Config>,
    db: Arc<Database>,
    db_path: PathBuf,
    workspace_path: PathBuf,
    /// 用户级配置文件的唯一持久化目标。生产环境使用真实用户目录；测试可注入临时路径。
    user_config_path: PathBuf,
    /// 工作区是否处于可信状态（由仓库外 workspace-trust.toml 绑定
    /// canonical path + 配置哈希 + remote 决定）。未信任时不得加载
    /// 插件贡献（MCP/Hooks/Skills/Commands），项目配置也不得生效。
    workspace_trusted: std::sync::atomic::AtomicBool,
    provider_registry: Mutex<Arc<ProviderRegistry>>,
    tool_registry: Mutex<Arc<ToolRegistry>>,
    mcp_resource_provider: Mutex<Option<Arc<dyn McpResourceProvider>>>,
    active_session_id: Mutex<Option<String>>,
    permission_mode: Mutex<String>,
    confirm_txs: ConfirmSenderMap,
    ask_user_txs: AskUserSenderMap,
    cancel_tokens: CancelTokenMap,
    /// 每会话生命周期操作占位（原子检查+占用）。turn 取消后仍保持占用，
    /// 直到事件循环真正 quiesce 才释放；破坏性会话操作也共用此槽位。
    active_sessions: SessionOperationMap,
    run_registry: Arc<Mutex<HashMap<String, SessionRunState>>>,
    pending_confirmations: PendingConfirmationMap,
    session_permission_rules: Arc<Mutex<HashMap<String, Vec<PermissionRule>>>>,
    terminal_sessions: Mutex<HashMap<String, TerminalSessionState>>,
    pty_sessions: Arc<Mutex<HashMap<String, PtySessionState>>>,
    general_settings: Mutex<GeneralSettings>,
    sleep_guard: Arc<Mutex<Option<Child>>>,
}

type TurnKey = (String, String);
type ConfirmSenderMap = Arc<Mutex<HashMap<TurnKey, UnboundedSender<ConfirmResponse>>>>;
type AskUserSenderMap = Arc<Mutex<HashMap<TurnKey, UnboundedSender<String>>>>;
type CancelTokenMap = Arc<Mutex<HashMap<TurnKey, tokio_util::sync::CancellationToken>>>;
type PendingConfirmationMap = Arc<Mutex<HashMap<TurnKey, PendingConfirmation>>>;

#[derive(Debug, Clone)]
struct PendingConfirmation {
    tool_name: String,
    command: Option<String>,
}

impl DesktopRuntime {
    pub async fn new() -> Result<Self> {
        let workspace_path = resolve_desktop_workspace_path().await;
        let user_config_path = default_user_config_path(&workspace_path);
        let workspace_trusted =
            yode_core::workspace_trust::WorkspaceTrustStore::load().is_trusted(&workspace_path);

        let config = match load_desktop_config(&user_config_path).await {
            Ok(config) => config,
            Err(err) => Config::load_from_async(None).await.with_context(|| {
                format!(
                    "failed to load desktop config from {} and default config after: {err}",
                    workspace_path.display()
                )
            })?,
        };
        // 会话数据库路径统一由共享 Config 决定，Desktop 不维护第二套路径规则。
        let db_path = desktop_session_db_path(&config);

        let provider_registry = Mutex::new(bootstrap_providers(&config));
        let (tool_registry, mcp_resource_provider) =
            setup_desktop_tooling(&config, &workspace_path, workspace_trusted).await;
        if let Ok(settings) = read_desktop_settings_async().await {
            if let Ok(browser_settings) = browser_settings_from_desktop_settings(&settings) {
                apply_browser_settings_env(&browser_settings);
            }
            if let Ok(git_settings) = git_settings_from_desktop_settings(&settings) {
                apply_git_settings_env(&git_settings);
            }
        }

        let configured_mode = config
            .permissions
            .default_mode
            .clone()
            .unwrap_or_else(|| "Default".to_string());
        // Bypass 永不跨应用重启恢复。即使旧版本曾把它写进用户配置，
        // 新版本启动时也回到需要确认的 Default。
        let default_mode = configured_mode
            .parse::<yode_core::permission::PermissionMode>()
            .ok()
            .filter(|mode| *mode != yode_core::permission::PermissionMode::Bypass)
            .unwrap_or(yode_core::permission::PermissionMode::Default)
            .to_string();

        let db = Arc::new(Database::open(&db_path)?);
        // 进程启动：上次运行遗留的 running/starting/waiting_*/cancelling turn
        // 一律标记为 interrupted，绝不伪装成成功；随后限量清理已终态 journal。
        // 仅在此处执行一次（不在 Database::open 内），避免后台线程二次打开误伤
        // 正在运行的新 turn。
        match db.mark_interrupted_turns("检测到上次运行未正常结束，已标记为中断")
        {
            Ok(marked) => {
                if marked > 0 {
                    tracing::info!("标记 {} 个上次运行遗留的 turn 为 interrupted", marked);
                }
            }
            Err(err) => {
                tracing::error!("标记遗留 turn 为 interrupted 失败: {}", err);
            }
        }
        if let Err(err) = db.prune_turn_journals() {
            tracing::error!("启动时清理 turn journal 失败: {}", err);
        }

        Ok(Self {
            config: Mutex::new(config),
            db,
            db_path,
            workspace_path,
            user_config_path,
            workspace_trusted: std::sync::atomic::AtomicBool::new(workspace_trusted),
            provider_registry,
            tool_registry: Mutex::new(tool_registry),
            mcp_resource_provider: Mutex::new(mcp_resource_provider),
            active_session_id: Mutex::new(None),
            permission_mode: Mutex::new(default_mode),
            confirm_txs: Arc::new(Mutex::new(HashMap::new())),
            ask_user_txs: Arc::new(Mutex::new(HashMap::new())),
            cancel_tokens: Arc::new(Mutex::new(HashMap::new())),
            active_sessions: Arc::new(Mutex::new(HashMap::new())),
            run_registry: Arc::new(Mutex::new(HashMap::new())),
            pending_confirmations: Arc::new(Mutex::new(HashMap::new())),
            session_permission_rules: Arc::new(Mutex::new(HashMap::new())),
            terminal_sessions: Mutex::new(HashMap::new()),
            pty_sessions: Arc::new(Mutex::new(HashMap::new())),
            general_settings: Mutex::new(default_general_settings()),
            sleep_guard: Arc::new(Mutex::new(None)),
        })
    }

    pub fn bootstrap(&self) -> Result<Bootstrap> {
        let sessions = self.sessions_list()?;
        let permission_mode = self
            .permission_mode
            .lock()
            .map_err(|_| anyhow::anyhow!("permission mode lock poisoned"))?
            .clone();
        let config = self
            .config
            .lock()
            .map_err(|_| anyhow::anyhow!("config lock poisoned"))?;
        Ok(Bootstrap {
            app_version: env!("CARGO_PKG_VERSION"),
            workspace_path: self.workspace_path.display().to_string(),
            workspace_trusted: self.workspace_trusted(),
            provider: config.llm.default_provider.clone(),
            model: config.llm.default_model.clone(),
            permission_mode: permission_mode.clone(),
            effective_permission_mode: permission_mode,
            sessions,
            runs: self.runs_list()?,
        })
    }

    pub fn runtime_state(&self) -> Result<RuntimeState> {
        let permission_mode = self
            .permission_mode
            .lock()
            .map_err(|_| anyhow::anyhow!("permission mode lock poisoned"))?
            .clone();
        let active_turns = self
            .cancel_tokens
            .lock()
            .map_err(|_| anyhow::anyhow!("cancel token lock poisoned"))?
            .len();
        Ok(RuntimeState {
            active_session_id: self
                .active_session_id
                .lock()
                .map_err(|_| anyhow::anyhow!("active session lock poisoned"))?
                .clone(),
            status: if active_turns > 0 { "running" } else { "idle" }.to_string(),
            permission_mode: permission_mode.clone(),
            effective_permission_mode: permission_mode,
            context_percent: 0,
            tool_calls: format!("{} active", active_turns),
        })
    }

    pub async fn permission_mode_set(
        &self,
        mode: String,
        bypass_confirmed: bool,
        scope: Option<String>,
    ) -> Result<PermissionModeState> {
        let parsed = mode
            .parse::<yode_core::permission::PermissionMode>()
            .map_err(|err| anyhow::anyhow!(err))?;
        let is_bypass = parsed == yode_core::permission::PermissionMode::Bypass;
        if is_bypass {
            if !bypass_confirmed {
                anyhow::bail!("启用完全信任模式需要再次明确确认。");
            }
            if scope.as_deref() != Some("application-session") {
                anyhow::bail!("完全信任模式仅允许作用于当前应用会话。");
            }
        }

        // 非 Bypass 模式先持久化，再更新内存；写入失败时有效模式保持不变。
        if !is_bypass {
            // 在文件锁内以磁盘最新配置应用窄修改：并发修改其他配置域不会被覆盖。
            self.update_user_config(move |config| {
                config.permissions.default_mode = Some(parsed.to_string());
                Ok(())
            })?;
        }
        {
            let mut active_mode = self
                .permission_mode
                .lock()
                .map_err(|_| anyhow::anyhow!("permission mode lock poisoned"))?;
            *active_mode = parsed.to_string();
        }
        Ok(PermissionModeState {
            effective_permission_mode: parsed.to_string(),
            scope: if is_bypass {
                "application-session".to_string()
            } else {
                "user-default".to_string()
            },
            persisted: !is_bypass,
            bypass_active: is_bypass,
        })
    }

    /// 运行状态列表：数据库 turn journal 是事实来源；内存热缓存仅补充
    /// 极少数尚未落盘的状态（如正在写入的首个事件）。
    pub fn runs_list(&self) -> Result<Vec<SessionRunState>> {
        let mut runs = self
            .db
            .list_recent_turns(64)?
            .into_iter()
            .map(|turn| SessionRunState {
                session_id: turn.session_id,
                turn_id: turn.turn_id,
                status: turn.status.as_str().to_string(),
                updated_at: turn.updated_at.to_rfc3339(),
                detail: turn.detail,
                started_at: Some(turn.started_at.to_rfc3339()),
                ended_at: turn.ended_at.map(|value| value.to_rfc3339()),
                last_seq: turn.last_seq,
                error_code: turn.error_code,
                cancellation_requested: turn.cancellation_requested,
            })
            .collect::<Vec<_>>();
        let registry = self
            .run_registry
            .lock()
            .map_err(|_| anyhow::anyhow!("run registry lock poisoned"))?;
        for entry in registry.values() {
            let exists = runs
                .iter()
                .any(|run| run.session_id == entry.session_id && run.turn_id == entry.turn_id);
            if !exists {
                runs.push(entry.clone());
            }
        }
        runs.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
        Ok(runs)
    }

    pub fn workspace_trusted(&self) -> bool {
        self.workspace_trusted
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// 信任当前工作区（显式用户确认后调用），并返回新的信任状态。
    /// 信任绑定 canonical path + `.yode/config.toml` 哈希 + git remote。
    pub async fn trust_workspace(&self) -> Result<bool> {
        let mut store = yode_core::workspace_trust::WorkspaceTrustStore::load();
        store
            .set_trusted(&self.workspace_path, true)
            .map_err(|err| anyhow::anyhow!(err))?;
        let trusted = store.is_trusted(&self.workspace_path);
        let previously_trusted = self.workspace_trusted();
        if trusted != previously_trusted {
            // 信任状态变化后重新装配工具（插件 Skills 等贡献此时才允许加载）
            let config = self
                .config
                .lock()
                .map_err(|_| anyhow::anyhow!("config lock poisoned"))?
                .clone();
            let (tool_registry, mcp_resource_provider) =
                setup_desktop_tooling(&config, &self.workspace_path, trusted).await;
            *self
                .tool_registry
                .lock()
                .map_err(|_| anyhow::anyhow!("tool registry lock poisoned"))? = tool_registry;
            *self
                .mcp_resource_provider
                .lock()
                .map_err(|_| anyhow::anyhow!("mcp resource provider lock poisoned"))? =
                mcp_resource_provider;
            self.workspace_trusted
                .store(trusted, std::sync::atomic::Ordering::Relaxed);
        }
        Ok(self.workspace_trusted())
    }

    /// 撤销当前工作区信任并返回新的信任状态。
    pub async fn revoke_workspace_trust(&self) -> Result<bool> {
        let mut store = yode_core::workspace_trust::WorkspaceTrustStore::load();
        store
            .revoke(&self.workspace_path)
            .map_err(|err| anyhow::anyhow!(err))?;
        if self.workspace_trusted() {
            let config = self
                .config
                .lock()
                .map_err(|_| anyhow::anyhow!("config lock poisoned"))?
                .clone();
            let (tool_registry, mcp_resource_provider) =
                setup_desktop_tooling(&config, &self.workspace_path, false).await;
            *self
                .tool_registry
                .lock()
                .map_err(|_| anyhow::anyhow!("tool registry lock poisoned"))? = tool_registry;
            *self
                .mcp_resource_provider
                .lock()
                .map_err(|_| anyhow::anyhow!("mcp resource provider lock poisoned"))? =
                mcp_resource_provider;
            self.workspace_trusted
                .store(false, std::sync::atomic::Ordering::Relaxed);
        }
        Ok(self.workspace_trusted())
    }

    pub async fn license_notices(&self) -> Result<Vec<LicenseNotice>> {
        Ok(read_license_notices(&self.workspace_path).await)
    }

    fn user_config_path(&self) -> PathBuf {
        self.user_config_path.clone()
    }

    fn project_config_path(&self) -> PathBuf {
        self.workspace_path.join(".yode").join("config.toml")
    }
}

fn default_user_config_path(workspace_path: &std::path::Path) -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| workspace_path.to_path_buf())
        .join(".yode")
        .join("config.toml")
}

/// Desktop 会话数据库路径统一遵守共享配置中的 `[session].db_path`。
fn desktop_session_db_path(config: &Config) -> PathBuf {
    config.session_db_path()
}

async fn resolve_desktop_workspace_path() -> PathBuf {
    let current_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    find_workspace_root(&current_dir)
        .await
        .unwrap_or(current_dir)
}

async fn find_workspace_root(start: &std::path::Path) -> Option<PathBuf> {
    for ancestor in start.ancestors() {
        let git_dir_exists = tokio::fs::metadata(ancestor.join(".git"))
            .await
            .map(|metadata| metadata.is_dir())
            .unwrap_or(false);
        if git_dir_exists || is_cargo_workspace_root(ancestor).await {
            return Some(ancestor.to_path_buf());
        }
    }
    None
}

async fn is_cargo_workspace_root(path: &std::path::Path) -> bool {
    tokio::fs::read_to_string(path.join("Cargo.toml"))
        .await
        .map(|content| content.contains("[workspace]"))
        .unwrap_or(false)
}
