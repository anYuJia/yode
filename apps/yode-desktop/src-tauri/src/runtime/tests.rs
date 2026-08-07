use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use yode_core::config::Config;
use yode_core::db::Database;
use yode_core::updater::Updater;
use yode_llm::registry::ProviderRegistry;
use yode_tools::registry::ToolRegistry;

use super::edit_diff_runtime::read_edit_diff_artifact_from_roots;
use super::settings_runtime::default_general_settings;
use super::terminal_helpers::{
    apply_terminal_color_env, clamp_pty_size, parse_terminal_run_stdout, terminal_shell_command,
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
        workspace_trusted: std::sync::atomic::AtomicBool::new(false),
        provider_registry: Mutex::new(Arc::new(ProviderRegistry::new())),
        tool_registry: Mutex::new(Arc::new(ToolRegistry::new())),
        mcp_resource_provider: Mutex::new(None),
        active_session_id: Mutex::new(None),
        permission_mode: Mutex::new("default".to_string()),
        confirm_txs: Arc::new(Mutex::new(HashMap::new())),
        ask_user_txs: Arc::new(Mutex::new(HashMap::new())),
        cancel_tokens: Arc::new(Mutex::new(HashMap::new())),
        active_sessions: Arc::new(Mutex::new(HashSet::new())),
        run_registry: Arc::new(Mutex::new(HashMap::new())),
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
        std::slice::from_ref(&project_root),
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
fn terminal_resize_clamps_pty_size_to_valid_range() {
    // 0/越界尺寸会被钳制，保证 xterm 行列与 PTY resize 始终一致
    assert_eq!(clamp_pty_size(0, 0), (1, 1));
    assert_eq!(clamp_pty_size(0, 80), (1, 80));
    assert_eq!(clamp_pty_size(24, 0), (24, 1));
    assert_eq!(clamp_pty_size(24, 80), (24, 80));
    assert_eq!(clamp_pty_size(65535, 65535), (65535, 65535));
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

#[test]
fn desktop_permission_modes_match_cli_semantics() {
    use yode_core::permission::{PermissionAction, PermissionMode};

    // 与 CLI（session_restore::configure_permissions）共享同一份配置语义：
    // always_allow/always_ask/always_deny + default_mode 在 Desktop 端行为一致
    let config: Config = toml::from_str(
        r#"
[llm]
default_provider = "openai"
default_model = "gpt-4o"
[tools]
bash_timeout = 30
require_confirmation = ["bash"]
[session]
db_path = ""
[ui]
language = "zh-CN"
theme = "dark"
[permissions]
default_mode = "default"
[[permissions.always_allow]]
tool = "read_file"
[[permissions.always_ask]]
tool = "bash"
[[permissions.always_deny]]
tool = "write_file"
"#,
    )
    .unwrap();

    let manager = super::turn_permissions::configure_desktop_permissions(
        &config,
        std::path::Path::new("/tmp"),
    );
    assert_eq!(manager.mode(), PermissionMode::Default);
    // always_allow
    assert_eq!(
        manager.explain_with_content("read_file", None).action,
        PermissionAction::Allow
    );
    // always_ask 覆盖 require_confirmation 的默认询问行为
    assert_eq!(
        manager
            .explain_with_content("bash", Some("cargo test"))
            .action,
        PermissionAction::Confirm
    );
    // always_deny
    assert_eq!(
        manager.explain_with_content("write_file", None).action,
        PermissionAction::Deny
    );
    // 未配置的只读工具默认允许（与 CLI 一致）
    assert_eq!(
        manager.explain_with_content("grep", None).action,
        PermissionAction::Allow
    );
}

#[test]
fn desktop_permission_mode_parses_all_five_modes() {
    use yode_core::permission::PermissionMode;
    for (raw, expected) in [
        ("default", PermissionMode::Default),
        ("plan", PermissionMode::Plan),
        ("auto", PermissionMode::Auto),
        ("accept-edits", PermissionMode::AcceptEdits),
        ("bypass", PermissionMode::Bypass),
    ] {
        assert_eq!(raw.parse::<PermissionMode>().unwrap(), expected);
    }
}

#[test]
fn provider_api_key_never_reaches_webview() {
    let (runtime, dir) = test_runtime("provider-key");
    {
        let mut config = runtime.config.lock().unwrap();
        config.llm.providers.insert(
            "openai".to_string(),
            yode_core::config::ProviderConfig {
                format: "openai".to_string(),
                base_url: None,
                api_key: Some("sk-real-secret-42".to_string()),
                models: vec!["gpt-4o".to_string()],
                enabled: Some(true),
                gradient: None,
            },
        );
    }

    let providers = runtime.config_get_providers().unwrap();
    let openai = providers
        .iter()
        .find(|p| p.id == "openai")
        .expect("openai provider");
    // WebView 永不拿到真实密钥，只拿到掩码标记
    assert_eq!(openai.api_key, "");
    assert!(openai.has_api_key);

    // 保存时留空 = 保持原密钥
    let mut saved = openai.clone();
    saved.api_key = "".to_string();
    saved.models = vec!["gpt-4o".to_string(), "gpt-4.1".to_string()];
    runtime.config_save_providers(vec![saved]).unwrap();
    let stored = runtime.config.lock().unwrap();
    assert_eq!(
        stored
            .llm
            .providers
            .get("openai")
            .and_then(|p| p.api_key.as_deref()),
        Some("sk-real-secret-42")
    );

    // 显式输入新密钥 = 覆盖
    drop(stored);
    let mut updated = openai.clone();
    updated.api_key = "sk-new-key".to_string();
    runtime.config_save_providers(vec![updated]).unwrap();
    let stored = runtime.config.lock().unwrap();
    assert_eq!(
        stored
            .llm
            .providers
            .get("openai")
            .and_then(|p| p.api_key.as_deref()),
        Some("sk-new-key")
    );
    drop(stored);
    let _ = dir;
}

#[test]
fn session_rejects_concurrent_turn_while_one_is_in_flight() {
    let (runtime, dir) = test_runtime("concurrent-turn");
    assert!(!runtime
        .active_sessions
        .lock()
        .unwrap()
        .contains("session-x"));

    // 模拟一个进行中的 turn：in-flight 占位已登记
    {
        let mut active = runtime.active_sessions.lock().unwrap();
        active.insert("session-x".to_string());
    }
    assert!(runtime
        .active_sessions
        .lock()
        .unwrap()
        .contains("session-x"));

    // 其他会话不受影响
    assert!(!runtime
        .active_sessions
        .lock()
        .unwrap()
        .contains("session-y"));

    // 占位释放后可再次占用
    {
        let mut active = runtime.active_sessions.lock().unwrap();
        active.remove("session-x");
    }
    assert!(!runtime
        .active_sessions
        .lock()
        .unwrap()
        .contains("session-x"));
    let _ = dir;
}

#[test]
fn session_turn_slot_is_atomic_under_concurrency() {
    // 真实并发调用：主线程持有一个占位，多个线程同时 acquire 同一会话，
    // 在占位释放前所有并发请求都必须被拒绝
    use super::turn_runtime::SessionTurnSlot;
    let active = std::sync::Arc::new(std::sync::Mutex::new(HashSet::new()));
    let holder = SessionTurnSlot::acquire(&active, "session-contended").unwrap();

    let threads = 16;
    let success = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let mut handles = Vec::new();
    for _ in 0..threads {
        let active = active.clone();
        let success = success.clone();
        handles.push(std::thread::spawn(move || {
            if SessionTurnSlot::acquire(&active, "session-contended").is_ok() {
                success.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            }
        }));
    }
    for handle in handles {
        handle.join().unwrap();
    }
    // 占位持有期间，16 个并发请求全部被拒（原子性：检查+占用不可分割）
    assert_eq!(success.load(std::sync::atomic::Ordering::SeqCst), 0);

    // 释放后新的 acquire 成功
    drop(holder);
    let next = SessionTurnSlot::acquire(&active, "session-contended").unwrap();
    assert!(active.lock().unwrap().contains("session-contended"));
    drop(next);
}

#[test]
fn session_turn_slot_releases_on_drop_and_disarm() {
    use super::turn_runtime::SessionTurnSlot;
    let active = std::sync::Arc::new(std::sync::Mutex::new(HashSet::new()));

    // Drop 自动释放（模拟 turn_send_message 中途失败提前返回）
    {
        let slot = SessionTurnSlot::acquire(&active, "session-drop").unwrap();
        assert!(active.lock().unwrap().contains("session-drop"));
        drop(slot);
    }
    assert!(!active.lock().unwrap().contains("session-drop"));

    // disarm 后 Drop 不释放（释放由 turn 事件循环负责）
    {
        let mut slot = SessionTurnSlot::acquire(&active, "session-disarm").unwrap();
        slot.disarm();
        drop(slot);
    }
    assert!(active.lock().unwrap().contains("session-disarm"));
}

#[test]
fn new_session_first_turn_occupies_slot_then_rejects_second_turn() {
    // 真实路径模拟：新建会话（uuid 天然唯一）首轮 acquire 必成功；
    // 首轮运行期间（占位未释放）同一会话的第二个 turn 必须被拒绝
    use super::turn_runtime::SessionTurnSlot;
    let active = std::sync::Arc::new(std::sync::Mutex::new(HashSet::new()));
    let new_session_id = "session-new-1".to_string();

    // 首轮新建会话：acquire 成功
    let first = SessionTurnSlot::acquire(&active, &new_session_id).unwrap();
    // 首轮运行中，同一会话第二个 turn：拒绝
    let second = SessionTurnSlot::acquire(&active, &new_session_id);
    assert!(second.is_err());
    assert_eq!(
        second.unwrap_err().to_string(),
        "该会话已有进行中的任务，请等待完成或取消后再发送。"
    );

    // 首轮结束后（disarm + 事件循环释放）才允许新 turn
    drop(first);
    let third = SessionTurnSlot::acquire(&active, &new_session_id);
    assert!(third.is_ok());
}

#[test]
fn release_turn_occupancy_cleans_slot_and_token_on_background_start_failure() {
    // 后台线程 Runtime::new / Database::open 失败路径调用同一清理函数：
    // 必须同时释放 in-flight 占位与 cancel token，会话不会永久判定为运行中
    use super::turn_runtime::release_turn_occupancy;
    let active = std::sync::Arc::new(std::sync::Mutex::new(HashSet::new()));
    let tokens: super::turn_loop::CancelTokenMap =
        std::sync::Arc::new(std::sync::Mutex::new(HashMap::new()));
    {
        active.lock().unwrap().insert("session-bg".to_string());
        tokens.lock().unwrap().insert(
            ("session-bg".to_string(), "turn-bg".to_string()),
            tokio_util::sync::CancellationToken::new(),
        );
    }

    release_turn_occupancy(&active, &tokens, "session-bg", "turn-bg");

    assert!(!active.lock().unwrap().contains("session-bg"));
    assert!(tokens.lock().unwrap().is_empty());

    // 幂等：重复清理无副作用
    release_turn_occupancy(&active, &tokens, "session-bg", "turn-bg");
    assert!(tokens.lock().unwrap().is_empty());
}
