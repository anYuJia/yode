use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use yode_core::config::Config;
use yode_core::db::Database;
use yode_llm::registry::ProviderRegistry;
use yode_tools::registry::ToolRegistry;

use super::edit_diff_runtime::read_edit_diff_artifact_from_roots;
use super::settings_runtime::default_general_settings;
use super::terminal_helpers::{
    apply_terminal_color_env, clamp_pty_size, parse_terminal_run_stdout, terminal_shell_command,
};
use super::{find_workspace_root, DesktopRuntime};
use crate::protocol::{
    CreateSessionRequest, DesktopMcpEnvInput, DesktopMcpServerInput, DesktopProvider,
};

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
        db: Arc::new(Database::open(&db_path).unwrap()),
        db_path,
        workspace_path: dir.clone(),
        user_config_path: dir.join(".yode").join("config.toml"),
        workspace_trusted: std::sync::atomic::AtomicBool::new(false),
        provider_registry: Mutex::new(Arc::new(ProviderRegistry::new())),
        tool_registry: Mutex::new(Arc::new(ToolRegistry::new())),
        mcp_resource_provider: Mutex::new(None),
        active_session_id: Mutex::new(None),
        permission_mode: Mutex::new("default".to_string()),
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

#[cfg(unix)]
#[tokio::test]
async fn edit_diff_artifact_read_rejects_symlink_escape() {
    let project_root = unique_temp_dir("project-root-symlink");
    let outside_root = unique_temp_dir("outside-symlink");
    let artifact_dir = project_root.join(".yode").join("edit-diffs");
    std::fs::create_dir_all(&artifact_dir).unwrap();
    std::fs::create_dir_all(&outside_root).unwrap();
    std::fs::write(outside_root.join("secret.diff"), "secret\n").unwrap();
    std::os::unix::fs::symlink(
        outside_root.join("secret.diff"),
        artifact_dir.join("link.diff"),
    )
    .unwrap();

    let error = read_edit_diff_artifact_from_roots(
        ".yode/edit-diffs/link.diff",
        std::slice::from_ref(&project_root),
    )
    .await
    .unwrap_err()
    .to_string();

    assert!(error.contains("outside") || error.contains("symlink"));
    let _ = std::fs::remove_dir_all(project_root);
    let _ = std::fs::remove_dir_all(outside_root);
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
    let real_config_path = dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".yode")
        .join("config.toml");
    let real_config_metadata = config_file_metadata(&real_config_path);
    // 模拟正常启动场景：磁盘上已存在带密钥的提供者与 MCP 服务器
    // （通过统一事务入口写入，与生产路径一致）。
    runtime
        .update_user_config(|config| {
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
            config.mcp.servers.insert(
                "preserve-mcp".to_string(),
                yode_core::config::McpServerConfig {
                    command: "npx".to_string(),
                    args: vec!["-y".to_string(), "example-mcp".to_string()],
                    ..Default::default()
                },
            );
            Ok(())
        })
        .unwrap();

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
    assert!(runtime.user_config_path().is_file());
    assert_ne!(runtime.user_config_path(), real_config_path);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        assert_eq!(
            std::fs::metadata(runtime.user_config_path())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        assert_eq!(
            std::fs::metadata(runtime.user_config_path().parent().unwrap())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
    }
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
    assert_eq!(
        stored
            .mcp
            .servers
            .get("preserve-mcp")
            .map(|server| server.command.as_str()),
        Some("npx")
    );
    drop(stored);
    let persisted = Config::load_with_overrides(Some(&runtime.user_config_path()), None).unwrap();
    assert_eq!(
        persisted
            .llm
            .providers
            .get("openai")
            .and_then(|p| p.api_key.as_deref()),
        Some("sk-new-key")
    );
    assert_eq!(
        persisted
            .mcp
            .servers
            .get("preserve-mcp")
            .map(|server| server.command.as_str()),
        Some("npx")
    );
    assert_eq!(
        config_file_metadata(&real_config_path),
        real_config_metadata
    );
    let _ = dir;
}

fn config_file_metadata(path: &std::path::Path) -> Option<(u64, std::time::SystemTime)> {
    let metadata = std::fs::metadata(path).ok()?;
    Some((metadata.len(), metadata.modified().ok()?))
}

#[test]
fn concurrent_updates_from_independent_runtimes_preserve_both_config_domains() {
    use std::sync::Barrier;

    // 两个完全独立的运行时（模拟两个应用实例）共享同一个用户配置文件
    let shared_dir = unique_temp_dir("concurrent-config");
    std::fs::create_dir_all(shared_dir.join(".yode")).unwrap();
    let shared_config_path = shared_dir.join(".yode").join("config.toml");
    let (mut runtime_a, _dir_a) = test_runtime("concurrent-config-a");
    let (mut runtime_b, _dir_b) = test_runtime("concurrent-config-b");
    runtime_a.user_config_path = shared_config_path.clone();
    runtime_b.user_config_path = shared_config_path.clone();

    let barrier = Arc::new(Barrier::new(2));
    let barrier_a = Arc::clone(&barrier);
    let barrier_b = Arc::clone(&barrier);
    let thread_a = std::thread::spawn(move || {
        barrier_a.wait();
        // 域 A：默认 LLM
        runtime_a
            .config_set_default_llm("anthropic".to_string(), "claude-sonnet-4-5".to_string())
            .unwrap();
    });
    let thread_b = std::thread::spawn(move || {
        barrier_b.wait();
        // 域 B：权限默认模式（异步 RPC 在独立线程上执行）
        tauri::async_runtime::block_on(runtime_b.permission_mode_set(
            "plan".to_string(),
            false,
            None,
        ))
        .unwrap();
    });
    thread_a.join().unwrap();
    thread_b.join().unwrap();

    // 最终磁盘配置必须同时保留两项改动，且 TOML 可解析
    let persisted = Config::load_with_overrides(Some(&shared_config_path), None)
        .expect("并发事务后用户配置必须可解析");
    assert_eq!(persisted.llm.default_provider, "anthropic");
    assert_eq!(persisted.llm.default_model, "claude-sonnet-4-5");
    assert_eq!(persisted.permissions.default_mode.as_deref(), Some("plan"));
    // 未被任一事务触碰的字段仍是默认值（说明不是某一方的过期快照）
    assert_eq!(persisted.ui.theme, "dark");
    let _ = std::fs::remove_dir_all(&shared_dir);
}

#[test]
fn same_runtime_concurrent_updates_keep_memory_and_disk_consistent() {
    use std::sync::Barrier;

    // 同一运行时两个并发 RPC 修改不同配置域：进程内事务互斥保证
    // “锁内修改 -> 刷新内存”串行化，最终内存与磁盘都必须同时包含两项改动。
    let runtime = std::sync::Arc::new(test_runtime("same-runtime-concurrent").0);
    let barrier = Arc::new(Barrier::new(2));
    let barrier_a = Arc::clone(&barrier);
    let barrier_b = Arc::clone(&barrier);
    let runtime_a = Arc::clone(&runtime);
    let runtime_b = Arc::clone(&runtime);
    let thread_a = std::thread::spawn(move || {
        barrier_a.wait();
        runtime_a
            .config_set_default_llm("anthropic".to_string(), "claude-sonnet-4-5".to_string())
            .unwrap();
    });
    let thread_b = std::thread::spawn(move || {
        barrier_b.wait();
        tauri::async_runtime::block_on(runtime_b.permission_mode_set(
            "plan".to_string(),
            false,
            None,
        ))
        .unwrap();
    });
    thread_a.join().unwrap();
    thread_b.join().unwrap();

    // 内存快照 = 最后一次写入后的磁盘状态，两项改动并存（不得倒退）
    let memory = runtime.config.lock().unwrap();
    assert_eq!(memory.llm.default_provider, "anthropic");
    assert_eq!(memory.llm.default_model, "claude-sonnet-4-5");
    assert_eq!(memory.permissions.default_mode.as_deref(), Some("plan"));
    drop(memory);
    let persisted = Config::load_with_overrides(Some(&runtime.user_config_path()), None).unwrap();
    assert_eq!(persisted.llm.default_provider, "anthropic");
    assert_eq!(persisted.permissions.default_mode.as_deref(), Some("plan"));
}

#[test]
fn config_saves_preserve_unknown_top_level_and_nested_fields() {
    let runtime = std::sync::Arc::new(test_runtime("unknown-fields-e2e").0);
    std::fs::create_dir_all(runtime.user_config_path().parent().unwrap()).unwrap();
    std::fs::write(
        runtime.user_config_path(),
        r#"
future_top_level = "keep"

[experimental]
flag = true

[llm]
default_provider = "openai"
default_model = "gpt-4o"
[llm.providers.openai]
format = "openai"
api_key = "sk-top-secret"
legacy_retries = 3
[llm.providers.ollama]
format = "openai"
base_url = "http://localhost:11434/v1"
ollama_kwargs = { top_p = 0.9 }

[mcp.servers.docs]
command = "npx"
args = ["-y", "docs"]
future_mcp_field = 42
[mcp.servers.docs.auth]
bearer_token_env = "DOCS_TOKEN"
"#,
    )
    .unwrap();

    // 1) 默认模型（窄修改 llm 域）
    runtime
        .config_set_default_llm("ollama".to_string(), "llama3".to_string())
        .unwrap();
    // 2) 权限模式（窄修改 permissions 域）——异步 RPC 在独立线程上执行
    let runtime_b = Arc::clone(&runtime);
    std::thread::spawn(move || {
        tauri::async_runtime::block_on(runtime_b.permission_mode_set(
            "plan".to_string(),
            false,
            None,
        ))
        .unwrap();
    })
    .join()
    .unwrap();
    // 3) MCP 表单保存：与 mcp_servers_save 完全相同的文件锁事务（合并保留
    //    auth 与未知字段）。工具重载（需真实 MCP 连接）不在本测试范围。
    runtime
        .update_user_config(|config| {
            let existing = config.mcp.servers.clone();
            config.mcp.servers = super::mcp_config::desktop_mcp_servers_to_config(
                &[DesktopMcpServerInput {
                    name: "docs".to_string(),
                    transport: "stdio".to_string(),
                    command: Some("npx".to_string()),
                    args: vec!["-y".to_string(), "docs".to_string()],
                    url: None,
                    env: std::collections::HashMap::<String, DesktopMcpEnvInput>::new(),
                    disabled: false,
                }],
                &existing,
            )?;
            Ok(())
        })
        .unwrap();
    // 4) provider 表单保存（表单不携带密钥：留空保持原密钥）
    runtime
        .config_save_providers(vec![
            DesktopProvider {
                id: "openai".to_string(),
                name: "OpenAI".to_string(),
                format: "openai".to_string(),
                enabled: true,
                api_key: String::new(),
                has_api_key: true,
                base_url: String::new(),
                models: vec![],
                gradient: None,
            },
            DesktopProvider {
                id: "ollama".to_string(),
                name: "Ollama".to_string(),
                format: "openai".to_string(),
                enabled: true,
                api_key: String::new(),
                has_api_key: false,
                base_url: String::new(),
                models: vec![],
                gradient: None,
            },
        ])
        .unwrap();

    // 无损：未知顶层、未知嵌套字段、密钥与 MCP auth 全部保留
    let raw = std::fs::read_to_string(runtime.user_config_path()).unwrap();
    assert!(raw.contains("future_top_level = \"keep\""));
    assert!(raw.contains("[experimental]"));
    assert!(raw.contains("flag = true"));
    assert!(raw.contains("legacy_retries = 3"));
    assert!(raw.contains("ollama_kwargs = { top_p = 0.9 }"));
    assert!(raw.contains("future_mcp_field = 42"));
    assert!(raw.contains("bearer_token_env = \"DOCS_TOKEN\""));
    assert!(raw.contains("api_key = \"sk-top-secret\""));
    assert!(raw.contains("default_provider = \"ollama\""));
    assert!(raw.contains("default_mode = \"plan\""));
    // TOML 可解析且语义完整
    let persisted = Config::load_with_overrides(Some(&runtime.user_config_path()), None).unwrap();
    assert_eq!(persisted.llm.default_provider, "ollama");
    assert_eq!(persisted.llm.default_model, "llama3");
    assert_eq!(persisted.permissions.default_mode.as_deref(), Some("plan"));
    assert_eq!(
        persisted
            .llm
            .providers
            .get("openai")
            .and_then(|p| p.api_key.as_deref()),
        Some("sk-top-secret")
    );
    assert_eq!(
        persisted
            .mcp
            .servers
            .get("docs")
            .and_then(|server| server.auth.as_ref())
            .and_then(|auth| auth.bearer_token_env.as_deref()),
        Some("DOCS_TOKEN")
    );
}

#[test]
fn failed_config_transaction_leaves_memory_and_disk_unchanged() {
    let (runtime, dir) = test_runtime("failed-config-transaction");
    // 先建立合法的磁盘配置（值必须不同于默认，确保确实写盘）
    runtime
        .update_user_config(|config| {
            config.llm.default_model = "gpt-4.1".to_string();
            Ok(())
        })
        .unwrap();
    let before = runtime.config.lock().unwrap().llm.default_model.clone();
    let disk_before = std::fs::read(runtime.user_config_path()).unwrap();

    // 事务闭包报错：不得写盘、不得刷新内存
    let error = runtime
        .update_user_config(|_config| -> anyhow::Result<()> { anyhow::bail!("模拟保存失败") })
        .unwrap_err();
    assert!(error.to_string().contains("模拟保存失败"));
    assert_eq!(runtime.config.lock().unwrap().llm.default_model, before);
    assert_eq!(
        std::fs::read(runtime.user_config_path()).unwrap(),
        disk_before
    );
    let _ = dir;
}

#[test]
fn session_operation_slot_is_atomic_under_concurrency() {
    use super::turn_runtime::{SessionOperation, SessionOperationSlot};

    let active = std::sync::Arc::new(std::sync::Mutex::new(HashMap::new()));
    let holder =
        SessionOperationSlot::acquire(&active, "session-contended", SessionOperation::Turn)
            .unwrap();

    let threads = 16;
    let success = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let mut handles = Vec::new();
    for _ in 0..threads {
        let active = active.clone();
        let success = success.clone();
        handles.push(std::thread::spawn(move || {
            if SessionOperationSlot::acquire(
                &active,
                "session-contended",
                SessionOperation::ClearMessages,
            )
            .is_ok()
            {
                success.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            }
        }));
    }
    for handle in handles {
        handle.join().unwrap();
    }

    assert_eq!(success.load(std::sync::atomic::Ordering::SeqCst), 0);
    drop(holder);
    assert!(SessionOperationSlot::acquire(
        &active,
        "session-contended",
        SessionOperation::ClearMessages,
    )
    .is_ok());
}

#[test]
fn session_operation_slot_makes_turn_and_mutation_mutually_exclusive() {
    use super::turn_runtime::{SessionOperation, SessionOperationSlot};

    let active = std::sync::Arc::new(std::sync::Mutex::new(HashMap::new()));
    let operations = [
        (SessionOperation::Turn, "运行任务"),
        (SessionOperation::ClearMessages, "清空消息"),
        (SessionOperation::Delete, "删除会话"),
        (SessionOperation::CompactLocal, "本地压缩"),
        (SessionOperation::CompactEngine, "引擎压缩"),
    ];

    for (held_operation, held_label) in operations {
        let holder = SessionOperationSlot::acquire(&active, "session-x", held_operation).unwrap();
        for (candidate_operation, _) in operations {
            if candidate_operation == held_operation {
                continue;
            }
            let error = SessionOperationSlot::acquire(&active, "session-x", candidate_operation)
                .unwrap_err();
            assert_eq!(
                error.to_string(),
                format!("该会话正在{held_label}，请等待完成后重试。")
            );
        }
        drop(holder);
    }
}

#[test]
fn session_operation_slot_allows_different_sessions_to_proceed() {
    use super::turn_runtime::{SessionOperation, SessionOperationSlot};

    let active = std::sync::Arc::new(std::sync::Mutex::new(HashMap::new()));
    let first =
        SessionOperationSlot::acquire(&active, "session-a", SessionOperation::Turn).unwrap();
    let second =
        SessionOperationSlot::acquire(&active, "session-b", SessionOperation::CompactLocal)
            .unwrap();

    assert_eq!(active.lock().unwrap().len(), 2);
    drop(first);
    drop(second);
}

#[test]
fn failed_operations_and_drop_release_the_session_slot_for_retry() {
    use super::turn_runtime::{SessionOperation, SessionOperationSlot};

    let (runtime, dir) = test_runtime("operation-slot-retry");
    let missing = "missing-session".to_string();
    assert!(runtime.sessions_clear_messages(missing.clone()).is_err());
    assert!(!runtime
        .active_sessions
        .lock()
        .unwrap()
        .contains_key(&missing));

    let first = SessionOperationSlot::acquire(
        &runtime.active_sessions,
        "session-drop",
        SessionOperation::Delete,
    )
    .unwrap();
    drop(first);
    assert!(SessionOperationSlot::acquire(
        &runtime.active_sessions,
        "session-drop",
        SessionOperation::Turn,
    )
    .is_ok());
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn destructive_session_operations_reject_while_turn_runs_without_mutating_history() {
    use super::turn_runtime::{SessionOperation, SessionOperationSlot};

    let (runtime, dir) = test_runtime("operation-slot-preserves-history");
    let session = runtime
        .sessions_create(CreateSessionRequest {
            title: Some("protected history".to_string()),
            project_root: None,
            provider: None,
            model: None,
        })
        .unwrap();
    for index in 0..24 {
        runtime
            .db
            .save_message(
                &session.id,
                if index % 2 == 0 { "user" } else { "assistant" },
                Some(&format!("protected message {index}")),
                None,
                None,
                None,
            )
            .unwrap();
    }
    let before = runtime.sessions_messages(session.id.clone()).unwrap();
    let _turn = SessionOperationSlot::acquire(
        &runtime.active_sessions,
        &session.id,
        SessionOperation::Turn,
    )
    .unwrap();

    for error in [
        runtime
            .sessions_clear_messages(session.id.clone())
            .unwrap_err(),
        runtime
            .sessions_compact_local(session.id.clone())
            .unwrap_err(),
        runtime.sessions_delete(session.id.clone()).unwrap_err(),
        runtime
            .sessions_compact_engine(session.id.clone())
            .await
            .unwrap_err(),
    ] {
        assert_eq!(error.to_string(), "该会话正在运行任务，请等待完成后重试。");
    }

    let after = runtime.sessions_messages(session.id.clone()).unwrap();
    assert_eq!(after.len(), before.len());
    assert_eq!(after[0].content, before[0].content);
    assert!(runtime.db.get_session(&session.id).unwrap().is_some());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn release_turn_occupancy_cleans_slot_and_token_on_background_start_failure() {
    use super::turn_runtime::{release_turn_occupancy, SessionOperation, SessionOperationSlot};

    let active = std::sync::Arc::new(std::sync::Mutex::new(HashMap::new()));
    let tokens: super::turn_loop::CancelTokenMap =
        std::sync::Arc::new(std::sync::Mutex::new(HashMap::new()));
    let mut turn_slot =
        SessionOperationSlot::acquire(&active, "session-bg", SessionOperation::Turn).unwrap();
    turn_slot.disarm();
    drop(turn_slot);
    assert_eq!(
        active.lock().unwrap().get("session-bg"),
        Some(&SessionOperation::Turn)
    );
    {
        tokens.lock().unwrap().insert(
            ("session-bg".to_string(), "turn-bg".to_string()),
            tokio_util::sync::CancellationToken::new(),
        );
    }

    release_turn_occupancy(&active, &tokens, "session-bg", "turn-bg");

    assert!(!active.lock().unwrap().contains_key("session-bg"));
    assert!(tokens.lock().unwrap().is_empty());
    let later_operation =
        SessionOperationSlot::acquire(&active, "session-bg", SessionOperation::Delete).unwrap();

    // 旧 turn 的迟到收尾不能误删后来领取的破坏性操作槽位。
    release_turn_occupancy(&active, &tokens, "session-bg", "turn-bg");
    assert_eq!(
        active.lock().unwrap().get("session-bg"),
        Some(&SessionOperation::Delete)
    );
    drop(later_operation);

    release_turn_occupancy(&active, &tokens, "session-bg", "turn-bg");
    assert!(tokens.lock().unwrap().is_empty());
}

#[tokio::test]
async fn turn_slot_stays_occupied_until_task_join_completes() {
    use super::turn_loop::join_turn_then_release_occupancy;
    use super::turn_runtime::{SessionOperation, SessionOperationSlot};

    let active = std::sync::Arc::new(std::sync::Mutex::new(HashMap::new()));
    let tokens: super::turn_loop::CancelTokenMap =
        std::sync::Arc::new(std::sync::Mutex::new(HashMap::new()));
    let mut turn_slot =
        SessionOperationSlot::acquire(&active, "session-join", SessionOperation::Turn).unwrap();
    turn_slot.disarm();
    drop(turn_slot);
    tokens.lock().unwrap().insert(
        ("session-join".to_string(), "turn-join".to_string()),
        tokio_util::sync::CancellationToken::new(),
    );

    let (started_tx, started_rx) = tokio::sync::oneshot::channel();
    let (finish_tx, finish_rx) = tokio::sync::oneshot::channel();
    let handle = tokio::spawn(async move {
        let _ = started_tx.send(());
        let _ = finish_rx.await;
    });
    let active_for_cleanup = active.clone();
    let tokens_for_cleanup = tokens.clone();
    let cleanup = tokio::spawn(async move {
        join_turn_then_release_occupancy(
            handle,
            &active_for_cleanup,
            &tokens_for_cleanup,
            "session-join",
            "turn-join",
        )
        .await
    });

    started_rx.await.unwrap();
    tokio::task::yield_now().await;
    assert_eq!(
        active.lock().unwrap().get("session-join"),
        Some(&SessionOperation::Turn)
    );
    assert!(tokens
        .lock()
        .unwrap()
        .contains_key(&("session-join".to_string(), "turn-join".to_string())));

    finish_tx.send(()).unwrap();
    assert!(cleanup.await.unwrap());
    assert!(!active.lock().unwrap().contains_key("session-join"));
    assert!(tokens.lock().unwrap().is_empty());
}

#[tokio::test]
async fn desktop_export_creates_unique_files_for_consecutive_exports() {
    let (runtime, dir) = test_runtime("desktop-export-unique");
    let session = runtime
        .sessions_create(CreateSessionRequest {
            title: Some("export me".to_string()),
            project_root: None,
            provider: None,
            model: None,
        })
        .unwrap();
    runtime
        .db
        .save_message(&session.id, "user", Some("导出的消息"), None, None, None)
        .unwrap();

    let first = runtime
        .sessions_export_markdown(session.id.clone())
        .await
        .unwrap();
    let second = runtime
        .sessions_export_markdown(session.id.clone())
        .await
        .unwrap();

    assert_ne!(first.path, second.path, "同一秒连续导出不得互相覆盖");
    let content_a = std::fs::read_to_string(&first.path).unwrap();
    let content_b = std::fs::read_to_string(&second.path).unwrap();
    assert!(content_a.contains("导出的消息"));
    assert!(content_b.contains("导出的消息"));
    assert_eq!(first.message_count, 1);
    assert_eq!(second.message_count, 1);
    assert!(std::fs::metadata(&first.path).unwrap().is_file());
    assert!(std::fs::metadata(&second.path).unwrap().is_file());
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn desktop_export_and_clear_rejected_while_other_process_holds_session_lock() {
    let (runtime, dir) = test_runtime("desktop-lock-other-process");
    let session = runtime
        .sessions_create(CreateSessionRequest {
            title: Some("locked".to_string()),
            project_root: None,
            provider: None,
            model: None,
        })
        .unwrap();
    runtime
        .db
        .save_message(&session.id, "user", Some("保留消息"), None, None, None)
        .unwrap();
    let export_dir = dir.join(".yode").join("exports");
    let existing =
        yode_core::session_lock::write_unique_export_file(&export_dir, "existing", "旧导出")
            .unwrap();

    // 另一进程（CLI/其他桌面）持有该 session 的跨进程锁
    let _other_lock =
        yode_core::session_lock::acquire_session_lock(&runtime.db_path, &session.id).unwrap();

    let err = runtime
        .sessions_export_markdown(session.id.clone())
        .await
        .expect_err("其他进程持锁时导出必须被拒绝");
    assert!(err.to_string().contains("该会话正在其他进程中运行"));

    let err = runtime
        .sessions_clear_messages(session.id.clone())
        .expect_err("其他进程持锁时清空必须被拒绝");
    assert!(err.to_string().contains("该会话正在其他进程中运行"));

    // 数据与旧导出文件保持完整
    let messages = runtime.sessions_messages(session.id.clone()).unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].content.as_deref(), Some("保留消息"));
    assert_eq!(
        std::fs::read_to_string(&existing).unwrap(),
        "旧导出",
        "锁失败时旧导出文件必须保持完整"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn desktop_uses_config_session_db_path_like_cli() {
    let dir = unique_temp_dir("desktop-config-db-path");
    std::fs::create_dir_all(&dir).unwrap();
    let custom_db = dir.join("custom").join("sessions.db");
    let user_config = dir.join("config.toml");
    std::fs::write(
        &user_config,
        format!("[session]\ndb_path = \"{}\"\n", custom_db.display()),
    )
    .unwrap();

    let config = super::configuration_runtime::load_desktop_config(&user_config)
        .await
        .unwrap();
    let desktop_path = super::desktop_session_db_path(&config);
    let cli_path = config.session_db_path();

    assert_eq!(
        desktop_path, cli_path,
        "桌面端与 CLI 必须使用同一个数据库文件"
    );
    assert_eq!(
        desktop_path, custom_db,
        "必须遵守配置中的 [session].db_path"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn runs_list_serves_persisted_turn_journal_as_authoritative_source() {
    use yode_core::db::TurnState;

    // 进程 A 的运行时：创建会话与 turn journal（含事件与状态流转）
    let (runtime_a, dir) = test_runtime("runs-journal");
    let session = runtime_a
        .sessions_create(CreateSessionRequest {
            title: Some("journal run".to_string()),
            project_root: None,
            provider: None,
            model: None,
        })
        .unwrap();
    runtime_a
        .db
        .create_turn(&session.id, "turn-journal-1")
        .unwrap();
    runtime_a
        .db
        .update_turn_state(
            &session.id,
            "turn-journal-1",
            TurnState::Running,
            None,
            None,
        )
        .unwrap();
    runtime_a
        .db
        .append_turn_event(&yode_core::db::TurnEvent {
            session_id: session.id.clone(),
            turn_id: "turn-journal-1".to_string(),
            seq: 0,
            kind: "turn_started".to_string(),
            timestamp: chrono::Utc::now(),
            payload_json: r#"{"body":"ok"}"#.to_string(),
        })
        .unwrap();
    runtime_a
        .db
        .update_turn_state(
            &session.id,
            "turn-journal-1",
            TurnState::Completed,
            None,
            None,
        )
        .unwrap();

    // 进程 B（模拟新进程）用同一数据库文件：runs_list 直接读持久化 journal
    let (mut runtime_b, _dir_b) = test_runtime("runs-journal-b");
    let shared_path = runtime_a.db_path.clone();
    runtime_b.db = Arc::new(Database::open(&shared_path).unwrap());
    runtime_b.db_path = shared_path;
    drop(runtime_a);

    let runs = runtime_b.runs_list().unwrap();
    let run = runs
        .iter()
        .find(|run| run.turn_id == "turn-journal-1")
        .expect("持久化 turn 必须出现在 runs_list");
    assert_eq!(run.status, "completed");
    assert_eq!(run.session_id, session.id);
    assert_eq!(run.last_seq, 0);
    assert!(run.started_at.is_some());
    assert!(run.ended_at.is_some());
    // 事件可重放
    let events = runtime_b
        .turn_events_since(session.id.clone(), "turn-journal-1".to_string(), -1, None)
        .unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].kind, "turn_started");
    assert_eq!(events[0].payload["body"], "ok");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn sessions_messages_page_windows_and_reports_has_more() {
    let (runtime, dir) = test_runtime("messages-page");
    let session = runtime
        .sessions_create(CreateSessionRequest {
            title: Some("page me".to_string()),
            project_root: None,
            provider: None,
            model: None,
        })
        .unwrap();
    for index in 0..25 {
        runtime
            .db
            .save_message(
                &session.id,
                if index % 2 == 0 { "user" } else { "assistant" },
                Some(&format!("page message {index:02}")),
                None,
                None,
                None,
            )
            .unwrap();
    }

    let first = runtime
        .sessions_messages_page(session.id.clone(), None, 10)
        .unwrap();
    assert_eq!(first.messages.len(), 10);
    assert!(first.has_more);
    assert_eq!(
        first.messages[0].content.as_deref(),
        Some("page message 24")
    );

    // 前端向上翻页：以已加载窗口最旧消息的 sort_order 作为 before
    let first_window = runtime
        .db
        .load_messages_window(&session.id, None, 10)
        .unwrap();
    let before = first_window
        .last()
        .map(|message| message.sort_order)
        .unwrap();
    let second = runtime
        .sessions_messages_page(session.id.clone(), Some(before), 10)
        .unwrap();
    assert_eq!(second.messages.len(), 10);
    assert!(second.has_more);
    assert_eq!(
        second.messages.last().and_then(|m| m.content.clone()),
        Some("page message 05".to_string())
    );
    let oldest_before = runtime
        .db
        .load_messages_window(&session.id, Some(before), 10)
        .unwrap()
        .last()
        .map(|message| message.sort_order)
        .unwrap();
    let last_page = runtime
        .sessions_messages_page(session.id.clone(), Some(oldest_before), 10)
        .unwrap();
    assert_eq!(last_page.messages.len(), 5);
    assert!(!last_page.has_more);
    assert_eq!(
        last_page.messages.last().and_then(|m| m.content.clone()),
        Some("page message 00".to_string())
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn turn_events_since_returns_redacted_payloads_safely() {
    use yode_core::db::TurnEvent;

    let (runtime, dir) = test_runtime("turn-events-redact");
    let session = runtime
        .sessions_create(CreateSessionRequest {
            title: Some("redact replay".to_string()),
            project_root: None,
            provider: None,
            model: None,
        })
        .unwrap();
    runtime.db.create_turn(&session.id, "turn-redact").unwrap();
    let secret_payload = serde_json::json!({
        "title": "工具调用",
        "body": "运行命令",
        "apiKey": "sk-live-secret1234567890",
        "authorization": "Bearer eyJhbGciOiJIUzI1NiJ9.xyz"
    });
    runtime
        .db
        .append_turn_event(&TurnEvent {
            session_id: session.id.clone(),
            turn_id: "turn-redact".to_string(),
            seq: 0,
            kind: "tool_started".to_string(),
            timestamp: chrono::Utc::now(),
            payload_json: secret_payload.to_string(),
        })
        .unwrap();

    // 通过 runtime 的 turn_events_since 重放：脱敏后的 payload 不泄漏密钥
    let events = runtime
        .turn_events_since(session.id.clone(), "turn-redact".to_string(), -1, None)
        .unwrap();
    assert_eq!(events.len(), 1);
    let payload = events[0].payload.to_string();
    assert!(payload.contains("[REDACTED]"));
    assert!(!payload.contains("sk-live-secret1234567890"));
    assert!(!payload.contains("eyJhbGciOiJIUzI1NiJ9"));
    assert!(events[0].session_id == session.id);
    assert!(events[0].turn_id == "turn-redact");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn turn_recent_events_returns_newest_window_in_ascending_order() {
    use yode_core::db::TurnEvent;

    let (runtime, dir) = test_runtime("turn-recent-events");
    let session = runtime
        .sessions_create(CreateSessionRequest {
            title: Some("recent events".to_string()),
            project_root: None,
            provider: None,
            model: None,
        })
        .unwrap();
    runtime.db.create_turn(&session.id, "turn-recent").unwrap();
    for seq in 0..5 {
        runtime
            .db
            .append_turn_event(&TurnEvent {
                session_id: session.id.clone(),
                turn_id: "turn-recent".to_string(),
                seq,
                kind: "assistant_text_delta".to_string(),
                timestamp: chrono::Utc::now(),
                payload_json: serde_json::json!({ "body": format!("chunk-{seq}") }).to_string(),
            })
            .unwrap();
    }

    let recent = runtime
        .turn_recent_events(session.id.clone(), "turn-recent".to_string(), 3)
        .unwrap();
    // 升序返回最近 3 条：chunk-2、chunk-3、chunk-4
    assert_eq!(recent.len(), 3);
    assert_eq!(recent[0].seq, 2);
    assert_eq!(recent[2].seq, 4);
    assert_eq!(recent[2].payload["body"], "chunk-4");
    let _ = std::fs::remove_dir_all(dir);
}
