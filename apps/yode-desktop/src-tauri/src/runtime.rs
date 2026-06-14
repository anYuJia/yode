use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, Mutex,
};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde_json::json;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};
use uuid::Uuid;

use yode_core::config::Config;
use yode_core::context::AgentContext;
use yode_core::db::{Database, StoredMessage};
use yode_core::engine::{AgentEngine, ConfirmResponse, EngineEvent};
use yode_core::permission::{PermissionManager, PermissionRule, RuleBehavior, RuleSource};
use yode_core::session::Session;
use yode_llm::registry::ProviderRegistry;
use yode_tools::registry::ToolRegistry;
use yode_tools::tool::McpResourceProvider;

use crate::protocol::{
    Bootstrap, ConfigurationState, ConfigurationUpdateRequest, CreateSessionRequest, DefaultLlm,
    DesktopActionResult, DesktopEvent, DesktopImageOutput, DesktopMessage, DesktopProvider,
    DesktopSession, DesktopSettingSetRequest, DesktopSettingValue, DesktopWorktree,
    DiagnosticCheck, GeneralSettings, ImportAiSessionsResult, LicenseNotice, OpenTargetRequest,
    RuntimeState, SendMessageRequest, TerminalExitEvent, TerminalOpenRequest, TerminalOpenResponse,
    TerminalOutputEvent, TerminalResizeRequest, TerminalRunRequest, TerminalRunResponse,
    TerminalWriteRequest, TurnAccepted, WorkspaceDiagnosticsResult,
};

pub struct DesktopRuntime {
    config: Mutex<Config>,
    db: Database,
    db_path: PathBuf,
    workspace_path: PathBuf,
    provider_registry: Mutex<Arc<ProviderRegistry>>,
    tool_registry: Arc<ToolRegistry>,
    mcp_resource_provider: Option<Arc<dyn McpResourceProvider>>,
    active_session_id: Mutex<Option<String>>,
    permission_mode: Mutex<String>,
    seq: AtomicU64,
    confirm_txs: Arc<Mutex<HashMap<(String, String), UnboundedSender<ConfirmResponse>>>>,
    ask_user_txs: Arc<Mutex<HashMap<(String, String), UnboundedSender<String>>>>,
    cancel_tokens: Arc<Mutex<HashMap<(String, String), tokio_util::sync::CancellationToken>>>,
    pending_confirmations: Arc<Mutex<HashMap<(String, String), PendingConfirmation>>>,
    session_permission_rules: Arc<Mutex<HashMap<String, Vec<PermissionRule>>>>,
    terminal_sessions: Mutex<HashMap<String, TerminalSessionState>>,
    pty_sessions: Arc<Mutex<HashMap<String, PtySessionState>>>,
    general_settings: Mutex<GeneralSettings>,
    sleep_guard: Arc<Mutex<Option<Child>>>,
}

#[derive(Debug, Clone)]
struct PendingConfirmation {
    tool_name: String,
    command: Option<String>,
}

#[derive(Debug, Clone)]
struct TerminalSessionState {
    cwd: PathBuf,
    env: HashMap<String, String>,
}

struct PtySessionState {
    master: Box<dyn portable_pty::MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    child: Box<dyn portable_pty::Child + Send + Sync>,
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
            tool_registry,
            mcp_resource_provider,
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

    pub fn sessions_list(&self) -> Result<Vec<DesktopSession>> {
        let active_session_id = self
            .active_session_id
            .lock()
            .map_err(|_| anyhow::anyhow!("active session lock poisoned"))?
            .clone();

        Ok(self
            .db
            .list_sessions(50)?
            .into_iter()
            .map(|session| self.map_session(session, active_session_id.as_deref()))
            .collect())
    }

    pub fn sessions_create(&self, request: CreateSessionRequest) -> Result<DesktopSession> {
        let now = Utc::now();
        let config = self
            .config
            .lock()
            .map_err(|_| anyhow::anyhow!("config lock poisoned"))?;
        let (default_provider, default_model) = self.default_llm_for_new_session(&config)?;
        let session = Session {
            id: Uuid::new_v4().to_string(),
            name: request.title.or_else(|| Some("桌面端会话".to_string())),
            project_root: request.project_root,
            provider: request.provider.unwrap_or(default_provider),
            model: request.model.unwrap_or(default_model),
            created_at: now,
            updated_at: now,
        };

        self.db.create_session(&session)?;
        self.set_active_session(session.id.clone())?;
        Ok(self.map_session(session, None))
    }

    pub fn sessions_messages(&self, session_id: String) -> Result<Vec<DesktopMessage>> {
        Ok(self
            .db
            .load_messages(&session_id)?
            .into_iter()
            .map(|message| DesktopMessage {
                images: stored_images(&message)
                    .into_iter()
                    .map(|image| DesktopImageOutput {
                        base64: image.base64,
                        media_type: image.media_type,
                    })
                    .collect(),
                id: message.id,
                role: message.role,
                content: message.content,
                reasoning: message.reasoning,
                tool_calls_json: message.tool_calls_json,
                tool_call_id: message.tool_call_id,
                metadata: message
                    .metadata_json
                    .as_deref()
                    .and_then(|json| serde_json::from_str(json).ok()),
                created_at: message.created_at.to_rfc3339(),
            })
            .collect())
    }

    pub fn sessions_delete(&self, session_id: String) -> Result<()> {
        self.db.delete_session(&session_id)?;
        Ok(())
    }

    pub fn sessions_update_llm(
        &self,
        session_id: String,
        provider: String,
        model: String,
    ) -> Result<()> {
        self.db.update_session_llm(&session_id, &provider, &model)?;
        Ok(())
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

    pub fn worktrees_list(&self) -> Result<Vec<DesktopWorktree>> {
        list_git_worktrees(&self.workspace_path)
    }

    pub fn worktrees_prune_idle(&self) -> Result<DesktopActionResult> {
        let output = Command::new("git")
            .args(["worktree", "prune", "--verbose"])
            .current_dir(&self.workspace_path)
            .output()
            .context("无法执行 git worktree prune")?;
        Ok(DesktopActionResult {
            ok: output.status.success(),
            message: if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if stdout.is_empty() {
                    "没有需要清理的闲置工作树。".to_string()
                } else {
                    stdout
                }
            } else {
                String::from_utf8_lossy(&output.stderr).trim().to_string()
            },
            path: Some(self.workspace_path.display().to_string()),
        })
    }

    pub fn worktree_delete(&self, path: String) -> Result<DesktopActionResult> {
        let output = Command::new("git")
            .args(["worktree", "remove", "--force", &path])
            .current_dir(&self.workspace_path)
            .output()
            .with_context(|| format!("无法删除工作树 {}", path))?;
        Ok(DesktopActionResult {
            ok: output.status.success(),
            message: if output.status.success() {
                format!("已删除工作树 {}", path)
            } else {
                String::from_utf8_lossy(&output.stderr).trim().to_string()
            },
            path: Some(path),
        })
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

    pub fn terminal_run(&self, request: TerminalRunRequest) -> Result<TerminalRunResponse> {
        let trimmed = request.command.trim();
        if trimmed.is_empty() {
            let cwd = self
                .terminal_session(&request.session_id, request.cwd.as_deref())?
                .cwd
                .display()
                .to_string();
            return Ok(TerminalRunResponse {
                output: String::new(),
                cwd,
                exit_code: 0,
            });
        }

        let mut session = self.terminal_session(&request.session_id, request.cwd.as_deref())?;
        let marker = format!("__YODE_TERMINAL_{}__", Uuid::new_v4().simple());
        let script = format!(
            "{{\n{}\n}}\n__yode_status=$?\nprintf '\\n{}STATUS:%s\\n' \"$__yode_status\"\nprintf '{}PWD:'\npwd\nprintf '{}ENV:'\nenv -0\n",
            trimmed, marker, marker, marker
        );
        let (shell, shell_args) = terminal_shell_command(&session.env);

        let mut command = std::process::Command::new(&shell);
        command.args(shell_args).arg(script);
        let output = command
            .current_dir(&session.cwd)
            .env_clear()
            .envs(&session.env)
            .output()
            .with_context(|| {
                format!(
                    "failed to run terminal command '{}' with shell '{}'",
                    trimmed,
                    shell.display()
                )
            })?;

        let (stdout, cwd, env, exit_code) = parse_terminal_run_stdout(
            &output.stdout,
            &marker,
            &session.cwd,
            &session.env,
            output.status.code().unwrap_or(1),
        );
        session.cwd = cwd;
        session.env = env;
        self.terminal_sessions
            .lock()
            .map_err(|_| anyhow::anyhow!("terminal session lock poisoned"))?
            .insert(request.session_id, session.clone());

        let mut text = stdout;
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !stderr.trim().is_empty() {
            if !text.is_empty() {
                text.push('\n');
            }
            text.push_str(stderr.trim_end());
        }
        if text.is_empty() && exit_code != 0 {
            text.push_str("命令执行失败，无输出。");
        }

        Ok(TerminalRunResponse {
            output: text,
            cwd: session.cwd.display().to_string(),
            exit_code,
        })
    }

    pub fn terminal_close(&self, session_id: String) -> Result<()> {
        self.terminal_sessions
            .lock()
            .map_err(|_| anyhow::anyhow!("terminal session lock poisoned"))?
            .remove(&session_id);
        if let Some(mut session) = self
            .pty_sessions
            .lock()
            .map_err(|_| anyhow::anyhow!("pty session lock poisoned"))?
            .remove(&session_id)
        {
            let _ = session.child.kill();
            let _ = session.child.wait();
        }
        Ok(())
    }

    pub fn terminal_open(
        &self,
        app: AppHandle,
        request: TerminalOpenRequest,
    ) -> Result<TerminalOpenResponse> {
        if self
            .pty_sessions
            .lock()
            .map_err(|_| anyhow::anyhow!("pty session lock poisoned"))?
            .contains_key(&request.session_id)
        {
            return Ok(TerminalOpenResponse {
                session_id: request.session_id,
            });
        }

        let cwd = request
            .cwd
            .as_deref()
            .and_then(valid_terminal_cwd)
            .unwrap_or_else(|| self.workspace_path.clone());
        let env: HashMap<String, String> = std::env::vars().collect();
        let (shell, _shell_args) = terminal_shell_command(&env);
        let pty_system = portable_pty::native_pty_system();
        let pair = pty_system
            .openpty(portable_pty::PtySize {
                rows: request.rows.unwrap_or(24).max(1),
                cols: request.cols.unwrap_or(80).max(1),
                pixel_width: 0,
                pixel_height: 0,
            })
            .context("failed to open pty")?;
        let mut command = portable_pty::CommandBuilder::new(shell);
        command.cwd(cwd);
        for (key, value) in env {
            command.env(key, value);
        }
        apply_terminal_color_env(&mut command);

        let child = pair
            .slave
            .spawn_command(command)
            .context("failed to spawn shell")?;
        drop(pair.slave);

        let mut reader = pair
            .master
            .try_clone_reader()
            .context("failed to clone pty reader")?;
        let writer = pair
            .master
            .take_writer()
            .context("failed to take pty writer")?;
        let session_id = request.session_id.clone();
        let sessions = Arc::clone(&self.pty_sessions);
        let app_for_output = app.clone();
        std::thread::spawn(move || {
            let mut buffer = [0u8; 8192];
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(n) => {
                        let data = String::from_utf8_lossy(&buffer[..n]).to_string();
                        let _ = app_for_output.emit(
                            "terminal-output",
                            TerminalOutputEvent {
                                session_id: session_id.clone(),
                                data,
                            },
                        );
                    }
                    Err(_) => break,
                }
            }

            if let Ok(mut sessions) = sessions.lock() {
                sessions.remove(&session_id);
            }
            let _ = app.emit(
                "terminal-exit",
                TerminalExitEvent {
                    session_id,
                    exit_code: None,
                },
            );
        });

        self.pty_sessions
            .lock()
            .map_err(|_| anyhow::anyhow!("pty session lock poisoned"))?
            .insert(
                request.session_id.clone(),
                PtySessionState {
                    master: pair.master,
                    writer,
                    child,
                },
            );

        Ok(TerminalOpenResponse {
            session_id: request.session_id,
        })
    }

    pub fn terminal_write(&self, request: TerminalWriteRequest) -> Result<()> {
        let mut sessions = self
            .pty_sessions
            .lock()
            .map_err(|_| anyhow::anyhow!("pty session lock poisoned"))?;
        let session = sessions
            .get_mut(&request.session_id)
            .ok_or_else(|| anyhow::anyhow!("terminal session not found"))?;
        session.writer.write_all(request.data.as_bytes())?;
        session.writer.flush()?;
        Ok(())
    }

    pub fn terminal_resize(&self, request: TerminalResizeRequest) -> Result<()> {
        let sessions = self
            .pty_sessions
            .lock()
            .map_err(|_| anyhow::anyhow!("pty session lock poisoned"))?;
        let session = sessions
            .get(&request.session_id)
            .ok_or_else(|| anyhow::anyhow!("terminal session not found"))?;
        session.master.resize(portable_pty::PtySize {
            rows: request.rows.max(1),
            cols: request.cols.max(1),
            pixel_width: 0,
            pixel_height: 0,
        })?;
        Ok(())
    }

    fn terminal_session(
        &self,
        session_id: &str,
        initial_cwd: Option<&str>,
    ) -> Result<TerminalSessionState> {
        let mut sessions = self
            .terminal_sessions
            .lock()
            .map_err(|_| anyhow::anyhow!("terminal session lock poisoned"))?;
        Ok(sessions
            .entry(session_id.to_string())
            .or_insert_with(|| TerminalSessionState {
                cwd: initial_cwd
                    .and_then(valid_terminal_cwd)
                    .unwrap_or_else(|| self.workspace_path.clone()),
                env: std::env::vars().collect(),
            })
            .clone())
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
        context.project_memory_enabled = session
            .project_root
            .as_deref()
            .is_some_and(|root| !root.trim().is_empty());
        context.output_style = config.ui.output_style.clone();

        let stored_msgs = self.db.load_messages(&session.id)?;
        let restored_messages: Vec<yode_llm::types::Message> = stored_msgs
            .into_iter()
            .filter_map(stored_message_to_message)
            .collect();

        let tools = self.tool_registry.clone();
        let mcp_resource_provider = self.mcp_resource_provider.clone();
        let config = config.clone();
        let db_path_clone = self.db_path.clone();

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
                    let (kind, payload) = match event {
                        EngineEvent::Thinking => {
                            ("turn_started", json!({ "title": "思考中...", "body": "" }))
                        }
                        EngineEvent::UsageUpdate(usage) => (
                            "usage_update",
                            json!({
                                "title": "用量更新",
                                "body": format!("输入 {}，输出 {}", usage.prompt_tokens, usage.completion_tokens),
                                "inputTokens": usage.prompt_tokens,
                                "outputTokens": usage.completion_tokens,
                                "status": "running"
                            }),
                        ),
                        EngineEvent::TextDelta(text) => {
                            ("assistant_text_delta", json!({ "body": text }))
                        }
                        EngineEvent::ActionNarrative(text) => (
                            "action_narrative",
                            json!({ "body": text, "status": "success" }),
                        ),
                        EngineEvent::TextComplete(text) => (
                            "assistant_text_complete",
                            json!({ "body": text, "status": "completed" }),
                        ),
                        EngineEvent::ReasoningDelta(reasoning) => {
                            ("assistant_reasoning_delta", json!({ "reasoning": reasoning }))
                        }
                        EngineEvent::ReasoningComplete(reasoning) => (
                            "assistant_reasoning_complete",
                            json!({ "reasoning": reasoning, "status": "completed" }),
                        ),
                        EngineEvent::ToolCallStart {
                            id,
                            name,
                            arguments,
                        } => (
                            "tool_started",
                            json!({
                                "id": id,
                                "tool": name,
                                "title": format!("调用工具: {}", name),
                                "body": arguments,
                                "status": "running"
                            }),
                        ),
                        EngineEvent::ToolConfirmRequired {
                            id,
                            name,
                            arguments,
                        } => {
                            if let Ok(mut pending) = pending_confirmations_clone.lock() {
                                pending.insert(
                                    (session_id_str.clone(), turn_id_str.clone()),
                                    PendingConfirmation {
                                        tool_name: name.clone(),
                                        command: extract_command_for_permission(&name, &arguments),
                                    },
                                );
                            }
                            (
                                "tool_confirm_required",
                                json!({
                                    "id": id,
                                    "tool": name,
                                    "title": format!("请求执行工具: {}", name),
                                    "body": arguments,
                                    "meta": "危险操作需要授权"
                                }),
                            )
                        }
                        EngineEvent::ToolProgress { id, name, progress } => (
                            "tool_progress",
                            json!({
                                "id": id,
                                "tool": name,
                                "title": format!("工具进度: {}", name),
                                "body": progress.message,
                                "percent": progress.percent,
                                "status": "running"
                            }),
                        ),
                        EngineEvent::ToolResult { id, name, result } => {
                            let (status, body) = if result.is_error {
                                ("blocked", result.content.clone())
                            } else {
                                ("success", result.content.clone())
                            };
                            (
                                "tool_result",
                                json!({
                                    "id": id,
                                    "tool": name,
                                    "title": format!("工具返回: {}", name),
                                    "body": body,
                                    "status": status,
                                    "errorType": result.error_type.map(|kind| format!("{:?}", kind)),
                                    "recoverable": result.recoverable,
                                    "suggestion": result.suggestion,
                                    "metadata": result.metadata
                                }),
                            )
                        }
                        EngineEvent::TurnComplete(response) => (
                            "turn_completed",
                            json!({
                                "status": "completed",
                                "body": response.message.content.unwrap_or_default(),
                                "reasoning": response.message.reasoning.unwrap_or_default(),
                                "hasToolCalls": !response.message.tool_calls.is_empty(),
                                "toolCallCount": response.message.tool_calls.len(),
                                "model": response.model,
                                "stopReason": response.stop_reason.map(|reason| format!("{:?}", reason)),
                                "inputTokens": response.usage.prompt_tokens,
                                "outputTokens": response.usage.completion_tokens,
                                "totalTokens": response.usage.total_tokens,
                                "contextPercent": 0
                            }),
                        ),
                        EngineEvent::Error(err_msg) => ("error", json!({ "body": err_msg })),
                        EngineEvent::Retrying {
                            error_message,
                            attempt,
                            max_attempts,
                            delay_secs,
                        } => (
                            "retrying",
                            json!({
                                "title": "正在重试",
                                "body": error_message,
                                "attempt": attempt,
                                "maxAttempts": max_attempts,
                                "delaySecs": delay_secs,
                                "status": "running"
                            }),
                        ),
                        EngineEvent::AskUser { id, question } => (
                            "ask_user",
                            json!({
                                "id": id,
                                "title": "需要用户输入",
                                "body": question,
                                "tool": "ask_user",
                                "meta": "等待用户回答"
                            }),
                        ),
                        EngineEvent::Done => (
                            "done",
                            json!({
                                "title": "完成",
                                "body": "本轮已完成。",
                                "status": "completed"
                            }),
                        ),
                        EngineEvent::SubAgentStart { description } => (
                            "subagent_started",
                            json!({
                                "title": "子代理启动",
                                "body": description,
                                "tool": "agent",
                                "status": "running"
                            }),
                        ),
                        EngineEvent::SubAgentComplete { result } => (
                            "subagent_completed",
                            json!({
                                "title": "子代理完成",
                                "body": result,
                                "tool": "agent",
                                "status": "success"
                            }),
                        ),
                        EngineEvent::PlanModeEntered => (
                            "plan_mode_entered",
                            json!({ "title": "计划模式", "body": "已进入计划模式。" }),
                        ),
                        EngineEvent::PlanApprovalRequired { plan_content } => (
                            "plan_approval_required",
                            json!({
                                "title": "计划需要确认",
                                "body": plan_content,
                                "tool": "plan",
                                "meta": "等待确认"
                            }),
                        ),
                        EngineEvent::PlanModeExited => (
                            "plan_mode_exited",
                            json!({ "title": "计划模式", "body": "已退出计划模式。" }),
                        ),
                        EngineEvent::ContextCompactionStarted { mode } => (
                            "context_compaction_started",
                            json!({
                                "title": "上下文压缩开始",
                                "body": mode,
                                "status": "running"
                            }),
                        ),
                        EngineEvent::ContextCompressed {
                            mode,
                            removed,
                            tool_results_truncated,
                            summary,
                            session_memory_path,
                            transcript_path,
                        } => (
                            "context_compressed",
                            json!({
                                "title": "上下文已压缩",
                                "body": summary.unwrap_or_else(|| format!("模式 {}，移除 {} 条，截断 {} 个工具结果。", mode, removed, tool_results_truncated)),
                                "mode": mode,
                                "removed": removed,
                                "toolResultsTruncated": tool_results_truncated,
                                "sessionMemoryPath": session_memory_path,
                                "transcriptPath": transcript_path
                            }),
                        ),
                        EngineEvent::CostUpdate {
                            estimated_cost,
                            input_tokens,
                            output_tokens,
                            cache_write_tokens,
                            cache_read_tokens,
                        } => (
                            "cost_update",
                            json!({
                                "title": "成本更新",
                                "body": format!("${:.4}，输入 {}，输出 {}", estimated_cost, input_tokens, output_tokens),
                                "estimatedCost": estimated_cost,
                                "inputTokens": input_tokens,
                                "outputTokens": output_tokens,
                                "cacheWriteTokens": cache_write_tokens,
                                "cacheReadTokens": cache_read_tokens
                            }),
                        ),
                        EngineEvent::BudgetExceeded { cost, limit } => (
                            "budget_exceeded",
                            json!({
                                "title": "预算已超出",
                                "body": format!("当前成本 ${:.4}，限制 ${:.4}", cost, limit),
                                "status": "blocked"
                            }),
                        ),
                        EngineEvent::SuggestionReady { suggestion } => (
                            "suggestion_ready",
                            json!({ "title": "建议", "body": suggestion }),
                        ),
                        EngineEvent::SessionMemoryUpdated {
                            path,
                            generated_summary,
                        } => (
                            "session_memory_updated",
                            json!({
                                "title": "会话记忆已更新",
                                "body": path,
                                "generatedSummary": generated_summary
                            }),
                        ),
                        EngineEvent::UpdateAvailable(version) => (
                            "update_available",
                            json!({ "title": "发现更新", "body": version }),
                        ),
                        EngineEvent::UpdateDownloading => (
                            "update_downloading",
                            json!({ "title": "正在下载更新", "body": "" }),
                        ),
                        EngineEvent::UpdateDownloaded(version) => (
                            "update_downloaded",
                            json!({ "title": "更新已下载", "body": version }),
                        ),
                    };

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

    fn set_active_session(&self, session_id: String) -> Result<()> {
        *self
            .active_session_id
            .lock()
            .map_err(|_| anyhow::anyhow!("active session lock poisoned"))? = Some(session_id);
        Ok(())
    }

    fn map_session(&self, session: Session, active_session_id: Option<&str>) -> DesktopSession {
        DesktopSession {
            id: session.id.clone(),
            title: session
                .name
                .clone()
                .unwrap_or_else(|| session.id.chars().take(8).collect()),
            project: session
                .project_root
                .as_deref()
                .and_then(project_label_from_root),
            project_root: session.project_root.clone(),
            provider: session.provider,
            model: session.model,
            updated_at: relative_time(session.updated_at),
            active: active_session_id == Some(session.id.as_str()),
        }
    }

    fn default_llm_for_new_session(&self, config: &Config) -> Result<(String, String)> {
        if let Some(session) = self.db.list_sessions(1)?.into_iter().next() {
            if !session.provider.trim().is_empty() && !session.model.trim().is_empty() {
                return Ok((session.provider, session.model));
            }
        }
        Ok((
            config.llm.default_provider.clone(),
            config.llm.default_model.clone(),
        ))
    }

    pub fn config_get_providers(&self) -> Result<Vec<DesktopProvider>> {
        let config = self
            .config
            .lock()
            .map_err(|_| anyhow::anyhow!("config lock poisoned"))?;
        let mut providers = Vec::new();
        for (id, p) in &config.llm.providers {
            let name = match id.as_str() {
                "anthropic" => "Anthropic Claude".to_string(),
                "openai" => "OpenAI".to_string(),
                "google" | "gemini" => "Google Gemini".to_string(),
                "deepseek" => "DeepSeek (深度求索)".to_string(),
                "ollama" => "Ollama (本地运行)".to_string(),
                _ => id.to_uppercase(),
            };
            providers.push(DesktopProvider {
                id: id.clone(),
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
        if !config.llm.providers.contains_key(&provider) {
            anyhow::bail!("Provider '{}' not found", provider);
        }
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
            new_providers.insert(
                p.id,
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
        if !new_providers.contains_key(&config.llm.default_provider) {
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

fn bootstrap_providers(config: &Config) -> Arc<ProviderRegistry> {
    let registry = ProviderRegistry::new();
    for (name, p_config) in &config.llm.providers {
        let env_prefix = name.to_uppercase().replace('-', "_");
        let override_key = format!("{}_API_KEY", env_prefix);
        let api_key = if let Ok(key) = std::env::var(&override_key) {
            key
        } else if let Some(key) = p_config.api_key.clone() {
            key
        } else {
            let fallback_keys = match name.as_str() {
                "anthropic" => vec!["ANTHROPIC_API_KEY", "ANTHROPIC_AUTH_TOKEN"],
                "openai" => vec!["OPENAI_API_KEY"],
                "gemini" | "google" => vec!["GOOGLE_API_KEY", "GEMINI_API_KEY"],
                "deepseek" => vec!["DEEPSEEK_API_KEY"],
                _ => vec![],
            };
            let mut resolved = String::new();
            for key in fallback_keys {
                if let Ok(val) = std::env::var(key) {
                    resolved = val;
                    break;
                }
            }
            resolved
        };

        let override_base = format!("{}_BASE_URL", env_prefix);
        let base_url = if let Ok(url) = std::env::var(&override_base) {
            url
        } else if let Some(url) = p_config.base_url.clone() {
            url
        } else {
            match p_config.format.as_str() {
                "anthropic" => "https://api.anthropic.com".to_string(),
                "gemini" => "https://generativelanguage.googleapis.com/v1beta".to_string(),
                _ => "https://api.openai.com/v1".to_string(),
            }
        };

        match p_config.format.as_str() {
            "anthropic" => {
                registry.register(Arc::new(
                    yode_llm::providers::anthropic::AnthropicProvider::new(
                        name, &api_key, &base_url,
                    ),
                ));
            }
            "gemini" => {
                let mut provider = yode_llm::providers::gemini::GeminiProvider::new(&api_key);
                if base_url != "https://generativelanguage.googleapis.com/v1beta" {
                    provider = provider.with_base_url(&base_url);
                }
                registry.register(Arc::new(provider));
            }
            _ => {
                registry.register(Arc::new(yode_llm::providers::openai::OpenAiProvider::new(
                    name, &api_key, &base_url,
                )));
            }
        }
    }
    Arc::new(registry)
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
        let mcp_config = yode_mcp::McpServerConfig {
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
        };

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

fn extract_command_for_permission(tool_name: &str, arguments: &str) -> Option<String> {
    if tool_name != "bash" {
        return None;
    }

    serde_json::from_str::<serde_json::Value>(arguments)
        .ok()
        .and_then(|value| {
            value
                .get("command")
                .and_then(|command| command.as_str())
                .map(str::to_string)
        })
        .or_else(|| Some(arguments.to_string()))
}

fn project_label_from_root(project_root: &str) -> Option<String> {
    let trimmed = project_root.trim();
    if trimmed.is_empty() {
        return None;
    }

    PathBuf::from(trimmed)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.trim().is_empty())
        .map(str::to_string)
}

fn title_from_content(content: &str) -> String {
    let title = content
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(28)
        .collect::<String>();

    if title.is_empty() {
        "新对话".to_string()
    } else {
        title
    }
}

fn title_from_content_or_images(content: &str, image_count: usize) -> String {
    if !content.trim().is_empty() {
        return title_from_content(content);
    }
    if image_count > 1 {
        format!("{} 张图片", image_count)
    } else {
        "图片".to_string()
    }
}

fn stored_message_to_message(message: StoredMessage) -> Option<yode_llm::types::Message> {
    let role = match message.role.as_str() {
        "user" => yode_llm::types::Role::User,
        "assistant" => yode_llm::types::Role::Assistant,
        "tool" => yode_llm::types::Role::Tool,
        "system" => yode_llm::types::Role::System,
        _ => return None,
    };
    let tool_calls: Vec<yode_llm::types::ToolCall> = message
        .tool_calls_json
        .as_deref()
        .and_then(|json| serde_json::from_str(json).ok())
        .unwrap_or_default();
    let mut blocks = Vec::new();
    if let Some(reasoning) = &message.reasoning {
        blocks.push(yode_llm::types::ContentBlock::Thinking {
            thinking: reasoning.clone(),
            signature: None,
        });
    }
    if let Some(content) = &message.content {
        blocks.push(yode_llm::types::ContentBlock::Text {
            text: content.clone(),
        });
    }

    let images = stored_images(&message);

    Some(
        yode_llm::types::Message {
            role,
            content: message.content,
            content_blocks: blocks,
            reasoning: message.reasoning,
            tool_calls,
            tool_call_id: message.tool_call_id,
            images,
        }
        .normalized(),
    )
}

fn stored_images(message: &StoredMessage) -> Vec<yode_llm::types::ImageData> {
    message
        .images_json
        .as_deref()
        .and_then(|json| serde_json::from_str(json).ok())
        .unwrap_or_default()
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

fn valid_terminal_cwd(raw: &str) -> Option<PathBuf> {
    let path = PathBuf::from(raw);
    if path.is_dir() {
        Some(path)
    } else {
        None
    }
}

fn terminal_shell_command(env: &HashMap<String, String>) -> (PathBuf, Vec<&'static str>) {
    let shell = env
        .get("SHELL")
        .filter(|shell| !shell.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/bin/sh"));
    let shell_name = shell
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();

    if shell_name.contains("zsh") || shell_name.contains("bash") {
        (shell, vec!["-lic"])
    } else {
        (PathBuf::from("/bin/sh"), vec!["-lc"])
    }
}

fn apply_terminal_color_env(command: &mut portable_pty::CommandBuilder) {
    command.env("TERM", "xterm-256color");
    command.env("COLORTERM", "truecolor");
    command.env("CLICOLOR", "1");
    command.env("FORCE_COLOR", "1");
    command.env("GREP_COLORS", "mt=01;35:fn=36:ln=32:se=2");
}

fn parse_terminal_run_stdout(
    stdout: &[u8],
    marker: &str,
    fallback_cwd: &std::path::Path,
    fallback_env: &HashMap<String, String>,
    fallback_exit_code: i32,
) -> (String, PathBuf, HashMap<String, String>, i32) {
    let status_marker = format!("\n{}STATUS:", marker).into_bytes();
    let Some(status_start) = find_bytes(stdout, &status_marker) else {
        return (
            String::from_utf8_lossy(stdout).trim_end().to_string(),
            fallback_cwd.to_path_buf(),
            fallback_env.clone(),
            fallback_exit_code,
        );
    };

    let visible_stdout = String::from_utf8_lossy(&stdout[..status_start])
        .trim_end_matches('\n')
        .to_string();
    let status_value_start = status_start + status_marker.len();
    let status_end = stdout[status_value_start..]
        .iter()
        .position(|byte| *byte == b'\n')
        .map(|offset| status_value_start + offset)
        .unwrap_or(stdout.len());
    let exit_code = String::from_utf8_lossy(&stdout[status_value_start..status_end])
        .trim()
        .parse::<i32>()
        .unwrap_or(fallback_exit_code);

    let pwd_marker = format!("{}PWD:", marker).into_bytes();
    let env_marker = format!("{}ENV:", marker).into_bytes();
    let pwd_start =
        find_bytes_from(stdout, &pwd_marker, status_end).map(|idx| idx + pwd_marker.len());
    let env_start = find_bytes_from(stdout, &env_marker, status_end);

    let cwd = pwd_start
        .and_then(|start| {
            let end = stdout[start..]
                .iter()
                .position(|byte| *byte == b'\n')
                .map(|offset| start + offset)
                .unwrap_or(stdout.len());
            let path = String::from_utf8_lossy(&stdout[start..end])
                .trim()
                .to_string();
            if path.is_empty() {
                None
            } else {
                Some(PathBuf::from(path))
            }
        })
        .unwrap_or_else(|| fallback_cwd.to_path_buf());

    let env = env_start
        .map(|start| parse_null_delimited_env(&stdout[start + env_marker.len()..]))
        .filter(|env| !env.is_empty())
        .unwrap_or_else(|| fallback_env.clone());

    (visible_stdout, cwd, env, exit_code)
}

fn parse_null_delimited_env(bytes: &[u8]) -> HashMap<String, String> {
    bytes
        .split(|byte| *byte == 0)
        .filter_map(|entry| {
            if entry.is_empty() {
                return None;
            }
            let eq = entry.iter().position(|byte| *byte == b'=')?;
            let key = String::from_utf8_lossy(&entry[..eq]).to_string();
            let value = String::from_utf8_lossy(&entry[eq + 1..]).to_string();
            Some((key, value))
        })
        .collect()
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    find_bytes_from(haystack, needle, 0)
}

fn find_bytes_from(haystack: &[u8], needle: &[u8], start: usize) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() || start >= haystack.len() {
        return None;
    }
    haystack[start..]
        .windows(needle.len())
        .position(|window| window == needle)
        .map(|offset| start + offset)
}

fn relative_time(updated_at: DateTime<Utc>) -> String {
    let local_time = updated_at.with_timezone(&chrono::Local);
    local_time.format("%m月%d日 %H:%M").to_string()
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

fn desktop_settings_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".yode")
        .join("desktop-settings.json")
}

fn read_desktop_settings() -> Result<serde_json::Map<String, serde_json::Value>> {
    let path = desktop_settings_path();
    if !path.exists() {
        return Ok(serde_json::Map::new());
    }
    let raw = std::fs::read_to_string(path)?;
    Ok(serde_json::from_str::<serde_json::Value>(&raw)
        .ok()
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default())
}

fn write_desktop_settings(settings: &serde_json::Map<String, serde_json::Value>) -> Result<()> {
    let path = desktop_settings_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_string_pretty(settings)?)?;
    Ok(())
}

fn list_git_worktrees(workspace_path: &Path) -> Result<Vec<DesktopWorktree>> {
    let output = Command::new("git")
        .args(["worktree", "list", "--porcelain"])
        .current_dir(workspace_path)
        .output()
        .context("无法读取 git worktree 列表")?;
    if !output.status.success() {
        return Ok(Vec::new());
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let mut result = Vec::new();
    let mut current_path: Option<String> = None;
    let mut current_branch: Option<String> = None;
    let mut detached = false;
    for line in text.lines().chain(std::iter::once("")) {
        if let Some(path) = line.strip_prefix("worktree ") {
            if let Some(previous_path) = current_path.take() {
                result.push(worktree_record(
                    previous_path,
                    current_branch.take(),
                    detached,
                    workspace_path,
                ));
                detached = false;
            }
            current_path = Some(path.to_string());
        } else if let Some(branch) = line.strip_prefix("branch ") {
            current_branch = Some(branch.trim_start_matches("refs/heads/").to_string());
        } else if line == "detached" {
            detached = true;
        } else if line.is_empty() {
            if let Some(previous_path) = current_path.take() {
                result.push(worktree_record(
                    previous_path,
                    current_branch.take(),
                    detached,
                    workspace_path,
                ));
                detached = false;
            }
        }
    }
    Ok(result)
}

fn worktree_record(
    path: String,
    branch: Option<String>,
    detached: bool,
    workspace_path: &Path,
) -> DesktopWorktree {
    let status = if Path::new(&path) == workspace_path {
        "Active"
    } else {
        "Idle"
    };
    DesktopWorktree {
        id: path.clone(),
        branch: branch.unwrap_or_else(|| {
            if detached {
                "detached".to_string()
            } else {
                "unknown".to_string()
            }
        }),
        size: human_size(directory_size(Path::new(&path)).unwrap_or(0)),
        path,
        status: status.to_string(),
    }
}

fn directory_size(path: &Path) -> Result<u64> {
    let mut total = 0u64;
    let mut stack = vec![path.to_path_buf()];
    while let Some(path) = stack.pop() {
        let Ok(metadata) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        if metadata.is_file() {
            total = total.saturating_add(metadata.len());
        } else if metadata.is_dir() {
            let Ok(entries) = std::fs::read_dir(&path) else {
                continue;
            };
            for entry in entries.flatten() {
                stack.push(entry.path());
            }
        }
    }
    Ok(total)
}

fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut value = bytes as f64;
    let mut unit = 0usize;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{} {}", bytes, UNITS[unit])
    } else {
        format!("{:.1} {}", value, UNITS[unit])
    }
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
        return Ok(());
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

fn collect_import_files(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut stack = paths;
    while let Some(path) = stack.pop() {
        if path.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&path) {
                for entry in entries.flatten() {
                    stack.push(entry.path());
                }
            }
            continue;
        }
        let Some(ext) = path.extension().and_then(|value| value.to_str()) else {
            continue;
        };
        if matches!(
            ext.to_lowercase().as_str(),
            "json" | "jsonl" | "md" | "markdown" | "txt"
        ) {
            files.push(path);
        }
    }
    files
}

fn import_one_ai_session(
    db: &Database,
    path: &Path,
    provider: &str,
    model: &str,
) -> Result<Option<Session>> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("无法读取导入文件 {}", path.display()))?;
    let messages = parse_import_messages(&text, path);
    if messages.is_empty() {
        return Ok(None);
    }

    let now = Utc::now();
    let title = import_title(path, &messages);
    let session = Session {
        id: Uuid::new_v4().to_string(),
        name: Some(title),
        project_root: None,
        provider: provider.to_string(),
        model: model.to_string(),
        created_at: now,
        updated_at: now,
    };
    db.create_session(&session)?;
    for (role, content) in messages {
        db.save_message(&session.id, &role, Some(&content), None, None, None)?;
    }
    db.touch_session(&session.id)?;
    Ok(Some(session))
}

fn parse_import_messages(text: &str, path: &Path) -> Vec<(String, String)> {
    let ext = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_lowercase();
    if ext == "jsonl" {
        let mut messages = Vec::new();
        for line in text.lines().filter(|line| !line.trim().is_empty()) {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(line) {
                append_messages_from_json(&value, &mut messages);
            }
        }
        return messages;
    }
    if ext == "json" {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(text) {
            let mut messages = Vec::new();
            append_messages_from_json(&value, &mut messages);
            if !messages.is_empty() {
                return messages;
            }
        }
    }
    vec![("user".to_string(), text.trim().to_string())]
        .into_iter()
        .filter(|(_, content)| !content.is_empty())
        .collect()
}

fn append_messages_from_json(value: &serde_json::Value, out: &mut Vec<(String, String)>) {
    if let Some(array) = value.as_array() {
        for item in array {
            append_messages_from_json(item, out);
        }
        return;
    }
    if let Some(messages) = value.get("messages").and_then(|value| value.as_array()) {
        for message in messages {
            append_messages_from_json(message, out);
        }
        return;
    }
    if let Some(mapping) = value.as_object() {
        let role = mapping
            .get("role")
            .or_else(|| mapping.get("author"))
            .or_else(|| mapping.get("sender"))
            .and_then(|value| value.as_str())
            .unwrap_or("user")
            .to_lowercase();
        let normalized_role = if role.contains("assistant") || role.contains("bot") {
            "assistant"
        } else if role.contains("system") {
            "system"
        } else {
            "user"
        };
        let content = mapping
            .get("content")
            .or_else(|| mapping.get("text"))
            .or_else(|| mapping.get("message"))
            .and_then(extract_json_text)
            .unwrap_or_default();
        if !content.trim().is_empty() {
            out.push((normalized_role.to_string(), content));
        }
    }
}

fn extract_json_text(value: &serde_json::Value) -> Option<String> {
    if let Some(text) = value.as_str() {
        return Some(text.to_string());
    }
    if let Some(array) = value.as_array() {
        let parts: Vec<String> = array.iter().filter_map(extract_json_text).collect();
        return Some(parts.join("\n"));
    }
    if let Some(object) = value.as_object() {
        if let Some(text) = object.get("text").and_then(|value| value.as_str()) {
            return Some(text.to_string());
        }
        if let Some(parts) = object.get("parts").and_then(|value| value.as_array()) {
            let parts: Vec<String> = parts.iter().filter_map(extract_json_text).collect();
            return Some(parts.join("\n"));
        }
    }
    None
}

fn import_title(path: &Path, messages: &[(String, String)]) -> String {
    let fallback = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("导入会话");
    let first = messages
        .iter()
        .find(|(role, _)| role == "user")
        .map(|(_, content)| content.trim())
        .filter(|content| !content.is_empty())
        .unwrap_or(fallback);
    let mut title = first.chars().take(36).collect::<String>();
    if first.chars().count() > 36 {
        title.push('…');
    }
    format!("导入：{}", title)
}

fn read_license_notices(workspace_path: &Path) -> Vec<LicenseNotice> {
    let root = find_workspace_root(workspace_path).unwrap_or_else(|| workspace_path.to_path_buf());
    let mut notices = vec![LicenseNotice {
        name: "yode".to_string(),
        version: Some(env!("CARGO_PKG_VERSION").to_string()),
        license: Some("MIT".to_string()),
        source: "workspace".to_string(),
    }];

    let cargo_lock = root.join("Cargo.lock");
    if let Ok(lock) = std::fs::read_to_string(&cargo_lock) {
        notices.extend(parse_cargo_lock_notices(&lock));
    }
    let package_lock = root.join("apps/yode-desktop/pnpm-lock.yaml");
    if let Ok(lock) = std::fs::read_to_string(&package_lock) {
        notices.extend(parse_pnpm_lock_notices(&lock));
    }
    notices.sort_by(|a, b| a.name.cmp(&b.name).then(a.version.cmp(&b.version)));
    notices.dedup_by(|a, b| a.name == b.name && a.version == b.version && a.source == b.source);
    notices
}

fn parse_cargo_lock_notices(lock: &str) -> Vec<LicenseNotice> {
    let mut notices = Vec::new();
    let mut name: Option<String> = None;
    let mut version: Option<String> = None;
    for line in lock.lines() {
        let trimmed = line.trim();
        if trimmed == "[[package]]" {
            if let Some(package_name) = name.take() {
                notices.push(LicenseNotice {
                    name: package_name,
                    version: version.take(),
                    license: None,
                    source: "Cargo.lock".to_string(),
                });
            }
        } else if let Some(value) = trimmed.strip_prefix("name = ") {
            name = Some(value.trim_matches('"').to_string());
        } else if let Some(value) = trimmed.strip_prefix("version = ") {
            version = Some(value.trim_matches('"').to_string());
        }
    }
    if let Some(package_name) = name.take() {
        notices.push(LicenseNotice {
            name: package_name,
            version,
            license: None,
            source: "Cargo.lock".to_string(),
        });
    }
    notices
}

fn parse_pnpm_lock_notices(lock: &str) -> Vec<LicenseNotice> {
    lock.lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if !trimmed.starts_with('/') || !trimmed.ends_with(':') {
                return None;
            }
            let package = trimmed.trim_start_matches('/').trim_end_matches(':');
            let (name, version) = package.rsplit_once('@')?;
            if name.is_empty() || version.is_empty() {
                return None;
            }
            Some(LicenseNotice {
                name: name.to_string(),
                version: Some(version.to_string()),
                license: None,
                source: "pnpm-lock.yaml".to_string(),
            })
        })
        .collect()
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
