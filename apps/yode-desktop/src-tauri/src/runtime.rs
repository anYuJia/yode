use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Child;
use std::sync::{atomic::AtomicU64, Arc, Mutex};

use anyhow::{Context, Result};
use tokio::sync::mpsc::UnboundedSender;

use yode_core::config::Config;
use yode_core::db::Database;
use yode_core::engine::ConfirmResponse;
use yode_core::permission::PermissionRule;
use yode_core::updater::Updater;
use yode_llm::registry::ProviderRegistry;
use yode_tools::registry::ToolRegistry;
use yode_tools::tool::McpResourceProvider;

use crate::browser_settings::{apply_browser_settings_env, browser_settings_from_desktop_settings};
use crate::desktop_settings_store::read_desktop_settings_async;
use crate::git_settings::{apply_git_settings_env, git_settings_from_desktop_settings};
use crate::license_notices::read_license_notices;
use crate::protocol::{Bootstrap, GeneralSettings, LicenseNotice, RuntimeState};

mod configuration_runtime;
mod edit_diff_runtime;
mod engine_setup;
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

pub struct DesktopRuntime {
    config: Mutex<Config>,
    db: Database,
    db_path: PathBuf,
    workspace_path: PathBuf,
    provider_registry: Mutex<Arc<ProviderRegistry>>,
    tool_registry: Mutex<Arc<ToolRegistry>>,
    mcp_resource_provider: Mutex<Option<Arc<dyn McpResourceProvider>>>,
    active_session_id: Mutex<Option<String>>,
    permission_mode: Mutex<String>,
    seq: AtomicU64,
    confirm_txs: ConfirmSenderMap,
    ask_user_txs: AskUserSenderMap,
    cancel_tokens: CancelTokenMap,
    pending_confirmations: PendingConfirmationMap,
    session_permission_rules: Arc<Mutex<HashMap<String, Vec<PermissionRule>>>>,
    terminal_sessions: Mutex<HashMap<String, TerminalSessionState>>,
    pty_sessions: Arc<Mutex<HashMap<String, PtySessionState>>>,
    general_settings: Mutex<GeneralSettings>,
    sleep_guard: Arc<Mutex<Option<Child>>>,
    updater: Updater,
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
        let db_path = dirs::home_dir()
            .unwrap_or_else(|| workspace_path.clone())
            .join(".yode")
            .join("sessions.db");

        let config = match load_desktop_config(&workspace_path).await {
            Ok(config) => config,
            Err(err) => Config::load_from_async(None).await.with_context(|| {
                format!(
                    "failed to load desktop config from {} and default config after: {err}",
                    workspace_path.display()
                )
            })?,
        };

        let provider_registry = Mutex::new(bootstrap_providers(&config));
        let (tool_registry, mcp_resource_provider) =
            setup_desktop_tooling(&config, &workspace_path).await;
        if let Ok(settings) = read_desktop_settings_async().await {
            if let Ok(browser_settings) = browser_settings_from_desktop_settings(&settings) {
                apply_browser_settings_env(&browser_settings);
            }
            if let Ok(git_settings) = git_settings_from_desktop_settings(&settings) {
                apply_git_settings_env(&git_settings);
            }
        }

        let default_mode = config
            .permissions
            .default_mode
            .clone()
            .unwrap_or_else(|| "Default".to_string());

        Ok(Self {
            config: Mutex::new(config),
            db: Database::open(&db_path)?,
            db_path,
            workspace_path,
            provider_registry,
            tool_registry: Mutex::new(tool_registry),
            mcp_resource_provider: Mutex::new(mcp_resource_provider),
            active_session_id: Mutex::new(None),
            permission_mode: Mutex::new(default_mode),
            seq: AtomicU64::new(1),
            confirm_txs: Arc::new(Mutex::new(HashMap::new())),
            ask_user_txs: Arc::new(Mutex::new(HashMap::new())),
            cancel_tokens: Arc::new(Mutex::new(HashMap::new())),
            pending_confirmations: Arc::new(Mutex::new(HashMap::new())),
            session_permission_rules: Arc::new(Mutex::new(HashMap::new())),
            terminal_sessions: Mutex::new(HashMap::new()),
            pty_sessions: Arc::new(Mutex::new(HashMap::new())),
            general_settings: Mutex::new(default_general_settings()),
            sleep_guard: Arc::new(Mutex::new(None)),
            updater: Updater::new(
                dirs::home_dir()
                    .unwrap_or_else(|| PathBuf::from("."))
                    .join(".yode"),
                true,
                true,
            ),
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
            provider: config.llm.default_provider.clone(),
            model: config.llm.default_model.clone(),
            permission_mode,
            sessions,
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
            permission_mode,
            context_percent: 0,
            tool_calls: format!("{} active", active_turns),
        })
    }

    pub fn permission_mode_set(&self, mode: String) -> Result<()> {
        let parsed = mode
            .parse::<yode_core::permission::PermissionMode>()
            .map_err(|err| anyhow::anyhow!(err))?;
        let mut active_mode = self
            .permission_mode
            .lock()
            .map_err(|_| anyhow::anyhow!("permission mode lock poisoned"))?;
        *active_mode = parsed.to_string();
        Ok(())
    }

    pub async fn license_notices(&self) -> Result<Vec<LicenseNotice>> {
        Ok(read_license_notices(&self.workspace_path).await)
    }

    fn user_config_path(&self) -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| self.workspace_path.clone())
            .join(".yode")
            .join("config.toml")
    }

    fn project_config_path(&self) -> PathBuf {
        self.workspace_path.join(".yode").join("config.toml")
    }

    pub async fn check_for_updates(&self) -> Result<Option<yode_core::updater::UpdateCheckResult>> {
        self.updater.check_for_updates().await
    }

    pub async fn download_update(&self) -> Result<String> {
        let update = self
            .updater
            .check_for_updates()
            .await?
            .ok_or_else(|| anyhow::anyhow!("no update available"))?;
        let result = self.updater.download_update(&update).await?;
        Ok(result.display().to_string())
    }

    pub async fn has_pending_update(&self) -> bool {
        self.updater.has_pending_update().await
    }

    pub async fn apply_downloaded_update(&self) -> Result<bool> {
        self.updater.apply_downloaded_update().await
    }
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
