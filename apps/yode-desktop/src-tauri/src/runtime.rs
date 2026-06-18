use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};
use std::process::{Child, Command};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, Mutex,
};

use anyhow::{Context, Result};
use chrono::Utc;
use serde_json::json;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};
use uuid::Uuid;

use yode_core::config::Config;
use yode_core::context::AgentContext;
use yode_core::db::Database;
use yode_core::engine::{AgentEngine, ConfirmResponse, EngineEvent};
use yode_core::permission::{PermissionManager, PermissionRule, RuleBehavior, RuleSource};
use yode_core::session::Session;
use yode_llm::registry::ProviderRegistry;
use yode_runtime::resolved_provider_id;
use yode_tools::registry::ToolRegistry;
use yode_tools::tool::McpResourceProvider;

use crate::browser_settings::{
    apply_browser_settings_env, browser_settings_from_desktop_settings, normalize_browser_settings,
    validate_browser_settings,
};
use crate::computer_use_settings::{
    application_display_name, computer_use_settings_from_desktop_settings,
    normalize_computer_use_settings, validate_computer_use_settings,
};
use crate::desktop_settings_store::{
    desktop_bool_setting, desktop_string_setting, read_desktop_settings, write_desktop_settings,
};
use crate::git_settings::{
    apply_git_settings_env, git_settings_from_desktop_settings, normalize_git_settings,
    validate_git_settings,
};
use crate::hook_settings::{
    build_desktop_hook_manager, hooks_settings_from_desktop_settings, normalize_hooks_settings,
    validate_hooks_settings,
};
use crate::license_notices::read_license_notices;
use crate::protocol::{
    Bootstrap, BrowserSettings, ComputerUseSettings, ConfigurationState,
    ConfigurationUpdateRequest, DefaultLlm, DesktopActionResult, DesktopEvent, DesktopMcpServer,
    DesktopMcpServerStatus, DesktopMcpState, DesktopProvider, DesktopSettingSetRequest,
    DesktopSettingValue, DesktopWorktree, DiagnosticCheck, GeneralSettings, GitSettings,
    HooksSettings, ImportAiSessionsResult, LicenseNotice, OpenTargetRequest, PersonalizationState,
    RuntimeState, SendMessageRequest, TurnAccepted, WorkspaceDiagnosticsResult,
};
use crate::session_helpers::{stored_message_to_message, title_from_content_or_images};
use crate::session_import::{collect_import_files, import_one_ai_session};
use crate::worktree::{
    current_git_branch, delete_worktree, list_git_worktrees, prune_idle_worktrees,
};

mod session_runtime;
mod terminal_runtime;

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
    pub fn new() -> Result<Self> {
        let workspace_path = resolve_desktop_workspace_path();
        let db_path = dirs::home_dir()
            .unwrap_or_else(|| workspace_path.clone())
            .join(".yode")
            .join("sessions.db");

        let config = load_desktop_config(&workspace_path)
            .unwrap_or_else(|_| Config::load_from(None).expect("failed to load default config"));

        let provider_registry = Mutex::new(bootstrap_providers(&config));
        let (tool_registry, mcp_resource_provider) =
            setup_desktop_tooling(&config, &workspace_path);
        if let Ok(settings) = read_desktop_settings() {
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

    pub fn edit_diff_artifact_read(&self, path: String) -> Result<String> {
        read_edit_diff_artifact_from_roots(&path, &self.edit_diff_artifact_roots()?)
    }

    fn edit_diff_artifact_roots(&self) -> Result<Vec<PathBuf>> {
        let active_session_id = self
            .active_session_id
            .lock()
            .map_err(|_| anyhow::anyhow!("active session lock poisoned"))?
            .clone();
        let mut roots = Vec::new();
        if let Some(session_id) = active_session_id {
            if let Some(session) = self.db.get_session(&session_id)? {
                if let Some(project_root) = session.project_root {
                    if !project_root.trim().is_empty() {
                        roots.push(PathBuf::from(project_root));
                    }
                }
            }
        }
        roots.push(self.workspace_path.clone());
        roots.dedup();
        Ok(roots)
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

    pub fn menu_bar_enabled(&self) -> Result<bool> {
        Ok(self
            .general_settings
            .lock()
            .map_err(|_| anyhow::anyhow!("general settings lock poisoned"))?
            .show_in_menu_bar)
    }

    pub fn general_settings_apply(
        &self,
        app: &AppHandle,
        settings: GeneralSettings,
    ) -> Result<GeneralSettings> {
        let effective_mode = permission_mode_from_general_settings(&settings);
        {
            let mut active_mode = self
                .permission_mode
                .lock()
                .map_err(|_| anyhow::anyhow!("permission mode lock poisoned"))?;
            *active_mode = effective_mode.to_string();
        }
        {
            let mut current = self
                .general_settings
                .lock()
                .map_err(|_| anyhow::anyhow!("general settings lock poisoned"))?;
            *current = settings.clone();
        }
        apply_menu_bar_setting(app, settings.show_in_menu_bar)?;
        if !settings.prevent_sleep {
            stop_sleep_guard(&self.sleep_guard);
        }
        Ok(settings)
    }

    pub fn open_target(&self, request: OpenTargetRequest) -> Result<()> {
        let settings = self
            .general_settings
            .lock()
            .map_err(|_| anyhow::anyhow!("general settings lock poisoned"))?
            .clone();
        let target = request
            .target
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(settings.open_destination);
        let path = request
            .path
            .filter(|value| !value.trim().is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| self.workspace_path.clone());
        open_with_destination(&target, &path)
    }

    pub fn import_ai_sessions(&self) -> Result<ImportAiSessionsResult> {
        let Some(paths) = rfd::FileDialog::new()
            .set_title("选择要导入的 AI 会话文件或目录")
            .add_filter("会话文件", &["json", "jsonl", "md", "markdown", "txt"])
            .pick_files()
        else {
            return Ok(ImportAiSessionsResult {
                imported: 0,
                skipped: 0,
                sessions: Vec::new(),
            });
        };

        let config = self
            .config
            .lock()
            .map_err(|_| anyhow::anyhow!("config lock poisoned"))?;
        let (provider, model) = self.default_llm_for_new_session(&config)?;
        drop(config);

        let mut imported_sessions = Vec::new();
        let mut skipped = 0usize;
        for file in collect_import_files(paths) {
            match import_one_ai_session(&self.db, &file, &provider, &model) {
                Ok(Some(session)) => imported_sessions.push(self.map_session(session, None)),
                Ok(None) => skipped += 1,
                Err(err) => {
                    tracing::warn!("Failed to import {}: {}", file.display(), err);
                    skipped += 1;
                }
            }
        }

        Ok(ImportAiSessionsResult {
            imported: imported_sessions.len(),
            skipped,
            sessions: imported_sessions,
        })
    }

    pub fn license_notices(&self) -> Result<Vec<LicenseNotice>> {
        Ok(read_license_notices(&self.workspace_path))
    }

    pub fn configuration_state(&self) -> Result<ConfigurationState> {
        let project_config_path = self.project_config_path();
        let mode = self
            .permission_mode
            .lock()
            .map_err(|_| anyhow::anyhow!("permission mode lock poisoned"))?
            .as_str()
            .parse::<yode_core::permission::PermissionMode>()
            .unwrap_or(yode_core::permission::PermissionMode::Default);
        Ok(ConfigurationState {
            scope: if project_config_path.exists() {
                "Project config".to_string()
            } else {
                "User config".to_string()
            },
            approval_policy: approval_policy_from_permission_mode(mode),
            sandbox_settings: sandbox_settings_from_permission_mode(mode),
            expose_dependencies: load_workspace_dependency_state(),
            config_path: self.user_config_path().display().to_string(),
            project_config_path: project_config_path.display().to_string(),
        })
    }

    pub fn configuration_update(
        &self,
        request: ConfigurationUpdateRequest,
    ) -> Result<ConfigurationState> {
        let scope = if request.scope.to_lowercase().contains("project") {
            ConfigScope::Project
        } else {
            ConfigScope::User
        };
        let permission_mode =
            permission_mode_from_configuration(&request.approval_policy, &request.sandbox_settings);
        {
            let mut runtime_mode = self
                .permission_mode
                .lock()
                .map_err(|_| anyhow::anyhow!("permission mode lock poisoned"))?;
            *runtime_mode = permission_mode.to_string();
        }
        let mut config = self
            .config
            .lock()
            .map_err(|_| anyhow::anyhow!("config lock poisoned"))?;
        config.permissions.default_mode = Some(permission_mode.to_string());
        save_config_to_path(&config, &self.config_path_for_scope(scope))?;
        set_workspace_dependency_state(request.expose_dependencies)?;
        Ok(ConfigurationState {
            scope: request.scope,
            approval_policy: request.approval_policy,
            sandbox_settings: request.sandbox_settings,
            expose_dependencies: request.expose_dependencies,
            config_path: self.user_config_path().display().to_string(),
            project_config_path: self.project_config_path().display().to_string(),
        })
    }

    pub fn open_configuration_file(&self, scope: String) -> Result<()> {
        let path = if scope.to_lowercase().contains("project") {
            self.project_config_path()
        } else {
            self.user_config_path()
        };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        if !path.exists() {
            let config = self
                .config
                .lock()
                .map_err(|_| anyhow::anyhow!("config lock poisoned"))?
                .clone();
            save_config_to_path(&config, &path)?;
        }
        open_with_destination("VS Code", &path)
    }

    pub fn diagnose_workspace(&self) -> Result<WorkspaceDiagnosticsResult> {
        let report_dir = self.workspace_path.join(".yode").join("diagnostics");
        std::fs::create_dir_all(&report_dir)?;
        let report_path = report_dir.join(format!(
            "diagnostics-{}.md",
            Utc::now().format("%Y%m%d-%H%M%S")
        ));
        let checks = workspace_diagnostic_checks(self)?;
        let mut report = String::from("# Yode 工作区诊断\n\n");
        for check in &checks {
            report.push_str(&format!(
                "- [{}] {}: {}\n",
                check.status, check.name, check.detail
            ));
        }
        std::fs::write(&report_path, report)?;
        Ok(WorkspaceDiagnosticsResult {
            report_path: report_path.display().to_string(),
            checks,
        })
    }

    pub fn reinstall_workspace(&self) -> Result<WorkspaceDiagnosticsResult> {
        let cache_dir = self.workspace_path.join(".yode").join("workspace");
        if cache_dir.exists() {
            std::fs::remove_dir_all(&cache_dir)?;
        }
        std::fs::create_dir_all(&cache_dir)?;
        std::fs::write(
            cache_dir.join("README.txt"),
            "Yode workspace dependencies are managed here.\n",
        )?;
        set_workspace_dependency_state(true)?;
        self.diagnose_workspace()
    }

    pub fn desktop_setting_get(&self, key: String) -> Result<DesktopSettingValue> {
        let settings = read_desktop_settings()?;
        Ok(DesktopSettingValue {
            value: settings.get(&key).cloned(),
            key,
        })
    }

    pub fn desktop_setting_set(
        &self,
        request: DesktopSettingSetRequest,
    ) -> Result<DesktopSettingValue> {
        let mut settings = read_desktop_settings()?;
        settings.insert(request.key.clone(), request.value.clone());
        write_desktop_settings(&settings)?;
        Ok(DesktopSettingValue {
            key: request.key,
            value: Some(request.value),
        })
    }

    pub fn personalization_state(&self) -> Result<PersonalizationState> {
        personalization_state_from_settings(&read_desktop_settings()?)
    }

    pub fn personalization_reset_memories(&self) -> Result<DesktopActionResult> {
        let mut removed = 0usize;
        for root in self.memory_roots()? {
            for path in [
                yode_core::session_memory::session_memory_path(&root),
                yode_core::session_memory::live_session_memory_path(&root),
                root.join("MEMORY.md"),
            ] {
                if path.exists() {
                    std::fs::remove_file(&path).with_context(|| {
                        format!("Failed to remove memory file: {}", path.display())
                    })?;
                    removed += 1;
                }
            }
            let memory_dir = root.join(".yode").join("memory");
            if memory_dir.exists() {
                std::fs::remove_dir_all(&memory_dir).with_context(|| {
                    format!(
                        "Failed to remove memory directory: {}",
                        memory_dir.display()
                    )
                })?;
                removed += 1;
            }
        }

        let mut settings = read_desktop_settings()?;
        settings.insert("yode-enable-memories".to_string(), json!(false));
        settings.insert("yode-skip-tool-chats".to_string(), json!(false));
        write_desktop_settings(&settings)?;

        Ok(DesktopActionResult {
            ok: true,
            message: if removed == 0 {
                "未发现需要清理的长期记忆，已关闭长期记忆。".to_string()
            } else {
                format!("已清理 {} 个长期记忆文件或目录，并关闭长期记忆。", removed)
            },
            path: None,
        })
    }

    pub fn mcp_servers_state(&self) -> Result<DesktopMcpState> {
        let config = self
            .config
            .lock()
            .map_err(|_| anyhow::anyhow!("config lock poisoned"))?;
        let servers = desktop_mcp_servers_from_config(&config);
        let statuses = mcp_statuses_from_servers(&servers, None);
        Ok(DesktopMcpState {
            config_path: self.user_config_path().display().to_string(),
            servers,
            statuses,
        })
    }

    pub fn mcp_servers_save(&self, servers: Vec<DesktopMcpServer>) -> Result<DesktopMcpState> {
        validate_desktop_mcp_servers(&servers)?;
        {
            let mut config = self
                .config
                .lock()
                .map_err(|_| anyhow::anyhow!("config lock poisoned"))?;
            config.mcp.servers = desktop_mcp_servers_to_config(&servers)?;
            save_config_to_path(&config, &self.user_config_path())?;
        }
        self.reload_desktop_tooling()?;
        self.mcp_servers_state()
    }

    pub fn mcp_server_test(&self, server: DesktopMcpServer) -> Result<DesktopMcpServerStatus> {
        validate_desktop_mcp_servers(std::slice::from_ref(&server))?;
        let config = desktop_mcp_server_to_config(&server)?;
        let mcp_config = core_mcp_server_to_runtime(&config);
        tauri::async_runtime::block_on(async move {
            if server.disabled {
                return Ok(DesktopMcpServerStatus {
                    name: server.name,
                    state: "disabled".to_string(),
                    detail: "服务器已禁用。".to_string(),
                    tool_count: 0,
                    resource_count: 0,
                    template_count: 0,
                });
            }
            match yode_mcp::McpClient::connect(&server.name, &mcp_config).await {
                Ok(client) => {
                    let tools = client.discover_wrapped_tools().await.unwrap_or_default();
                    let resources = client.list_resources().await.unwrap_or_default();
                    let templates = client.list_resource_templates().await.unwrap_or_default();
                    let _ = client.shutdown().await;
                    Ok(DesktopMcpServerStatus {
                        name: server.name,
                        state: "ready".to_string(),
                        detail: format!(
                            "连接成功，发现 {} 个工具、{} 个资源、{} 个资源模板。",
                            tools.len(),
                            resources.len(),
                            templates.len()
                        ),
                        tool_count: tools.len(),
                        resource_count: resources.len(),
                        template_count: templates.len(),
                    })
                }
                Err(err) => Ok(DesktopMcpServerStatus {
                    name: server.name,
                    state: "failed".to_string(),
                    detail: err.to_string(),
                    tool_count: 0,
                    resource_count: 0,
                    template_count: 0,
                }),
            }
        })
    }

    pub fn mcp_servers_reload(&self) -> Result<DesktopMcpState> {
        self.reload_desktop_tooling()?;
        self.mcp_servers_state()
    }

    fn reload_desktop_tooling(&self) -> Result<()> {
        let config = self
            .config
            .lock()
            .map_err(|_| anyhow::anyhow!("config lock poisoned"))?
            .clone();
        let (tool_registry, mcp_resource_provider) =
            setup_desktop_tooling(&config, &self.workspace_path);
        *self
            .tool_registry
            .lock()
            .map_err(|_| anyhow::anyhow!("tool registry lock poisoned"))? = tool_registry;
        *self
            .mcp_resource_provider
            .lock()
            .map_err(|_| anyhow::anyhow!("mcp resource provider lock poisoned"))? =
            mcp_resource_provider;
        Ok(())
    }

    fn memory_roots(&self) -> Result<Vec<PathBuf>> {
        let mut roots = Vec::new();
        let mut seen = HashSet::new();
        for root in [
            self.workspace_path.clone(),
            dirs::home_dir().unwrap_or_else(|| self.workspace_path.clone()),
        ] {
            let key = root.display().to_string();
            if seen.insert(key) {
                roots.push(root);
            }
        }
        for session in self.db.list_sessions(1_000)? {
            if let Some(root) = session
                .project_root
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .map(PathBuf::from)
            {
                let key = root.display().to_string();
                if seen.insert(key) {
                    roots.push(root);
                }
            }
        }
        Ok(roots)
    }

    pub fn browser_clear_data(&self) -> Result<DesktopActionResult> {
        let mut cleared = Vec::new();
        for path in [
            self.workspace_path.join(".yode").join("browser-cache"),
            dirs::home_dir()
                .unwrap_or_else(|| self.workspace_path.clone())
                .join(".yode")
                .join("browser-data"),
        ] {
            if path.exists() {
                std::fs::remove_dir_all(&path)?;
                cleared.push(path.display().to_string());
            }
            std::fs::create_dir_all(&path)?;
        }
        Ok(DesktopActionResult {
            ok: true,
            message: if cleared.is_empty() {
                "浏览器数据目录已初始化。".to_string()
            } else {
                format!("已清理 {} 个浏览器数据目录。", cleared.len())
            },
            path: Some(self.workspace_path.join(".yode").display().to_string()),
        })
    }

    pub fn browser_settings_get(&self) -> Result<BrowserSettings> {
        browser_settings_from_desktop_settings(&read_desktop_settings()?)
    }

    pub fn browser_settings_apply(&self, settings: BrowserSettings) -> Result<BrowserSettings> {
        validate_browser_settings(&settings)?;
        let normalized = normalize_browser_settings(settings);
        let mut desktop_settings = read_desktop_settings()?;
        desktop_settings.insert(
            "yode-browser-enabled".to_string(),
            json!(normalized.enabled),
        );
        desktop_settings.insert(
            "yode-browser-annotation-screenshots".to_string(),
            json!(normalized.annotation_screenshots),
        );
        desktop_settings.insert(
            "yode-browser-approval".to_string(),
            json!(normalized.approval_policy),
        );
        desktop_settings.insert(
            "yode-browser-blocked-domains".to_string(),
            json!(normalized.blocked_domains),
        );
        desktop_settings.insert(
            "yode-browser-allowed-domains".to_string(),
            json!(normalized.allowed_domains),
        );
        write_desktop_settings(&desktop_settings)?;
        apply_browser_settings_env(&normalized);
        Ok(normalized)
    }

    pub fn hooks_settings_get(&self) -> Result<HooksSettings> {
        hooks_settings_from_desktop_settings(&read_desktop_settings()?)
    }

    pub fn hooks_settings_apply(&self, settings: HooksSettings) -> Result<HooksSettings> {
        let normalized = normalize_hooks_settings(settings);
        validate_hooks_settings(&normalized)?;
        let mut desktop_settings = read_desktop_settings()?;
        desktop_settings.insert("yode-hooks-enabled".to_string(), json!(normalized.enabled));
        desktop_settings.insert("yode-hooks-list".to_string(), json!(normalized.hooks));
        write_desktop_settings(&desktop_settings)?;
        Ok(normalized)
    }

    pub fn git_settings_get(&self) -> Result<GitSettings> {
        git_settings_from_desktop_settings(&read_desktop_settings()?)
    }

    pub fn git_settings_apply(&self, settings: GitSettings) -> Result<GitSettings> {
        let normalized = normalize_git_settings(settings);
        validate_git_settings(&normalized)?;
        let mut desktop_settings = read_desktop_settings()?;
        desktop_settings.insert(
            "yode-git-branch-prefix".to_string(),
            json!(normalized.branch_prefix),
        );
        desktop_settings.insert(
            "yode-git-merge-method".to_string(),
            json!(normalized.merge_method),
        );
        desktop_settings.insert(
            "yode-git-show-pr-icons".to_string(),
            json!(normalized.show_pr_icons),
        );
        desktop_settings.insert(
            "yode-git-always-force-push".to_string(),
            json!(normalized.always_force_push),
        );
        desktop_settings.insert(
            "yode-git-create-draft-prs".to_string(),
            json!(normalized.create_draft_prs),
        );
        desktop_settings.insert(
            "yode-git-auto-delete-worktrees".to_string(),
            json!(normalized.auto_delete_worktrees),
        );
        desktop_settings.insert(
            "yode-git-auto-delete-limit".to_string(),
            json!(normalized.auto_delete_limit),
        );
        desktop_settings.insert(
            "yode-git-commit-instructions".to_string(),
            json!(normalized.commit_instructions),
        );
        desktop_settings.insert(
            "yode-git-pr-instructions".to_string(),
            json!(normalized.pr_instructions),
        );
        write_desktop_settings(&desktop_settings)?;
        apply_git_settings_env(&normalized);
        Ok(normalized)
    }

    pub fn git_current_branch(&self, workspace_path: Option<String>) -> Result<Option<String>> {
        let workspace_path = workspace_path
            .map(PathBuf::from)
            .unwrap_or_else(|| self.workspace_path.clone());
        current_git_branch(&workspace_path)
    }

    pub fn worktrees_list(&self) -> Result<Vec<DesktopWorktree>> {
        list_git_worktrees(&self.workspace_path)
    }

    pub fn worktrees_prune_idle(&self) -> Result<DesktopActionResult> {
        prune_idle_worktrees(&self.workspace_path)
    }

    pub fn worktree_delete(&self, path: String) -> Result<DesktopActionResult> {
        delete_worktree(&self.workspace_path, path)
    }

    pub fn computer_use_open_accessibility(&self) -> Result<DesktopActionResult> {
        #[cfg(target_os = "macos")]
        {
            let _ = Command::new("open")
                .arg(
                    "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility",
                )
                .status();
        }
        Ok(DesktopActionResult {
            ok: true,
            message: "已打开系统辅助功能权限设置，请为 Yode 授权。".to_string(),
            path: None,
        })
    }

    pub fn computer_use_open_chrome(&self) -> Result<DesktopActionResult> {
        #[cfg(target_os = "macos")]
        let status = Command::new("open")
            .args(["-a", "Google Chrome"])
            .status()
            .context("无法打开 Google Chrome")?;

        #[cfg(not(target_os = "macos"))]
        let status = Command::new("google-chrome")
            .status()
            .or_else(|_| Command::new("chrome").status())
            .context("无法打开 Google Chrome")?;

        Ok(DesktopActionResult {
            ok: status.success(),
            message: if status.success() {
                "已打开 Google Chrome，请确认扩展连接状态。".to_string()
            } else {
                "尝试打开 Google Chrome 失败，请确认已安装浏览器。".to_string()
            },
            path: None,
        })
    }

    pub fn computer_use_pick_application(&self) -> Result<DesktopActionResult> {
        let dialog = rfd::FileDialog::new().set_title("选择始终允许的应用");
        #[cfg(target_os = "macos")]
        let dialog = dialog.set_directory("/Applications");
        let Some(path) = dialog.pick_folder() else {
            return Ok(DesktopActionResult {
                ok: false,
                message: "已取消选择应用。".to_string(),
                path: None,
            });
        };
        let app_name = application_display_name(&path);
        if app_name.trim().is_empty() {
            anyhow::bail!("无法识别应用名称。");
        }
        Ok(DesktopActionResult {
            ok: true,
            message: app_name,
            path: Some(path.display().to_string()),
        })
    }

    pub fn computer_use_settings_get(&self) -> Result<ComputerUseSettings> {
        computer_use_settings_from_desktop_settings(&read_desktop_settings()?)
    }

    pub fn computer_use_settings_apply(
        &self,
        settings: ComputerUseSettings,
    ) -> Result<ComputerUseSettings> {
        validate_computer_use_settings(&settings)?;
        let normalized = normalize_computer_use_settings(settings);
        let mut desktop_settings = read_desktop_settings()?;
        desktop_settings.insert(
            "yode-computer-use-anyapp".to_string(),
            json!(normalized.any_app_status),
        );
        desktop_settings.insert(
            "yode-computer-use-chrome".to_string(),
            json!(normalized.chrome_status),
        );
        desktop_settings.insert(
            "yode-computer-use-allowed-apps".to_string(),
            json!(normalized.allowed_apps),
        );
        write_desktop_settings(&desktop_settings)?;
        Ok(normalized)
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

    fn config_path_for_scope(&self, scope: ConfigScope) -> PathBuf {
        match scope {
            ConfigScope::User => self.user_config_path(),
            ConfigScope::Project => self.project_config_path(),
        }
    }

    pub fn turn_send_message(
        &self,
        app: AppHandle,
        request: SendMessageRequest,
    ) -> Result<TurnAccepted> {
        let config = self
            .config
            .lock()
            .map_err(|_| anyhow::anyhow!("config lock poisoned"))?;
        let content = request.content.trim().to_string();
        let images = request
            .images
            .into_iter()
            .filter(|image| {
                !image.base64.trim().is_empty() && image.media_type.starts_with("image/")
            })
            .map(|image| yode_llm::types::ImageData {
                base64: image.base64,
                media_type: image.media_type,
            })
            .collect::<Vec<_>>();
        if content.is_empty() && images.is_empty() {
            anyhow::bail!("message content cannot be empty");
        }

        let now = Utc::now();
        let session = if let Some(session_id) = request
            .session_id
            .as_deref()
            .filter(|id| !id.trim().is_empty())
        {
            let mut s = self
                .db
                .get_session(session_id)?
                .with_context(|| format!("session '{}' not found", session_id))?;

            let mut changed = false;
            if let Some(ref req_provider) = request.provider {
                if s.provider != *req_provider {
                    s.provider = req_provider.clone();
                    changed = true;
                }
            }
            if let Some(ref req_model) = request.model {
                if s.model != *req_model {
                    s.model = req_model.clone();
                    changed = true;
                }
            }
            if changed {
                self.normalize_session_llm(&mut s, &config);
                self.db.update_session_llm(&s.id, &s.provider, &s.model)?;
            }
            s
        } else {
            let (default_provider, default_model) = self.default_llm_for_new_session(&config)?;
            let session = Session {
                id: Uuid::new_v4().to_string(),
                name: request
                    .title
                    .filter(|title| !title.trim().is_empty())
                    .or_else(|| Some(title_from_content_or_images(&content, images.len()))),
                project_root: if request.standalone.unwrap_or(false) {
                    None
                } else {
                    request
                        .project_root
                        .filter(|root| !root.trim().is_empty())
                        .or_else(|| Some(self.workspace_path.display().to_string()))
                },
                provider: request.provider.unwrap_or(default_provider),
                model: request.model.unwrap_or(default_model),
                created_at: now,
                updated_at: now,
            };
            let mut session = session;
            self.normalize_session_llm(&mut session, &config);
            self.db.create_session(&session)?;
            session
        };

        self.set_active_session(session.id.clone())?;
        self.db.touch_session(&session.id)?;
        let accepted_session = self.map_session(session.clone(), Some(session.id.as_str()));

        let turn_id = Uuid::new_v4().to_string();
        let session_id = session.id.clone();
        let emit_turn_id = turn_id.clone();
        let seq_base = self.seq.fetch_add(100, Ordering::SeqCst);

        let provider = self
            .provider_registry
            .lock()
            .map_err(|_| anyhow::anyhow!("registry lock poisoned"))?
            .get(&session.provider)
            .ok_or_else(|| {
                anyhow::anyhow!("Provider '{}' not found in registry", session.provider)
            })?;

        let turn_workspace_path = session
            .project_root
            .as_deref()
            .filter(|root| !root.trim().is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| self.workspace_path.clone());

        let mut permissions = configure_desktop_permissions(&config, &turn_workspace_path);
        if let Ok(active_mode_guard) = self.permission_mode.lock() {
            if let Ok(mode) = active_mode_guard.parse::<yode_core::permission::PermissionMode>() {
                permissions.set_mode(mode);
            }
        }
        if let Ok(rules) = self.session_permission_rules.lock() {
            if let Some(session_rules) = rules.get(&session.id) {
                permissions.add_rules(session_rules.clone());
            }
        }
        let mut context = AgentContext::resume(
            session.id.clone(),
            turn_workspace_path,
            session.provider.clone(),
            session.model.clone(),
        );
        let personalization = self.personalization_state()?;
        context.project_memory_enabled = personalization.enable_memories
            && session
                .project_root
                .as_deref()
                .is_some_and(|root| !root.trim().is_empty());
        context.skip_tool_assisted_memory = personalization.skip_tool_chats;
        context.personalization_prompt = build_personalization_prompt(&personalization);
        context.output_style = config.ui.output_style.clone();

        let stored_msgs = self.db.load_messages(&session.id)?;
        let restored_messages: Vec<yode_llm::types::Message> = stored_msgs
            .into_iter()
            .filter_map(stored_message_to_message)
            .collect();

        let tools = self
            .tool_registry
            .lock()
            .map_err(|_| anyhow::anyhow!("tool registry lock poisoned"))?
            .clone();
        let mcp_resource_provider = self
            .mcp_resource_provider
            .lock()
            .map_err(|_| anyhow::anyhow!("mcp resource provider lock poisoned"))?
            .clone();
        let config = config.clone();
        let db_path_clone = self.db_path.clone();
        let hook_manager = build_desktop_hook_manager(&self.workspace_path)?;

        let (confirm_tx, confirm_rx) = unbounded_channel::<ConfirmResponse>();
        {
            let mut txs = self
                .confirm_txs
                .lock()
                .map_err(|_| anyhow::anyhow!("poisoned"))?;
            txs.insert((session_id.clone(), emit_turn_id.clone()), confirm_tx);
        }

        let (ask_user_query_tx, mut ask_user_query_rx) =
            unbounded_channel::<yode_tools::tool::UserQuery>();
        let (ask_user_answer_tx, ask_user_answer_rx) = unbounded_channel::<String>();
        {
            let mut txs = self
                .ask_user_txs
                .lock()
                .map_err(|_| anyhow::anyhow!("poisoned"))?;
            txs.insert(
                (session_id.clone(), emit_turn_id.clone()),
                ask_user_answer_tx,
            );
        }

        let cancel_token = tokio_util::sync::CancellationToken::new();
        {
            let mut tokens = self
                .cancel_tokens
                .lock()
                .map_err(|_| anyhow::anyhow!("poisoned"))?;
            tokens.insert(
                (session_id.clone(), emit_turn_id.clone()),
                cancel_token.clone(),
            );
        }
        let should_prevent_sleep = self
            .general_settings
            .lock()
            .map(|settings| settings.prevent_sleep)
            .unwrap_or(false);
        if should_prevent_sleep {
            start_sleep_guard(&self.sleep_guard);
        }

        let confirm_txs_clone = self.confirm_txs.clone();
        let ask_user_txs_clone = self.ask_user_txs.clone();
        let cancel_tokens_clone = self.cancel_tokens.clone();
        let pending_confirmations_clone = self.pending_confirmations.clone();
        let sleep_guard_clone = self.sleep_guard.clone();

        std::thread::spawn(move || {
            let rt = match tokio::runtime::Runtime::new() {
                Ok(rt) => rt,
                Err(err) => {
                    tracing::error!("Failed to create tokio runtime: {}", err);
                    return;
                }
            };

            rt.block_on(async {
                let mut engine = AgentEngine::new(provider, tools, permissions, context);
                if let Some(hook_manager) = hook_manager {
                    engine.set_hook_manager(hook_manager);
                }
                let db_clone = match Database::open(&db_path_clone) {
                    Ok(db) => db,
                    Err(err) => {
                        tracing::error!("Failed to open database in background thread: {}", err);
                        let desktop_event = DesktopEvent {
                            session_id: session_id.clone(),
                            turn_id: emit_turn_id.clone(),
                            seq: seq_base,
                            kind: "error".to_string(),
                            timestamp: Utc::now().to_rfc3339(),
                            payload: json!({ "body": err.to_string() }),
                        };
                        let _ = app.emit("desktop-event", desktop_event);
                        return;
                    }
                };
                engine.set_database(db_clone);
                if let Some(mcp) = mcp_resource_provider {
                    engine.set_mcp_resource_provider(mcp);
                }
                engine.set_ask_user_channels(ask_user_query_tx, ask_user_answer_rx);
                engine.set_mcp_resource_policy(yode_tools::tool::McpResourcePolicy {
                    allow: config.mcp.resource_allow.clone(),
                    deny: config.mcp.resource_deny.clone(),
                });
                engine.restore_messages(restored_messages);

                let (event_tx, mut event_rx) = unbounded_channel::<EngineEvent>();

                let session_id_str = session_id.clone();
                let turn_id_str = emit_turn_id.clone();

                let error_event_tx = event_tx.clone();
                let handle = tokio::spawn(async move {
                    if let Err(err) = engine
                        .run_turn_streaming_with_images(
                            &content,
                            images,
                            yode_core::context::QuerySource::User,
                            event_tx,
                            confirm_rx,
                            Some(cancel_token),
                        )
                        .await
                    {
                        tracing::error!("AgentEngine run_turn_streaming failed: {}", err);
                        let _ = error_event_tx.send(EngineEvent::Error(err.to_string()));
                    }
                });

                let mut seq = seq_base;
                loop {
                    let event = tokio::select! {
                        Some(query) = ask_user_query_rx.recv() => {
                            let first_question = query.questions.first();
                            let desktop_event = DesktopEvent {
                                session_id: session_id_str.clone(),
                                turn_id: turn_id_str.clone(),
                                seq,
                                kind: "ask_user".to_string(),
                                timestamp: Utc::now().to_rfc3339(),
                                payload: json!({
                                    "id": query.id,
                                    "title": first_question.map(|question| question.header.clone()).unwrap_or_else(|| "需要用户输入".to_string()),
                                    "body": first_question.map(|question| question.question.clone()).unwrap_or_else(|| "请在输入框回复。".to_string()),
                                    "query": query
                                }),
                            };
                            let _ = app.emit("desktop-event", desktop_event);
                            seq += 1;
                            continue;
                        }
                        Some(event) = event_rx.recv() => event,
                        else => break,
                    };
                    let mapped = yode_runtime::engine_event_to_runtime_parts(event);
                    if let Some(pending_confirmation) = mapped.pending_confirmation.as_ref() {
                        if let Ok(mut pending) = pending_confirmations_clone.lock() {
                            pending.insert(
                                (session_id_str.clone(), turn_id_str.clone()),
                                PendingConfirmation {
                                    tool_name: pending_confirmation.tool_name.clone(),
                                    command: pending_confirmation.command.clone(),
                                },
                            );
                        }
                    }
                    let kind = mapped.kind;
                    let payload = mapped.payload;

                    if std::env::var("YODE_ACTION_NARRATIVE_DEBUG")
                        .is_ok_and(|value| value == "1")
                        && matches!(
                            kind,
                            "assistant_text_delta"
                                | "assistant_reasoning_delta"
                                | "action_narrative"
                                | "tool_started"
                                | "assistant_text_complete"
                                | "assistant_reasoning_complete"
                                | "turn_completed"
                        )
                    {
                        let preview = payload
                            .get("body")
                            .or_else(|| payload.get("reasoning"))
                            .and_then(|value| value.as_str())
                            .unwrap_or("")
                            .chars()
                            .take(120)
                            .collect::<String>()
                            .replace('\n', "\\n");
                        eprintln!(
                            "[action-narrative-debug] turn={} kind={} preview={:?}",
                            turn_id_str, kind, preview
                        );
                    }

                    let desktop_event = DesktopEvent {
                        session_id: session_id_str.clone(),
                        turn_id: turn_id_str.clone(),
                        seq,
                        kind: kind.to_string(),
                        timestamp: Utc::now().to_rfc3339(),
                        payload,
                    };

                    let _ = app.emit("desktop-event", desktop_event);
                    seq += 1;
                }

                let _ = handle.await;

                if let Ok(mut txs) = confirm_txs_clone.lock() {
                    txs.remove(&(session_id.clone(), emit_turn_id.clone()));
                }
                if let Ok(mut txs) = ask_user_txs_clone.lock() {
                    txs.remove(&(session_id.clone(), emit_turn_id.clone()));
                }
                if let Ok(mut tokens) = cancel_tokens_clone.lock() {
                    let _: Option<tokio_util::sync::CancellationToken> =
                        tokens.remove(&(session_id.clone(), emit_turn_id.clone()));
                    if tokens.is_empty() {
                        drop(tokens);
                        stop_sleep_guard(&sleep_guard_clone);
                    }
                }
                if let Ok(mut pending) = pending_confirmations_clone.lock() {
                    pending.remove(&(session_id.clone(), emit_turn_id.clone()));
                }
            });
        });

        Ok(TurnAccepted {
            session_id: session.id,
            turn_id,
            session: accepted_session,
        })
    }

    pub fn permission_respond(
        &self,
        session_id: String,
        turn_id: String,
        allow: bool,
        always_allow: bool,
    ) -> Result<()> {
        let pending_request = self
            .pending_confirmations
            .lock()
            .ok()
            .and_then(|mut pending| pending.remove(&(session_id.clone(), turn_id.clone())));

        if allow && always_allow {
            if let Some(request) = pending_request {
                let rule = PermissionRule {
                    source: RuleSource::Session,
                    behavior: RuleBehavior::Allow,
                    tool_name: request.tool_name,
                    category: None,
                    pattern: request.command,
                    description: Some("Allowed from desktop confirmation prompt".to_string()),
                };
                let mut rules = self
                    .session_permission_rules
                    .lock()
                    .map_err(|_| anyhow::anyhow!("poisoned"))?;
                rules.entry(session_id.clone()).or_default().push(rule);
            }
        }

        let tx = self
            .confirm_txs
            .lock()
            .map_err(|_| anyhow::anyhow!("poisoned"))?
            .get(&(session_id, turn_id))
            .cloned();
        if let Some(tx) = tx {
            let response = if allow && always_allow {
                ConfirmResponse::AllowAlways
            } else if allow {
                ConfirmResponse::Allow
            } else {
                ConfirmResponse::Deny
            };
            let _ = tx.send(response);
        }
        Ok(())
    }

    pub fn ask_user_respond(
        &self,
        session_id: String,
        turn_id: String,
        answer: String,
    ) -> Result<()> {
        let txs = self
            .ask_user_txs
            .lock()
            .map_err(|_| anyhow::anyhow!("poisoned"))?;
        if let Some(tx) = txs.get(&(session_id, turn_id)) {
            let _ = tx.send(answer);
        }
        Ok(())
    }

    pub fn turn_cancel(&self, session_id: String, turn_id: String) -> Result<()> {
        let mut tokens = self
            .cancel_tokens
            .lock()
            .map_err(|_| anyhow::anyhow!("poisoned"))?;
        if let Some(token) = tokens.remove(&(session_id, turn_id)) {
            let token: tokio_util::sync::CancellationToken = token;
            token.cancel();
        }
        Ok(())
    }

    pub fn config_get_providers(&self) -> Result<Vec<DesktopProvider>> {
        let config = self
            .config
            .lock()
            .map_err(|_| anyhow::anyhow!("config lock poisoned"))?;
        let mut providers = Vec::new();
        for (id, p) in &config.llm.providers {
            let resolved_id = resolved_provider_id(id, p);
            if resolved_id.trim().is_empty() {
                continue;
            }
            let name = match resolved_id.as_str() {
                "anthropic" => "Anthropic Claude".to_string(),
                "openai" => "OpenAI".to_string(),
                "google" | "gemini" => "Google Gemini".to_string(),
                "deepseek" => "DeepSeek (深度求索)".to_string(),
                "doubao" => "豆包".to_string(),
                "ollama" => "Ollama (本地运行)".to_string(),
                _ => resolved_id.to_uppercase(),
            };
            providers.push(DesktopProvider {
                id: resolved_id,
                name,
                format: p.format.clone(),
                enabled: p.enabled.unwrap_or(true),
                api_key: p.api_key.clone().unwrap_or_default(),
                base_url: p.base_url.clone().unwrap_or_default(),
                models: p.models.clone(),
                gradient: p.gradient.clone(),
            });
        }
        let order = [
            "openai",
            "anthropic",
            "gemini",
            "google",
            "deepseek",
            "ollama",
        ];
        providers.sort_by_key(|p| order.iter().position(|&x| x == p.id).unwrap_or(99));
        Ok(providers)
    }

    pub fn config_get_default_llm(&self) -> Result<DefaultLlm> {
        let config = self
            .config
            .lock()
            .map_err(|_| anyhow::anyhow!("config lock poisoned"))?;
        Ok(DefaultLlm {
            provider: config.llm.default_provider.clone(),
            model: config.llm.default_model.clone(),
        })
    }

    pub fn config_set_default_llm(&self, provider: String, model: String) -> Result<DefaultLlm> {
        let provider = provider.trim().to_string();
        let model = model.trim().to_string();
        if provider.is_empty() || model.is_empty() {
            anyhow::bail!("provider and model cannot be empty");
        }
        let mut config = self
            .config
            .lock()
            .map_err(|_| anyhow::anyhow!("config lock poisoned"))?;
        if !config
            .llm
            .providers
            .iter()
            .any(|(id, provider_config)| resolved_provider_id(id, provider_config) == provider)
        {
            anyhow::bail!("Provider '{}' not found", provider);
        }
        let (provider, model) = normalized_provider_model(&config, &provider, &model);
        config.llm.default_provider = provider;
        config.llm.default_model = model;
        config.save()?;
        Ok(DefaultLlm {
            provider: config.llm.default_provider.clone(),
            model: config.llm.default_model.clone(),
        })
    }

    pub fn config_save_providers(&self, providers: Vec<DesktopProvider>) -> Result<()> {
        let mut config = self
            .config
            .lock()
            .map_err(|_| anyhow::anyhow!("config lock poisoned"))?;
        let mut new_providers = std::collections::HashMap::new();
        for p in providers {
            let id = p.id.trim().to_string();
            if id.is_empty() {
                continue;
            }
            new_providers.insert(
                id,
                yode_core::config::ProviderConfig {
                    format: p.format,
                    base_url: if p.base_url.is_empty() {
                        None
                    } else {
                        Some(p.base_url)
                    },
                    api_key: if p.api_key.is_empty() {
                        None
                    } else {
                        Some(p.api_key)
                    },
                    models: p.models,
                    enabled: Some(p.enabled),
                    gradient: p.gradient,
                },
            );
        }
        if !new_providers.iter().any(|(id, provider_config)| {
            resolved_provider_id(id, provider_config) == config.llm.default_provider
        }) {
            if let Some((provider, config_provider)) = new_providers
                .iter()
                .find(|(_, provider)| provider.enabled.unwrap_or(true))
                .or_else(|| new_providers.iter().next())
            {
                config.llm.default_provider = provider.clone();
                config.llm.default_model = config_provider
                    .models
                    .first()
                    .cloned()
                    .unwrap_or_else(|| config.llm.default_model.clone());
            }
        }
        config.llm.providers = new_providers;
        let (provider, model) = normalized_provider_model(
            &config,
            &config.llm.default_provider,
            &config.llm.default_model,
        );
        config.llm.default_provider = provider;
        config.llm.default_model = model;
        config.save()?;

        let new_registry = bootstrap_providers(&config);
        let mut reg_guard = self
            .provider_registry
            .lock()
            .map_err(|_| anyhow::anyhow!("registry lock poisoned"))?;
        *reg_guard = new_registry;

        Ok(())
    }

    pub async fn config_test_provider(&self, p: DesktopProvider) -> Result<()> {
        let api_key = resolve_provider_api_key(&p.id, &p.format, p.api_key.trim());
        let base_url = resolve_provider_base_url(&p.id, &p.format, p.base_url.trim());
        let provider: Arc<dyn yode_llm::provider::LlmProvider> = match p.format.as_str() {
            "anthropic" => Arc::new(yode_llm::providers::anthropic::AnthropicProvider::new(
                &p.id, &api_key, &base_url,
            )),
            "gemini" => {
                let mut provider = yode_llm::providers::gemini::GeminiProvider::new(&api_key);
                if base_url != "https://generativelanguage.googleapis.com/v1beta" {
                    provider = provider.with_base_url(&base_url);
                }
                Arc::new(provider)
            }
            _ => Arc::new(yode_llm::providers::openai::OpenAiProvider::new(
                &p.id, &api_key, &base_url,
            )),
        };

        let _models = provider.list_models().await?;
        Ok(())
    }
}

fn resolve_provider_api_key(id: &str, format: &str, configured: &str) -> String {
    if !configured.is_empty() {
        return configured.to_string();
    }

    let env_prefix = id.to_uppercase().replace('-', "_");
    let mut candidates = vec![format!("{}_API_KEY", env_prefix)];
    candidates.extend(match (id, format) {
        ("anthropic", _) | (_, "anthropic") => vec![
            "ANTHROPIC_API_KEY".to_string(),
            "ANTHROPIC_AUTH_TOKEN".to_string(),
        ],
        ("gemini", _) | ("google", _) | (_, "gemini") => {
            vec!["GOOGLE_API_KEY".to_string(), "GEMINI_API_KEY".to_string()]
        }
        ("deepseek", _) => vec!["DEEPSEEK_API_KEY".to_string()],
        ("openai", _) => vec!["OPENAI_API_KEY".to_string()],
        _ => Vec::new(),
    });

    candidates
        .into_iter()
        .find_map(|key| std::env::var(key).ok())
        .unwrap_or_default()
}

fn resolve_provider_base_url(id: &str, format: &str, configured: &str) -> String {
    let env_prefix = id.to_uppercase().replace('-', "_");
    let override_base = format!("{}_BASE_URL", env_prefix);
    if let Ok(url) = std::env::var(override_base) {
        return url;
    }
    if !configured.is_empty() {
        return configured.to_string();
    }
    match format {
        "anthropic" => "https://api.anthropic.com".to_string(),
        "gemini" => "https://generativelanguage.googleapis.com/v1beta".to_string(),
        _ => "https://api.openai.com/v1".to_string(),
    }
}

fn normalized_provider_model(config: &Config, provider: &str, model: &str) -> (String, String) {
    let provider = provider.trim();
    let model = model.trim();

    let configured_provider = config
        .llm
        .providers
        .iter()
        .find(|(id, provider_config)| {
            resolved_provider_id(id, provider_config) == provider
                && provider_config.enabled.unwrap_or(true)
        })
        .map(|(_, provider_config)| provider_config);

    if let Some(provider_config) = configured_provider {
        if provider_config.models.is_empty()
            || provider_config
                .models
                .iter()
                .any(|candidate| candidate == model)
        {
            return (provider.to_string(), model.to_string());
        }
        if let Some(first_model) = provider_config.models.first() {
            return (provider.to_string(), first_model.clone());
        }
    }

    if let Some(default_provider) = config
        .llm
        .providers
        .get(&config.llm.default_provider)
        .filter(|provider_config| provider_config.enabled.unwrap_or(true))
    {
        let fallback_model = default_provider
            .models
            .first()
            .cloned()
            .unwrap_or_else(|| config.llm.default_model.clone());
        return (config.llm.default_provider.clone(), fallback_model);
    }

    if let Some((fallback_provider, fallback_config)) =
        config.llm.providers.iter().find(|(id, provider_config)| {
            !resolved_provider_id(id, provider_config).trim().is_empty()
                && provider_config.enabled.unwrap_or(true)
        })
    {
        let fallback_model = fallback_config
            .models
            .first()
            .cloned()
            .unwrap_or_else(|| config.llm.default_model.clone());
        return (
            resolved_provider_id(fallback_provider, fallback_config),
            fallback_model,
        );
    }

    (
        config.llm.default_provider.clone(),
        config.llm.default_model.clone(),
    )
}

fn bootstrap_providers(config: &Config) -> Arc<ProviderRegistry> {
    yode_runtime::bootstrap_registry_only(config)
}

fn setup_desktop_tooling(
    config: &Config,
    workdir: &std::path::Path,
) -> (Arc<ToolRegistry>, Option<Arc<dyn McpResourceProvider>>) {
    let tool_registry = ToolRegistry::new();
    yode_tools::builtin::register_builtin_tools(&tool_registry);

    let mut mcp_clients = Vec::new();
    for (name, server_config) in &config.mcp.servers {
        if server_config.disabled {
            continue;
        }
        let mcp_config = core_mcp_server_to_runtime(server_config);

        if let Ok(client) = tauri::async_runtime::block_on(async {
            yode_mcp::McpClient::connect(name, &mcp_config).await
        }) {
            if let Ok(wrappers) =
                tauri::async_runtime::block_on(async { client.discover_wrapped_tools().await })
            {
                for wrapper in wrappers {
                    tool_registry.register(wrapper);
                }
            }
            mcp_clients.push(client);
        }
    }

    let skill_paths = yode_core::skills::SkillRegistry::default_paths(workdir);
    let skill_registry = yode_core::skills::SkillRegistry::discover(&skill_paths);
    use yode_tools::builtin::skill::{SkillContextMode, SkillEntry, SkillStore};
    let mut store = SkillStore::new();
    for skill in skill_registry.list() {
        let context = match skill.metadata.context {
            yode_core::skills::SkillContextMode::Inline => SkillContextMode::Inline,
            yode_core::skills::SkillContextMode::Fork => SkillContextMode::Fork,
        };
        store.add_entry(SkillEntry {
            name: skill.name.clone(),
            description: skill.description.clone(),
            content: skill.content.clone(),
            allowed_tools: skill.metadata.allowed_tools.clone(),
            paths: skill.metadata.paths.clone(),
            trigger_examples: skill.metadata.trigger_examples.clone(),
            context,
            model: skill.metadata.model.clone(),
            effort: skill.metadata.effort.clone(),
        });
    }
    let store = Arc::new(tokio::sync::Mutex::new(store));
    yode_tools::builtin::register_skill_tool(&tool_registry, store);

    let mcp_resource_provider = if !mcp_clients.is_empty() {
        Some(
            Arc::new(yode_mcp::McpClientResourceProvider::new(mcp_clients))
                as Arc<dyn McpResourceProvider>,
        )
    } else {
        None
    };

    (Arc::new(tool_registry), mcp_resource_provider)
}

fn configure_desktop_permissions(config: &Config, _workdir: &std::path::Path) -> PermissionManager {
    let mut permissions =
        PermissionManager::from_confirmation_list(config.tools.require_confirmation.clone());
    if let Some(mode_str) = &config.permissions.default_mode {
        if let Ok(mode) = mode_str.parse::<yode_core::permission::PermissionMode>() {
            permissions.set_mode(mode);
        }
    }
    for rule in &config.permissions.always_allow {
        permissions.add_rule(PermissionRule {
            source: RuleSource::UserConfig,
            behavior: RuleBehavior::Allow,
            tool_name: rule.tool.clone(),
            category: rule.category.clone(),
            pattern: rule.pattern.clone(),
            description: rule.description.clone(),
        });
    }
    for rule in &config.permissions.always_deny {
        permissions.add_rule(PermissionRule {
            source: RuleSource::UserConfig,
            behavior: RuleBehavior::Deny,
            tool_name: rule.tool.clone(),
            category: rule.category.clone(),
            pattern: rule.pattern.clone(),
            description: rule.description.clone(),
        });
    }
    permissions
}

fn read_edit_diff_artifact_from_roots(path: &str, roots: &[PathBuf]) -> Result<String> {
    let clean = path.trim();
    if clean.is_empty() {
        anyhow::bail!("diff artifact path is empty");
    }
    if clean.contains('\0') {
        anyhow::bail!("diff artifact path contains invalid characters");
    }

    let relative = Path::new(clean);
    if relative.is_absolute() {
        anyhow::bail!("diff artifact path must be relative");
    }
    if relative.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        anyhow::bail!("diff artifact path contains unsafe components");
    }

    let mut searched = Vec::new();
    let mut last_error: Option<anyhow::Error> = None;
    let mut candidate_roots = Vec::new();
    for root in roots {
        candidate_roots.push(root.clone());
        if let Ok(entries) = std::fs::read_dir(root) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    candidate_roots.push(path);
                }
            }
        }
    }
    candidate_roots.dedup();

    for root in &candidate_roots {
        let allowed_dir = root.join(".yode").join("edit-diffs");
        searched.push(allowed_dir.display().to_string());
        let target = root.join(relative);
        let canonical_target = match target.canonicalize() {
            Ok(path) => path,
            Err(err) => {
                last_error = Some(
                    anyhow::anyhow!(err).context(format!("Failed to access {}", target.display())),
                );
                continue;
            }
        };
        let canonical_allowed = match allowed_dir.canonicalize() {
            Ok(path) => path,
            Err(err) => {
                last_error = Some(
                    anyhow::anyhow!(err)
                        .context(format!("Failed to access {}", allowed_dir.display())),
                );
                continue;
            }
        };
        if !canonical_target.starts_with(&canonical_allowed) {
            last_error = Some(anyhow::anyhow!(
                "diff artifact path is outside .yode/edit-diffs"
            ));
            continue;
        }

        let metadata = std::fs::metadata(&canonical_target)
            .with_context(|| format!("Failed to inspect {}", canonical_target.display()))?;
        if metadata.len() > 2 * 1024 * 1024 {
            anyhow::bail!("diff artifact is too large to display");
        }

        return std::fs::read_to_string(&canonical_target)
            .with_context(|| format!("Failed to read {}", canonical_target.display()));
    }

    let searched = if searched.is_empty() {
        "no project roots".to_string()
    } else {
        searched.join(", ")
    };
    if let Some(error) = last_error {
        anyhow::bail!(
            "Failed to read diff artifact {}; searched {}; last error: {}",
            clean,
            searched,
            error
        );
    }
    anyhow::bail!(
        "Failed to read diff artifact {}; searched {}",
        clean,
        searched
    )
}

fn resolve_desktop_workspace_path() -> PathBuf {
    let current_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    find_workspace_root(&current_dir).unwrap_or(current_dir)
}

fn find_workspace_root(start: &std::path::Path) -> Option<PathBuf> {
    for ancestor in start.ancestors() {
        if ancestor.join(".git").is_dir() || is_cargo_workspace_root(ancestor) {
            return Some(ancestor.to_path_buf());
        }
    }
    None
}

fn is_cargo_workspace_root(path: &std::path::Path) -> bool {
    std::fs::read_to_string(path.join("Cargo.toml"))
        .map(|content| content.contains("[workspace]"))
        .unwrap_or(false)
}

fn default_general_settings() -> GeneralSettings {
    GeneralSettings {
        work_mode: "coding".to_string(),
        default_file_permission: true,
        auto_review: true,
        full_access: true,
        open_destination: "VS Code".to_string(),
        show_in_menu_bar: true,
        bottom_panel: true,
        terminal_location: "bottom".to_string(),
        prevent_sleep: false,
        code_review_policy: "inline".to_string(),
        suggested_prompts: true,
        context_usage: false,
        follow_up_behavior: "queue".to_string(),
        require_opt_enter: false,
        completion_notification: "Only when unfocused".to_string(),
        permission_notification: true,
        question_notification: true,
    }
}

#[derive(Debug, Clone, Copy)]
enum ConfigScope {
    User,
    Project,
}

fn load_desktop_config(workspace_path: &Path) -> Result<Config> {
    let project_config = workspace_path.join(".yode").join("config.toml");
    if project_config.exists() {
        Config::load_from(Some(&project_config))
    } else {
        Config::load()
    }
}

fn save_config_to_path(config: &Config, path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, toml::to_string_pretty(config)?)?;
    Ok(())
}

fn permission_mode_from_configuration(
    approval_policy: &str,
    sandbox_settings: &str,
) -> yode_core::permission::PermissionMode {
    let approval = approval_policy.to_lowercase();
    if approval.contains("always") || approval.contains("始终") {
        return yode_core::permission::PermissionMode::Bypass;
    }
    if approval.contains("never") || approval.contains("从不") {
        return yode_core::permission::PermissionMode::Plan;
    }

    let sandbox = sandbox_settings.to_lowercase();
    if sandbox.contains("read only") || sandbox.contains("只读") {
        yode_core::permission::PermissionMode::Plan
    } else if sandbox.contains("full") || sandbox.contains("读写") {
        yode_core::permission::PermissionMode::AcceptEdits
    } else {
        yode_core::permission::PermissionMode::Default
    }
}

fn approval_policy_from_permission_mode(mode: yode_core::permission::PermissionMode) -> String {
    match mode {
        yode_core::permission::PermissionMode::Bypass => "Always auto-approve",
        yode_core::permission::PermissionMode::Plan => "Never approve",
        _ => "On request",
    }
    .to_string()
}

fn sandbox_settings_from_permission_mode(mode: yode_core::permission::PermissionMode) -> String {
    match mode {
        yode_core::permission::PermissionMode::Plan => "Read only",
        yode_core::permission::PermissionMode::AcceptEdits
        | yode_core::permission::PermissionMode::Bypass => "Full write access",
        _ => "Restricted",
    }
    .to_string()
}

fn workspace_dependency_state_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".yode")
        .join("desktop-workspace-deps.json")
}

fn load_workspace_dependency_state() -> bool {
    let path = workspace_dependency_state_path();
    let Ok(raw) = std::fs::read_to_string(path) else {
        return true;
    };
    serde_json::from_str::<serde_json::Value>(&raw)
        .ok()
        .and_then(|value| {
            value
                .get("exposeDependencies")
                .and_then(|value| value.as_bool())
        })
        .unwrap_or(true)
}

fn set_workspace_dependency_state(expose: bool) -> Result<()> {
    let path = workspace_dependency_state_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(
        path,
        serde_json::to_string_pretty(&json!({
            "exposeDependencies": expose,
            "updatedAt": Utc::now().to_rfc3339()
        }))?,
    )?;
    Ok(())
}

fn workspace_diagnostic_checks(runtime: &DesktopRuntime) -> Result<Vec<DiagnosticCheck>> {
    let mut checks = Vec::new();
    let user_config = runtime.user_config_path();
    let project_config = runtime.project_config_path();
    checks.push(path_check("用户配置", &user_config, true));
    checks.push(path_check("项目配置", &project_config, false));
    checks.push(path_check("会话数据库", &runtime.db_path, true));
    checks.push(command_check("Node.js", "node", &["--version"]));
    checks.push(command_check("Python", "python3", &["--version"]));
    checks.push(command_check("Cargo", "cargo", &["--version"]));
    checks.push(path_check(
        "桌面端 package.json",
        &runtime
            .workspace_path
            .join("apps")
            .join("yode-desktop")
            .join("package.json"),
        true,
    ));
    checks.push(DiagnosticCheck {
        name: "依赖项暴露".to_string(),
        status: if load_workspace_dependency_state() {
            "ok"
        } else {
            "warn"
        }
        .to_string(),
        detail: if load_workspace_dependency_state() {
            "已允许向工作区暴露 Node.js 与 Python 工具。"
        } else {
            "当前已关闭依赖项暴露。"
        }
        .to_string(),
    });
    Ok(checks)
}

fn path_check(name: &str, path: &Path, required: bool) -> DiagnosticCheck {
    let exists = path.exists();
    DiagnosticCheck {
        name: name.to_string(),
        status: if exists || !required { "ok" } else { "error" }.to_string(),
        detail: if exists {
            path.display().to_string()
        } else if required {
            format!("未找到 {}", path.display())
        } else {
            format!("未创建 {}", path.display())
        },
    }
}

fn command_check(name: &str, command: &str, args: &[&str]) -> DiagnosticCheck {
    match Command::new(command).args(args).output() {
        Ok(output) if output.status.success() => {
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            DiagnosticCheck {
                name: name.to_string(),
                status: "ok".to_string(),
                detail: if stdout.is_empty() { stderr } else { stdout },
            }
        }
        Ok(output) => DiagnosticCheck {
            name: name.to_string(),
            status: "error".to_string(),
            detail: format!("退出码 {}", output.status.code().unwrap_or(-1)),
        },
        Err(err) => DiagnosticCheck {
            name: name.to_string(),
            status: "error".to_string(),
            detail: err.to_string(),
        },
    }
}

fn permission_mode_from_general_settings(
    settings: &GeneralSettings,
) -> yode_core::permission::PermissionMode {
    if settings.full_access {
        yode_core::permission::PermissionMode::Bypass
    } else if settings.default_file_permission {
        yode_core::permission::PermissionMode::AcceptEdits
    } else {
        yode_core::permission::PermissionMode::Default
    }
}

fn start_sleep_guard(sleep_guard: &Arc<Mutex<Option<Child>>>) {
    let Ok(mut guard) = sleep_guard.lock() else {
        return;
    };
    if guard.is_some() {
        return;
    }
    #[cfg(target_os = "macos")]
    let child = Command::new("caffeinate").args(["-dimsu"]).spawn();
    #[cfg(not(target_os = "macos"))]
    let child = Command::new("sh").args(["-c", "sleep 2147483647"]).spawn();
    if let Ok(child) = child {
        *guard = Some(child);
    }
}

fn stop_sleep_guard(sleep_guard: &Arc<Mutex<Option<Child>>>) {
    let Ok(mut guard) = sleep_guard.lock() else {
        return;
    };
    if let Some(mut child) = guard.take() {
        let _ = child.kill();
        let _ = child.wait();
    }
}

fn open_with_destination(destination: &str, path: &Path) -> Result<()> {
    let dest = destination.to_lowercase();
    if dest.contains("cursor") {
        return open_editor(path, "Cursor", "cursor");
    }
    if dest.contains("terminal") {
        return open_terminal_app(path);
    }
    open_editor(path, "Visual Studio Code", "code")
}

fn open_editor(path: &Path, mac_app: &str, cli: &str) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        let status = Command::new("open")
            .args(["-a", mac_app])
            .arg(path)
            .status();
        if status.is_ok_and(|status| status.success()) {
            return Ok(());
        }
    }
    Command::new(cli)
        .arg(path)
        .spawn()
        .with_context(|| format!("无法启动 {}", mac_app))?;
    Ok(())
}

fn open_terminal_app(path: &Path) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .args(["-a", "Terminal"])
            .arg(path)
            .spawn()
            .context("无法启动 Terminal")?;
        Ok(())
    }
    #[cfg(target_os = "windows")]
    {
        Command::new("cmd")
            .args(["/C", "start", "cmd"])
            .current_dir(path)
            .spawn()
            .context("无法启动系统终端")?;
        return Ok(());
    }
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    {
        Command::new("x-terminal-emulator")
            .current_dir(path)
            .spawn()
            .context("无法启动系统终端")?;
        Ok(())
    }
}

fn desktop_mcp_servers_from_config(config: &Config) -> Vec<DesktopMcpServer> {
    let mut servers = config
        .mcp
        .servers
        .iter()
        .map(|(name, server)| DesktopMcpServer {
            name: name.clone(),
            transport: server.transport.label().to_string(),
            command: (!server.command.is_empty()).then(|| server.command.clone()),
            args: server.args.clone(),
            url: server.url.clone(),
            env: server.env.clone(),
            disabled: server.disabled,
        })
        .collect::<Vec<_>>();
    servers.sort_by(|a, b| a.name.cmp(&b.name));
    servers
}

fn desktop_mcp_servers_to_config(
    servers: &[DesktopMcpServer],
) -> Result<HashMap<String, yode_core::config::McpServerConfig>> {
    let mut map = HashMap::new();
    for server in servers {
        map.insert(server.name.clone(), desktop_mcp_server_to_config(server)?);
    }
    Ok(map)
}

fn desktop_mcp_server_to_config(
    server: &DesktopMcpServer,
) -> Result<yode_core::config::McpServerConfig> {
    Ok(yode_core::config::McpServerConfig {
        disabled: server.disabled,
        transport: parse_mcp_transport(&server.transport)?,
        command: server.command.clone().unwrap_or_default(),
        args: server.args.clone(),
        env: server.env.clone(),
        url: server.url.clone().filter(|url| !url.trim().is_empty()),
        auth: None,
    })
}

fn validate_desktop_mcp_servers(servers: &[DesktopMcpServer]) -> Result<()> {
    let mut names = HashSet::new();
    for server in servers {
        let name = server.name.trim();
        if name.is_empty() {
            anyhow::bail!("MCP 服务器名称不能为空。");
        }
        if !name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.'))
        {
            anyhow::bail!("MCP 服务器名称只能包含字母、数字、_、- 或 .。");
        }
        if !names.insert(name.to_string()) {
            anyhow::bail!("MCP 服务器名称 '{}' 已存在。", name);
        }
        let transport = parse_mcp_transport(&server.transport)?;
        match transport {
            yode_core::config::McpTransportConfig::Stdio => {
                if server
                    .command
                    .as_deref()
                    .unwrap_or_default()
                    .trim()
                    .is_empty()
                {
                    anyhow::bail!("stdio MCP 服务器 '{}' 需要执行指令。", name);
                }
            }
            _ => {
                if server.url.as_deref().unwrap_or_default().trim().is_empty() {
                    anyhow::bail!("远程 MCP 服务器 '{}' 需要 URL。", name);
                }
            }
        }
    }
    Ok(())
}

fn parse_mcp_transport(transport: &str) -> Result<yode_core::config::McpTransportConfig> {
    match transport.to_ascii_lowercase().as_str() {
        "stdio" => Ok(yode_core::config::McpTransportConfig::Stdio),
        "sse" => Ok(yode_core::config::McpTransportConfig::Sse),
        "http" => Ok(yode_core::config::McpTransportConfig::Http),
        "websocket" => Ok(yode_core::config::McpTransportConfig::Websocket),
        other => anyhow::bail!("不支持的 MCP transport: {}", other),
    }
}

fn core_mcp_server_to_runtime(
    server_config: &yode_core::config::McpServerConfig,
) -> yode_mcp::McpServerConfig {
    yode_mcp::McpServerConfig {
        disabled: server_config.disabled,
        transport: match server_config.transport {
            yode_core::config::McpTransportConfig::Stdio => yode_mcp::McpTransportConfig::Stdio,
            yode_core::config::McpTransportConfig::Sse => yode_mcp::McpTransportConfig::Sse,
            yode_core::config::McpTransportConfig::Http => yode_mcp::McpTransportConfig::Http,
            yode_core::config::McpTransportConfig::Websocket => {
                yode_mcp::McpTransportConfig::Websocket
            }
        },
        command: server_config.command.clone(),
        args: server_config.args.clone(),
        env: server_config.env.clone(),
        url: server_config.url.clone(),
        auth: server_config
            .auth
            .as_ref()
            .map(|auth| yode_mcp::McpAuthConfig {
                oauth: auth.oauth.as_ref().map(|oauth| yode_mcp::McpOAuthConfig {
                    client_id: oauth.client_id.clone(),
                    authorization_url: oauth.authorization_url.clone(),
                    token_url: oauth.token_url.clone(),
                    scopes: oauth.scopes.clone(),
                }),
                bearer_token_env: auth.bearer_token_env.clone(),
            }),
    }
}

fn mcp_statuses_from_servers(
    servers: &[DesktopMcpServer],
    tested: Option<&DesktopMcpServerStatus>,
) -> Vec<DesktopMcpServerStatus> {
    servers
        .iter()
        .map(|server| {
            if let Some(status) = tested.filter(|status| status.name == server.name) {
                return status.clone();
            }
            if server.disabled {
                DesktopMcpServerStatus {
                    name: server.name.clone(),
                    state: "disabled".to_string(),
                    detail: "服务器已禁用。".to_string(),
                    tool_count: 0,
                    resource_count: 0,
                    template_count: 0,
                }
            } else {
                DesktopMcpServerStatus {
                    name: server.name.clone(),
                    state: "configured".to_string(),
                    detail: "已保存到配置；可测试连接或重载运行时。".to_string(),
                    tool_count: 0,
                    resource_count: 0,
                    template_count: 0,
                }
            }
        })
        .collect()
}

fn personalization_state_from_settings(
    settings: &serde_json::Map<String, serde_json::Value>,
) -> Result<PersonalizationState> {
    Ok(PersonalizationState {
        personality: desktop_string_setting(settings, "yode-personality", "Friendly"),
        custom_instructions: desktop_string_setting(settings, "yode-custom-instructions", ""),
        enable_memories: desktop_bool_setting(settings, "yode-enable-memories", false),
        skip_tool_chats: desktop_bool_setting(settings, "yode-skip-tool-chats", false),
    })
}

fn build_personalization_prompt(state: &PersonalizationState) -> Option<String> {
    let mut lines = Vec::new();
    match state.personality.as_str() {
        "Professional" => lines.push(
            "Tone: professional, rigorous, precise, and calm. Prefer concrete tradeoffs and clear verification notes.",
        ),
        "Concise" => lines.push(
            "Tone: concise and direct. Keep explanations compact while still naming important risks and verification.",
        ),
        _ => lines.push(
            "Tone: friendly, warm, and collaborative. Stay clear and practical without becoming verbose.",
        ),
    }

    let custom = state.custom_instructions.trim();
    if !custom.is_empty() {
        lines.push("Host-level custom instructions from the user:");
        lines.push(custom);
    }

    Some(lines.join("\n"))
}

fn apply_menu_bar_setting(app: &AppHandle, enabled: bool) -> Result<()> {
    if let Some(tray) = app.tray_by_id("main") {
        tray.set_visible(enabled)?;
        return Ok(());
    }
    if !enabled {
        return Ok(());
    }

    #[allow(unused_imports)]
    use tauri::{
        menu::{Menu, MenuItem},
        tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    };

    let show = MenuItem::with_id(app, "show", "显示 Yode", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &quit])?;
    let _tray = TrayIconBuilder::with_id("main")
        .tooltip("Yode")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show" => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if matches!(
                event,
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                }
            ) {
                let app = tray.app_handle();
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
        })
        .build(app)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::CreateSessionRequest;
    use crate::runtime::terminal_runtime::{
        apply_terminal_color_env, parse_terminal_run_stdout, terminal_shell_command,
    };
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_config() -> Config {
        toml::from_str(include_str!("../../../../config/default.toml")).unwrap()
    }

    fn test_runtime(name: &str) -> (DesktopRuntime, PathBuf) {
        let dir = unique_temp_dir(name);
        std::fs::create_dir_all(&dir).unwrap();
        let config = test_config();
        let db_path = dir.join("sessions.db");
        let runtime = DesktopRuntime {
            config: Mutex::new(config),
            db: Database::open(&db_path).unwrap(),
            db_path,
            workspace_path: dir.clone(),
            provider_registry: Mutex::new(Arc::new(ProviderRegistry::new())),
            tool_registry: Mutex::new(Arc::new(ToolRegistry::new())),
            mcp_resource_provider: Mutex::new(None),
            active_session_id: Mutex::new(None),
            permission_mode: Mutex::new("default".to_string()),
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
        };
        (runtime, dir)
    }

    fn unique_temp_dir(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("yode-{name}-{nonce}"))
    }

    #[test]
    fn workspace_root_detection_climbs_out_of_src_tauri() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../..")
            .canonicalize()
            .unwrap();
        let src_tauri = root.join("apps/yode-desktop/src-tauri");

        assert_eq!(
            find_workspace_root(&src_tauri).as_deref(),
            Some(root.as_path())
        );
    }

    #[test]
    fn edit_diff_artifact_read_searches_session_project_roots() {
        let workspace_root = unique_temp_dir("workspace-root");
        let project_root = unique_temp_dir("project-root");
        let artifact_dir = project_root.join(".yode").join("edit-diffs");
        std::fs::create_dir_all(&artifact_dir).unwrap();
        std::fs::write(artifact_dir.join("example.diff"), "+hello\n").unwrap();

        let content = read_edit_diff_artifact_from_roots(
            ".yode/edit-diffs/example.diff",
            &[workspace_root.clone(), project_root.clone()],
        )
        .unwrap();

        assert_eq!(content, "+hello\n");
        let _ = std::fs::remove_dir_all(workspace_root);
        let _ = std::fs::remove_dir_all(project_root);
    }

    #[test]
    fn sessions_clear_messages_removes_current_history() {
        let (runtime, dir) = test_runtime("desktop-clear-session");
        let session = runtime
            .sessions_create(CreateSessionRequest {
                title: Some("clear me".to_string()),
                project_root: None,
                provider: None,
                model: None,
            })
            .unwrap();
        runtime
            .db
            .save_message(&session.id, "user", Some("hello"), None, None, None)
            .unwrap();
        assert_eq!(
            runtime.sessions_messages(session.id.clone()).unwrap().len(),
            1
        );

        runtime.sessions_clear_messages(session.id.clone()).unwrap();

        assert!(runtime.sessions_messages(session.id).unwrap().is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn sessions_rename_updates_session_title() {
        let (runtime, dir) = test_runtime("desktop-rename-session");
        let session = runtime
            .sessions_create(CreateSessionRequest {
                title: Some("old".to_string()),
                project_root: None,
                provider: None,
                model: None,
            })
            .unwrap();

        let renamed = runtime
            .sessions_rename(session.id.clone(), "new title".to_string())
            .unwrap();

        assert_eq!(renamed.title, "new title");
        assert_eq!(
            runtime.db.get_session(&session.id).unwrap().unwrap().name,
            Some("new title".to_string())
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn sessions_export_markdown_writes_transcript() {
        let (runtime, dir) = test_runtime("desktop-export-session");
        let session = runtime
            .sessions_create(CreateSessionRequest {
                title: Some("export me".to_string()),
                project_root: Some(dir.display().to_string()),
                provider: None,
                model: None,
            })
            .unwrap();
        runtime
            .db
            .save_message(&session.id, "user", Some("hello export"), None, None, None)
            .unwrap();
        runtime
            .db
            .save_message(&session.id, "assistant", Some("hi back"), None, None, None)
            .unwrap();

        let exported = runtime.sessions_export_markdown(session.id).unwrap();
        let content = std::fs::read_to_string(&exported.path).unwrap();

        assert_eq!(exported.message_count, 2);
        assert!(content.contains("# export me"));
        assert!(content.contains("hello export"));
        assert!(content.contains("hi back"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn sessions_compact_local_keeps_recent_history() {
        let (runtime, dir) = test_runtime("desktop-compact-session");
        let session = runtime
            .sessions_create(CreateSessionRequest {
                title: Some("compact me".to_string()),
                project_root: None,
                provider: None,
                model: None,
            })
            .unwrap();
        for index in 0..24 {
            let role = if index % 2 == 0 { "user" } else { "assistant" };
            runtime
                .db
                .save_message(
                    &session.id,
                    role,
                    Some(&format!("message {index}")),
                    None,
                    None,
                    None,
                )
                .unwrap();
        }

        let compacted = runtime.sessions_compact_local(session.id.clone()).unwrap();
        let messages = runtime.sessions_messages(session.id).unwrap();

        assert_eq!(compacted.before_count, 24);
        assert_eq!(compacted.after_count, 17);
        assert_eq!(messages.len(), 17);
        assert_eq!(messages[0].role, "system");
        assert!(messages[0]
            .content
            .as_deref()
            .unwrap_or_default()
            .contains("[Context summary]"));
        assert_eq!(
            messages
                .last()
                .and_then(|message| message.content.as_deref()),
            Some("message 23")
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn edit_diff_artifact_read_rejects_parent_components() {
        let project_root = unique_temp_dir("project-root");
        let artifact_dir = project_root.join(".yode").join("edit-diffs");
        std::fs::create_dir_all(&artifact_dir).unwrap();

        let error = read_edit_diff_artifact_from_roots(
            ".yode/edit-diffs/../secret.diff",
            &[project_root.clone()],
        )
        .unwrap_err()
        .to_string();

        assert!(error.contains("unsafe components"));
        let _ = std::fs::remove_dir_all(project_root);
    }

    #[test]
    fn terminal_shell_uses_login_interactive_zsh() {
        let env = HashMap::from([("SHELL".to_string(), "/bin/zsh".to_string())]);
        let (shell, args) = terminal_shell_command(&env);

        assert_eq!(shell, PathBuf::from("/bin/zsh"));
        assert_eq!(args, vec!["-lic"]);
    }

    #[test]
    fn terminal_color_env_uses_truecolor_capabilities() {
        let mut command = portable_pty::CommandBuilder::new("/bin/sh");
        apply_terminal_color_env(&mut command);

        assert_eq!(
            command.get_env("TERM").and_then(|value| value.to_str()),
            Some("xterm-256color")
        );
        assert_eq!(
            command
                .get_env("COLORTERM")
                .and_then(|value| value.to_str()),
            Some("truecolor")
        );
        assert_eq!(
            command.get_env("CLICOLOR").and_then(|value| value.to_str()),
            Some("1")
        );
    }

    #[test]
    fn terminal_stdout_parser_extracts_runtime_state() {
        let marker = "__YODE_TERMINAL_TEST__";
        let stdout = b"hello\n__YODE_TERMINAL_TEST__STATUS:7\n__YODE_TERMINAL_TEST__PWD:/tmp/project\n__YODE_TERMINAL_TEST__ENV:FOO=bar\0PWD=/tmp/project\0";
        let fallback_env = HashMap::from([("FOO".to_string(), "old".to_string())]);

        let (visible, cwd, env, exit_code) = parse_terminal_run_stdout(
            stdout,
            marker,
            std::path::Path::new("/tmp"),
            &fallback_env,
            1,
        );

        assert_eq!(visible, "hello");
        assert_eq!(cwd, PathBuf::from("/tmp/project"));
        assert_eq!(env.get("FOO"), Some(&"bar".to_string()));
        assert_eq!(exit_code, 7);
    }

    #[test]
    fn terminal_stdout_parser_falls_back_without_marker() {
        let fallback_env = HashMap::from([("FOO".to_string(), "old".to_string())]);

        let (visible, cwd, env, exit_code) = parse_terminal_run_stdout(
            b"plain output\n",
            "__YODE_TERMINAL_TEST__",
            std::path::Path::new("/tmp"),
            &fallback_env,
            2,
        );

        assert_eq!(visible, "plain output");
        assert_eq!(cwd, PathBuf::from("/tmp"));
        assert_eq!(env.get("FOO"), Some(&"old".to_string()));
        assert_eq!(exit_code, 2);
    }
}
