use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{atomic::AtomicU64, Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use yode_core::config::Config;
use yode_core::db::Database;
use yode_core::updater::Updater;
use yode_llm::registry::ProviderRegistry;
use yode_tools::registry::ToolRegistry;

use super::edit_diff_runtime::read_edit_diff_artifact_from_roots;
use super::settings_runtime::default_general_settings;
use super::terminal_helpers::{
    apply_terminal_color_env, parse_terminal_run_stdout, terminal_shell_command,
};
use super::{find_workspace_root, DesktopRuntime};
use crate::protocol::CreateSessionRequest;

fn test_config() -> Config {
    toml::from_str(include_str!("../../../../../config/default.toml")).unwrap()
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
        updater: Updater::new(
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".yode"),
            false,
            false,
        ),
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

#[tokio::test]
async fn workspace_root_detection_climbs_out_of_src_tauri() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap();
    let src_tauri = root.join("apps/yode-desktop/src-tauri");

    assert_eq!(
        find_workspace_root(&src_tauri).await.as_deref(),
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
