use std::path::PathBuf;

use anyhow::Result;
use tracing::{info, warn};

use crate::Cli;
use yode_core::config::Config;
use yode_core::context::AgentContext;
use yode_core::db::{Database, StoredMessage};
use yode_core::permission::{PermissionConfig, PermissionManager, PermissionSourceView};
use yode_core::session::Session;
use yode_llm::types::Message;

#[derive(Debug, Clone, Default)]
pub(crate) struct SessionRestoreReport {
    pub mode: &'static str,
    pub fallback_reason: Option<String>,
    pub decoded_messages: usize,
    pub skipped_messages: usize,
}

pub(crate) fn configure_permissions(
    config: &Config,
    workdir: &std::path::Path,
) -> PermissionManager {
    let mut permissions =
        PermissionManager::from_confirmation_list(config.tools.require_confirmation.clone());

    let mut source_views = Vec::new();
    let layers = permission_layers(config, workdir);
    for (source, path, layer) in layers {
        // 仓库内配置（Project/Local）只能贡献 deny 收紧规则，不能切换权限模式。
        if !matches!(
            source,
            yode_core::permission::RuleSource::ProjectConfig
                | yode_core::permission::RuleSource::LocalConfig
        ) {
            if let Some(mode_str) = &layer.default_mode {
                if let Ok(mode) = mode_str.parse::<yode_core::PermissionMode>() {
                    permissions.set_mode(mode);
                }
            }
        }
        let rules = layer.to_rules(source);
        if !rules.is_empty() {
            permissions.add_rules(rules.clone());
        }
        source_views.push(PermissionSourceView {
            source,
            path,
            default_mode: layer.default_mode.clone(),
            rules,
        });
    }
    permissions.set_source_views(source_views);

    permissions
}

fn permission_layers(
    root_config: &Config,
    workdir: &std::path::Path,
) -> Vec<(
    yode_core::permission::RuleSource,
    Option<String>,
    PermissionConfig,
)> {
    use yode_core::permission::RuleSource;

    let mut layers = vec![(
        RuleSource::UserConfig,
        dirs::home_dir().map(|home| home.join(".yode").join("config.toml").display().to_string()),
        permission_config_from_runtime_config(root_config),
    )];

    let managed_path = dirs::home_dir()
        .map(|home| home.join(".yode").join("managed-config.toml"))
        .filter(|path| path.exists());
    if let Some(path) = managed_path.as_deref() {
        if let Some(config) = load_full_permission_config_from_path(path) {
            layers.push((
                RuleSource::ManagedConfig,
                Some(path.display().to_string()),
                config,
            ));
        }
    }

    let project_path = workdir.join(".yode").join("config.toml");
    if let Some(config) = load_tightening_permission_config_from_path(&project_path) {
        layers.push((
            RuleSource::ProjectConfig,
            Some(project_path.display().to_string()),
            config,
        ));
    }

    let local_path = workdir.join(".yode").join("config.local.toml");
    if let Some(config) = load_tightening_permission_config_from_path(&local_path) {
        layers.push((
            RuleSource::LocalConfig,
            Some(local_path.display().to_string()),
            config,
        ));
    }

    layers
}

fn permission_config_from_runtime_config(config: &Config) -> PermissionConfig {
    PermissionConfig {
        default_mode: config.permissions.default_mode.clone(),
        always_allow: config
            .permissions
            .always_allow
            .iter()
            .map(permission_rule_entry_to_config)
            .collect(),
        always_ask: config
            .permissions
            .always_ask
            .iter()
            .map(permission_rule_entry_to_config)
            .collect(),
        always_deny: config
            .permissions
            .always_deny
            .iter()
            .map(permission_rule_entry_to_config)
            .collect(),
    }
}

fn permission_rule_entry_to_config(
    entry: &yode_core::config::PermissionRuleEntry,
) -> yode_core::permission::PermissionRuleConfig {
    yode_core::permission::PermissionRuleConfig {
        tool: entry.tool.clone(),
        category: entry.category.clone(),
        pattern: entry.pattern.clone(),
        description: entry.description.clone(),
    }
}

/// 加载完整权限配置（用户配置、受管策略）：保留 default_mode 与全部规则。
fn load_full_permission_config_from_path(path: &std::path::Path) -> Option<PermissionConfig> {
    if !path.exists() {
        return None;
    }
    yode_core::config::Config::load_from(Some(path))
        .ok()
        .map(|config| permission_config_from_runtime_config(&config))
}

/// 仓库内配置（项目/本地覆盖）只能收紧：只保留 always_deny 规则，
/// 忽略 default_mode、always_allow 与 always_ask。
fn load_tightening_permission_config_from_path(path: &std::path::Path) -> Option<PermissionConfig> {
    if !path.exists() {
        return None;
    }
    yode_core::config::Config::load_from(Some(path))
        .ok()
        .map(|config| PermissionConfig {
            default_mode: None,
            always_allow: Vec::new(),
            always_ask: Vec::new(),
            always_deny: config
                .permissions
                .always_deny
                .iter()
                .map(permission_rule_entry_to_config)
                .collect(),
        })
}

pub(crate) fn restore_or_create_context(
    cli: &Cli,
    db: &Database,
    workdir: PathBuf,
    provider_name: String,
    model: String,
    output_style: String,
) -> Result<(AgentContext, Option<Vec<Message>>, SessionRestoreReport)> {
    if let Some(resume_id) = &cli.resume {
        if let Some(session) = resume_session_metadata(db, resume_id)? {
            info!("Resuming session: {}", resume_id);
            // BUG-003：恢复时以持久化的 project_root 为准，而不是当前 cwd，
            // 避免从其他目录 resume 时工具在错误的仓库运行。
            let restored_root = session
                .project_root
                .as_deref()
                .map(PathBuf::from)
                .filter(|root| root.is_dir())
                .unwrap_or(workdir);
            let mut context = AgentContext::resume(
                session.id.clone(),
                restored_root,
                session.provider.clone(),
                session.model.clone(),
            );
            context.output_style = output_style;
            let (messages, report) = restore_messages_full(db, resume_id)?;
            return Ok((context, Some(messages), report));
        }

        eprintln!("会话 '{}' 未找到，创建新会话。", resume_id);
        let mut context = AgentContext::new(workdir, provider_name, model);
        context.output_style = output_style;
        return Ok((
            context,
            None,
            SessionRestoreReport {
                mode: "new_session",
                fallback_reason: Some("resume_session_not_found".to_string()),
                decoded_messages: 0,
                skipped_messages: 0,
            },
        ));
    }

    let mut context = AgentContext::new(workdir, provider_name, model);
    context.output_style = output_style;
    Ok((
        context,
        None,
        SessionRestoreReport {
            mode: "new_session",
            fallback_reason: None,
            decoded_messages: 0,
            skipped_messages: 0,
        },
    ))
}

pub(crate) fn ensure_session_exists(db: &Database, context: &AgentContext) -> Result<()> {
    if context.is_resumed {
        return Ok(());
    }

    let session = Session {
        id: context.session_id.clone(),
        name: None,
        project_root: None,
        provider: context.provider.clone(),
        model: context.model.clone(),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    db.create_session(&session)?;
    Ok(())
}

pub(crate) async fn shutdown_mcp_clients(clients: Vec<yode_mcp::McpClient>) {
    for client in clients {
        if let Err(err) = client.shutdown().await {
            warn!(error = %err, "Error shutting down MCP client");
        }
    }
}

fn resume_session_metadata(db: &Database, resume_id: &str) -> Result<Option<Session>> {
    db.get_session(resume_id)
}

fn restore_messages_full(
    db: &Database,
    resume_id: &str,
) -> Result<(Vec<Message>, SessionRestoreReport)> {
    let stored = db.load_messages(resume_id)?;
    let total = stored.len();
    let decoded_messages = stored
        .into_iter()
        .filter_map(stored_message_to_message)
        .collect::<Vec<_>>();
    let report = SessionRestoreReport {
        mode: "full_transcript_restore",
        fallback_reason: None,
        decoded_messages: decoded_messages.len(),
        skipped_messages: total.saturating_sub(decoded_messages.len()),
    };
    Ok((decoded_messages, report))
}

fn stored_message_to_message(message: StoredMessage) -> Option<Message> {
    message.to_message()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use yode_core::permission::RuleSource;
    use yode_core::session::Session;

    fn test_cli(resume: Option<&str>) -> crate::Cli {
        crate::Cli {
            provider: None,
            model: None,
            config: None,
            workdir: None,
            resume: resume.map(str::to_string),
            serve_mcp: false,
            chat_message: None,
            yes: false,
            command: None,
        }
    }

    fn test_db() -> Database {
        // 每个测试独立数据库文件，避免并行测试互相污染。
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path()).unwrap();
        Database::open(&dir.path().join("sessions.db")).unwrap()
    }

    fn test_config() -> Config {
        let dir = std::env::temp_dir().join(format!(
            "yode-session-restore-config-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        std::fs::write(
            &path,
            r#"
[llm]
default_provider = "openai"
default_model = "gpt-4o"

[tools]
bash_timeout = 120
require_confirmation = ["bash"]

[session]
db_path = ""

[ui]
language = "zh-CN"
theme = "dark"
            "#,
        )
        .unwrap();
        let config = Config::load_from(Some(&path)).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        config
    }

    #[test]
    fn restore_path_uses_metadata_then_full_messages() {
        let db = test_db();
        db.create_session(&Session {
            id: "resume-1".to_string(),
            name: None,
            provider: "anthropic".to_string(),
            model: "claude".to_string(),
            project_root: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        })
        .unwrap();
        db.save_message("resume-1", "user", Some("hello"), None, None, None)
            .unwrap();

        let (context, restored, report) = restore_or_create_context(
            &test_cli(Some("resume-1")),
            &db,
            std::env::temp_dir(),
            "openai".to_string(),
            "gpt".to_string(),
            "learning".to_string(),
        )
        .unwrap();

        assert!(context.is_resumed);
        assert_eq!(context.output_style, "learning");
        assert_eq!(report.mode, "full_transcript_restore");
        assert_eq!(report.decoded_messages, 1);
        assert_eq!(restored.unwrap().len(), 1);
    }

    #[test]
    fn resume_uses_persisted_project_root_not_current_cwd() {
        let db = test_db();
        let saved_root = tempfile::tempdir().unwrap();
        db.create_session(&Session {
            id: "resume-root".to_string(),
            name: None,
            project_root: Some(saved_root.path().display().to_string()),
            provider: "anthropic".to_string(),
            model: "claude".to_string(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        })
        .unwrap();

        let (context, _, _) = restore_or_create_context(
            &test_cli(Some("resume-root")),
            &db,
            std::env::temp_dir(), // 故意从别的 cwd resume
            "openai".to_string(),
            "gpt".to_string(),
            "default".to_string(),
        )
        .unwrap();

        assert!(
            context.working_dir_compat().starts_with(saved_root.path()),
            "resume must use persisted project_root, got {:?}",
            context.working_dir_compat()
        );
    }

    #[test]
    fn restore_path_reports_missing_session_fallback() {
        let db = test_db();
        let (context, restored, report) = restore_or_create_context(
            &test_cli(Some("missing")),
            &db,
            std::env::temp_dir(),
            "openai".to_string(),
            "gpt".to_string(),
            "explanatory".to_string(),
        )
        .unwrap();

        assert!(!context.is_resumed);
        assert_eq!(context.output_style, "explanatory");
        assert!(restored.is_none());
        assert_eq!(report.mode, "new_session");
        assert_eq!(
            report.fallback_reason.as_deref(),
            Some("resume_session_not_found")
        );
    }

    #[test]
    fn restore_path_tracks_skipped_message_decodes() {
        let db = test_db();
        db.create_session(&Session {
            id: "resume-2".to_string(),
            name: None,
            provider: "anthropic".to_string(),
            model: "claude".to_string(),
            project_root: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        })
        .unwrap();
        db.save_message("resume-2", "user", Some("ok"), None, None, None)
            .unwrap();
        db.save_message("resume-2", "unknown-role", Some("skip"), None, None, None)
            .unwrap();

        let (_messages, report) = restore_messages_full(&db, "resume-2").unwrap();
        assert_eq!(report.decoded_messages, 1);
        assert_eq!(report.skipped_messages, 1);
    }

    #[test]
    fn configure_permissions_merges_project_and_local_layers() {
        let workdir =
            std::env::temp_dir().join(format!("yode-permission-layer-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&workdir);
        let yode_dir = workdir.join(".yode");
        std::fs::create_dir_all(&yode_dir).unwrap();
        std::fs::write(
            yode_dir.join("config.toml"),
            r#"
[llm]
default_provider = "openai"
default_model = "gpt-4o"

[tools]
bash_timeout = 120
require_confirmation = ["bash"]

[session]
db_path = ""

[ui]
language = "zh-CN"
theme = "dark"

[permissions]
default_mode = "plan"

[[permissions.always_deny]]
category = "write"
description = "project deny writes"
            "#,
        )
        .unwrap();
        std::fs::write(
            yode_dir.join("config.local.toml"),
            r#"
[llm]
default_provider = "openai"
default_model = "gpt-4o"

[tools]
bash_timeout = 120
require_confirmation = ["bash"]

[session]
db_path = ""

[ui]
language = "zh-CN"
theme = "dark"

[permissions]
default_mode = "accept-edits"

[[permissions.always_allow]]
tool = "write_file"
description = "local override"
            "#,
        )
        .unwrap();

        let permissions = configure_permissions(&test_config(), &workdir);
        // 仓库内配置不能切换模式：plan/accept-edits 均被忽略，保持 Default。
        assert_eq!(permissions.mode(), yode_core::PermissionMode::Default);
        let views = permissions.source_views_snapshot();
        assert!(views
            .iter()
            .any(|view| view.source == RuleSource::ProjectConfig));
        assert!(views
            .iter()
            .any(|view| view.source == RuleSource::LocalConfig));
        // 仓库内配置不能放宽：local 的 always_allow 被忽略，项目 deny 生效。
        let explanation = permissions.explain_with_content("write_file", None);
        assert_eq!(
            explanation.action,
            yode_core::permission::PermissionAction::Deny
        );
        let _ = std::fs::remove_dir_all(&workdir);
    }

    #[test]
    fn always_allow_ask_deny_matrix_applies_from_config() {
        use yode_core::permission::PermissionAction;

        let dir =
            std::env::temp_dir().join(format!("yode-permission-matrix-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        std::fs::write(
            &path,
            r#"
[llm]
default_provider = "openai"
default_model = "gpt-4o"

[tools]
bash_timeout = 120
require_confirmation = ["bash"]

[session]
db_path = ""

[ui]
language = "zh-CN"
theme = "dark"

[[permissions.always_allow]]
tool = "read_file"

[[permissions.always_ask]]
tool = "bash"

[[permissions.always_deny]]
tool = "write_file"
"#,
        )
        .unwrap();
        let config = Config::load_from(Some(&path)).unwrap();
        let permissions = configure_permissions(&config, &dir);

        // always_allow: 无需确认直接允许
        assert_eq!(
            permissions.explain_with_content("read_file", None).action,
            PermissionAction::Allow
        );
        // always_ask: 覆盖 require_confirmation 的默认询问行为（仍需要确认）
        assert_eq!(
            permissions
                .explain_with_content("bash", Some("cargo test"))
                .action,
            PermissionAction::Confirm
        );
        // always_deny: 直接拒绝
        assert_eq!(
            permissions.explain_with_content("write_file", None).action,
            PermissionAction::Deny
        );
        // 未配置的只读工具保持默认允许
        assert_eq!(
            permissions.explain_with_content("grep", None).action,
            PermissionAction::Allow
        );
        // 未配置且默认需确认的工具（如 git_commit）保持询问
        assert_eq!(
            permissions.explain_with_content("git_commit", None).action,
            PermissionAction::Confirm
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
