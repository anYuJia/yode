use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::process::Child;
use std::sync::{atomic::AtomicU64, Arc, Mutex};

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
use crate::desktop_settings_store::read_desktop_settings;
use crate::git_settings::{apply_git_settings_env, git_settings_from_desktop_settings};
use crate::license_notices::read_license_notices;
use crate::protocol::{
    Bootstrap, DesktopActionResult, DesktopWorktree, GeneralSettings, ImportAiSessionsResult,
    LicenseNotice, RuntimeState,
};
use crate::session_import::{collect_import_files, import_one_ai_session};
use crate::worktree::{
    current_git_branch, delete_worktree, list_git_worktrees, prune_idle_worktrees,
};

mod configuration_runtime;
mod mcp_runtime;
mod personalization_runtime;
mod provider_runtime;
mod session_runtime;
mod settings_runtime;
mod terminal_runtime;
mod turn_runtime;

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

    pub async fn edit_diff_artifact_read(&self, path: String) -> Result<String> {
        read_edit_diff_artifact_from_roots(&path, &self.edit_diff_artifact_roots()?).await
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

    pub async fn import_ai_sessions(&self) -> Result<ImportAiSessionsResult> {
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

        let (provider, model) = {
            let config = self
                .config
                .lock()
                .map_err(|_| anyhow::anyhow!("config lock poisoned"))?;
            self.default_llm_for_new_session(&config)?
        };

        let mut imported_sessions = Vec::new();
        let mut skipped = 0usize;
        for file in collect_import_files(paths).await {
            match import_one_ai_session(&self.db, &file, &provider, &model).await {
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

    fn user_config_path(&self) -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| self.workspace_path.clone())
            .join(".yode")
            .join("config.toml")
    }

    fn project_config_path(&self) -> PathBuf {
        self.workspace_path.join(".yode").join("config.toml")
    }
}

async fn read_edit_diff_artifact_from_roots(path: &str, roots: &[PathBuf]) -> Result<String> {
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
        if let Ok(mut entries) = tokio::fs::read_dir(root).await {
            while let Some(entry) = entries.next_entry().await? {
                let path = entry.path();
                if entry.file_type().await?.is_dir() {
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
        let canonical_target = match tokio::fs::canonicalize(&target).await {
            Ok(path) => path,
            Err(err) => {
                last_error = Some(
                    anyhow::anyhow!(err).context(format!("Failed to access {}", target.display())),
                );
                continue;
            }
        };
        let canonical_allowed = match tokio::fs::canonicalize(&allowed_dir).await {
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

        let metadata = tokio::fs::metadata(&canonical_target)
            .await
            .with_context(|| format!("Failed to inspect {}", canonical_target.display()))?;
        if metadata.len() > 2 * 1024 * 1024 {
            anyhow::bail!("diff artifact is too large to display");
        }

        return tokio::fs::read_to_string(&canonical_target)
            .await
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

    #[tokio::test]
    async fn edit_diff_artifact_read_searches_session_project_roots() {
        let workspace_root = unique_temp_dir("workspace-root");
        let project_root = unique_temp_dir("project-root");
        let artifact_dir = project_root.join(".yode").join("edit-diffs");
        std::fs::create_dir_all(&artifact_dir).unwrap();
        std::fs::write(artifact_dir.join("example.diff"), "+hello\n").unwrap();

        let content = read_edit_diff_artifact_from_roots(
            ".yode/edit-diffs/example.diff",
            &[workspace_root.clone(), project_root.clone()],
        )
        .await
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

    #[tokio::test]
    async fn sessions_export_markdown_writes_transcript() {
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

        let exported = runtime.sessions_export_markdown(session.id).await.unwrap();
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

    #[tokio::test]
    async fn edit_diff_artifact_read_rejects_parent_components() {
        let project_root = unique_temp_dir("project-root");
        let artifact_dir = project_root.join(".yode").join("edit-diffs");
        std::fs::create_dir_all(&artifact_dir).unwrap();

        let error = read_edit_diff_artifact_from_roots(
            ".yode/edit-diffs/../secret.diff",
            &[project_root.clone()],
        )
        .await
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
