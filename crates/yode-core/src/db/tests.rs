use chrono::Utc;
use rusqlite::Connection;
use serde_json::json;
use tempfile::tempdir;
use yode_llm::types::{ImageData, Message};

use super::{Database, SessionArtifacts};
use crate::session::Session;

#[test]
fn open_migrates_legacy_database_columns() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("legacy.db");
    {
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE sessions (
                id TEXT PRIMARY KEY,
                name TEXT,
                provider TEXT NOT NULL,
                model TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE TABLE messages (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT NOT NULL,
                role TEXT NOT NULL,
                content TEXT,
                tool_calls_json TEXT,
                tool_call_id TEXT,
                created_at TEXT NOT NULL
            );
            CREATE TABLE session_artifacts (
                session_id TEXT PRIMARY KEY,
                last_compaction_mode TEXT,
                last_compaction_at TEXT,
                last_compaction_summary_excerpt TEXT,
                last_compaction_session_memory_path TEXT,
                last_compaction_transcript_path TEXT,
                last_session_memory_update_at TEXT,
                last_session_memory_update_path TEXT,
                last_session_memory_generated_summary INTEGER NOT NULL DEFAULT 0,
                updated_at TEXT NOT NULL
            );",
        )
        .unwrap();
    }

    let db = Database::open(&path).unwrap();
    db.create_session(&Session {
        id: "legacy-session".to_string(),
        name: None,
        project_root: Some("/tmp/legacy".to_string()),
        provider: "mock".to_string(),
        model: "mock-model".to_string(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    })
    .unwrap();
    db.save_message_with_metadata(
        "legacy-session",
        "assistant",
        Some("ok"),
        Some("reasoning"),
        None,
        None,
        Some(&json!({"migrated": true})),
    )
    .unwrap();
    db.upsert_session_artifacts(
        "legacy-session",
        &SessionArtifacts {
            last_compaction_mode: None,
            last_compaction_at: None,
            last_compaction_summary_excerpt: None,
            last_compaction_session_memory_path: None,
            last_compaction_transcript_path: None,
            last_compact_boundary_json: Some(r#"{"legacy":true}"#.to_string()),
            last_session_memory_update_at: None,
            last_session_memory_update_path: None,
            last_session_memory_generated_summary: false,
        },
    )
    .unwrap();

    let messages = db.load_messages("legacy-session").unwrap();
    assert_eq!(messages[0].reasoning.as_deref(), Some("reasoning"));
    assert_eq!(
        messages[0].metadata_json.as_deref(),
        Some(r#"{"migrated":true}"#)
    );

    let sessions = db.list_sessions_with_artifacts(10).unwrap();
    assert_eq!(
        sessions[0].session.project_root.as_deref(),
        Some("/tmp/legacy")
    );
    assert_eq!(
        sessions[0].artifacts.last_compact_boundary_json.as_deref(),
        Some(r#"{"legacy":true}"#)
    );
}

#[test]
fn replace_messages_overwrites_previous_session_history() {
    let temp = tempdir().unwrap();
    let db = Database::open(&temp.path().join("sessions.db")).unwrap();
    db.create_session(&Session {
        id: "session-1".to_string(),
        name: None,
        project_root: None,
        provider: "mock".to_string(),
        model: "mock-model".to_string(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    })
    .unwrap();

    db.save_message("session-1", "user", Some("old"), None, None, None)
        .unwrap();
    db.save_message("session-1", "assistant", Some("older"), None, None, None)
        .unwrap();

    db.replace_messages(
        "session-1",
        &[
            Message::user("new user"),
            Message::assistant("new assistant"),
        ],
    )
    .unwrap();

    let messages = db.load_messages("session-1").unwrap();
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].content.as_deref(), Some("new user"));
    assert_eq!(messages[1].content.as_deref(), Some("new assistant"));
}

#[test]
fn replace_messages_preserves_user_images() {
    let temp = tempdir().unwrap();
    let db = Database::open(&temp.path().join("sessions.db")).unwrap();
    db.create_session(&Session {
        id: "session-images".to_string(),
        name: None,
        project_root: None,
        provider: "mock".to_string(),
        model: "mock-model".to_string(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    })
    .unwrap();

    db.replace_messages(
        "session-images",
        &[Message::user_with_images(
            "inspect",
            vec![ImageData {
                base64: "ZmFrZQ==".to_string(),
                media_type: "image/png".to_string(),
            }],
        )],
    )
    .unwrap();

    let messages = db.load_messages("session-images").unwrap();
    let images: Vec<ImageData> =
        serde_json::from_str(messages[0].images_json.as_deref().unwrap()).unwrap();
    assert_eq!(images[0].media_type, "image/png");
    assert_eq!(images[0].base64, "ZmFrZQ==");
}

#[test]
fn replace_messages_keeps_retained_rows_metadata_images_and_order() {
    let temp = tempdir().unwrap();
    let db = Database::open(&temp.path().join("sessions.db")).unwrap();
    db.create_session(&Session {
        id: "snapshot-session".to_string(),
        name: None,
        project_root: None,
        provider: "mock".to_string(),
        model: "mock-model".to_string(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    })
    .unwrap();
    {
        let conn = db.lock_connection().unwrap();
        for (tool_call_id, image, metadata) in [
            ("call-a", "Zmlyc3Q=", r#"{"source":"first"}"#),
            ("call-b", "c2Vjb25k", r#"{"source":"second"}"#),
        ] {
            conn.execute(
                "INSERT INTO messages (session_id, role, content, tool_call_id, images_json, metadata_json, sort_order, created_at) VALUES (?1, 'tool', 'same output', ?2, ?3, ?4, ?5, ?6)",
                rusqlite::params![
                    "snapshot-session",
                    tool_call_id,
                    format!(r#"[{{"base64":"{image}","media_type":"image/png"}}]"#),
                    metadata,
                    if tool_call_id == "call-a" { 0 } else { 1 },
                    format!("2026-01-01T00:00:0{}Z", if tool_call_id == "call-a" { 1 } else { 2 }),
                ],
            )
            .unwrap();
        }
    }

    let stored = db.load_messages("snapshot-session").unwrap();
    let retained = stored[1].to_message().unwrap();
    let retained_id = stored[1].id;
    let retained_created_at = stored[1].created_at;
    let ids = db
        .replace_messages(
            "snapshot-session",
            &[Message::system("[Context summary] compressed"), retained],
        )
        .unwrap();

    assert_eq!(ids[1], retained_id);
    let after = db.load_messages("snapshot-session").unwrap();
    assert_eq!(after.len(), 2);
    assert_eq!(
        after[0].content.as_deref(),
        Some("[Context summary] compressed")
    );
    assert_eq!(after[1].id, retained_id);
    assert_eq!(after[1].tool_call_id.as_deref(), Some("call-b"));
    assert_eq!(after[1].created_at, retained_created_at);
    assert_eq!(
        after[1].images_json.as_deref(),
        Some(r#"[{"base64":"c2Vjb25k","media_type":"image/png"}]"#)
    );
    assert_eq!(
        after[1].metadata_json.as_deref(),
        Some(r#"{"source":"second"}"#)
    );
}

#[test]
fn replace_messages_rolls_back_when_a_later_snapshot_message_is_invalid() {
    let temp = tempdir().unwrap();
    let db = Database::open(&temp.path().join("sessions.db")).unwrap();
    db.create_session(&Session {
        id: "rollback-session".to_string(),
        name: None,
        project_root: None,
        provider: "mock".to_string(),
        model: "mock-model".to_string(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    })
    .unwrap();
    db.save_message_with_metadata(
        "rollback-session",
        "tool",
        Some("same output"),
        None,
        None,
        Some("call-1"),
        Some(&json!({"activity": "preserve"})),
    )
    .unwrap();
    let before = db.load_messages("rollback-session").unwrap();
    let mut invalid = Message::user("new message");
    invalid.storage_id = Some(9_999_999);

    assert!(db
        .replace_messages(
            "rollback-session",
            &[before[0].to_message().unwrap(), invalid],
        )
        .is_err());

    let after = db.load_messages("rollback-session").unwrap();
    assert_eq!(after.len(), 1);
    assert_eq!(after[0].id, before[0].id);
    assert_eq!(after[0].content, before[0].content);
    assert_eq!(after[0].metadata_json, before[0].metadata_json);
    assert_eq!(after[0].images_json, before[0].images_json);
}

#[test]
fn save_message_preserves_metadata_json() {
    let temp = tempdir().unwrap();
    let db = Database::open(&temp.path().join("sessions.db")).unwrap();
    db.create_session(&Session {
        id: "session-metadata".to_string(),
        name: None,
        project_root: None,
        provider: "mock".to_string(),
        model: "mock-model".to_string(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    })
    .unwrap();

    db.save_message_with_metadata(
        "session-metadata",
        "tool",
        Some("ok"),
        None,
        None,
        Some("call-1"),
        Some(&json!({
            "activity": {
                "kind": "run",
                "command": "git status --short"
            }
        })),
    )
    .unwrap();

    let messages = db.load_messages("session-metadata").unwrap();
    let metadata: serde_json::Value =
        serde_json::from_str(messages[0].metadata_json.as_deref().unwrap()).unwrap();
    assert_eq!(metadata["activity"]["kind"], json!("run"));
    assert_eq!(metadata["activity"]["command"], json!("git status --short"));
}

#[test]
fn upsert_session_artifacts_persists_and_lists_metadata() {
    let temp = tempdir().unwrap();
    let db = Database::open(&temp.path().join("sessions.db")).unwrap();
    db.create_session(&Session {
        id: "session-1".to_string(),
        name: Some("demo".to_string()),
        project_root: Some("/tmp/yode".to_string()),
        provider: "mock".to_string(),
        model: "mock-model".to_string(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    })
    .unwrap();

    db.upsert_session_artifacts(
        "session-1",
        &SessionArtifacts {
            last_compaction_mode: Some("manual".to_string()),
            last_compaction_at: Some("2026-01-01 10:00:00".to_string()),
            last_compaction_summary_excerpt: Some("summary".to_string()),
            last_compaction_session_memory_path: Some("/tmp/session.md".to_string()),
            last_compaction_transcript_path: Some("/tmp/transcript.md".to_string()),
            last_compact_boundary_json: Some(r#"{"mode":"manual"}"#.to_string()),
            last_session_memory_update_at: Some("2026-01-01 10:05:00".to_string()),
            last_session_memory_update_path: Some("/tmp/live.md".to_string()),
            last_session_memory_generated_summary: true,
        },
    )
    .unwrap();

    let sessions = db.list_sessions_with_artifacts(10).unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(
        sessions[0].artifacts.last_compaction_mode.as_deref(),
        Some("manual")
    );
    assert_eq!(
        sessions[0].session.project_root.as_deref(),
        Some("/tmp/yode")
    );
    assert_eq!(
        sessions[0]
            .artifacts
            .last_compaction_transcript_path
            .as_deref(),
        Some("/tmp/transcript.md")
    );
    assert_eq!(
        sessions[0].artifacts.last_compact_boundary_json.as_deref(),
        Some(r#"{"mode":"manual"}"#)
    );
    assert!(sessions[0].artifacts.last_session_memory_generated_summary);
}

#[test]
fn open_enables_pragmas_and_sets_schema_version() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("pragma.db");
    {
        let db = Database::open(&path).unwrap();
        let conn = db.lock_connection().unwrap();
        let foreign_keys: i64 = conn
            .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
            .unwrap();
        assert_eq!(foreign_keys, 1, "foreign_keys must be ON");
        let journal_mode: String = conn
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .unwrap();
        assert_eq!(journal_mode.to_lowercase(), "wal");
        let busy_timeout: i64 = conn
            .query_row("PRAGMA busy_timeout", [], |row| row.get(0))
            .unwrap();
        assert_eq!(busy_timeout, 5_000);
        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, 6, "all migrations must be applied");
    }
    // 重新打开后 pragma 仍生效（WAL 持久化在库上，user_version 持久化）
    let db = Database::open(&path).unwrap();
    let conn = db.lock_connection().unwrap();
    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, 6);
}

#[test]
fn foreign_keys_enforced_on_delete() {
    let temp = tempdir().unwrap();
    let db = Database::open(&temp.path().join("fk.db")).unwrap();
    db.create_session(&Session {
        id: "fk-session".to_string(),
        name: None,
        project_root: None,
        provider: "mock".to_string(),
        model: "mock-model".to_string(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    })
    .unwrap();
    db.save_message("fk-session", "user", Some("hello"), None, None, None)
        .unwrap();

    // 外键约束启用时，删除不存在外键引用的消息应报错
    let conn = db.lock_connection().unwrap();
    let result = conn.execute(
        "INSERT INTO messages (session_id, role, content, created_at) VALUES ('ghost', 'user', 'x', '2024-01-01T00:00:00Z')",
        [],
    );
    assert!(
        result.is_err(),
        "orphan message insert must fail with FK enabled"
    );
}

#[test]
fn corrupt_timestamp_is_reported_as_corruption() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("corrupt.db");
    {
        let db = Database::open(&path).unwrap();
        db.create_session(&Session {
            id: "corrupt-session".to_string(),
            name: None,
            project_root: None,
            provider: "mock".to_string(),
            model: "mock-model".to_string(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        })
        .unwrap();
    }
    // 直接破坏时间戳
    {
        let conn = Connection::open(&path).unwrap();
        conn.execute(
            "UPDATE sessions SET created_at = 'not-a-timestamp' WHERE id = 'corrupt-session'",
            [],
        )
        .unwrap();
    }
    let db = Database::open(&path).unwrap();
    let error = db.get_session("corrupt-session").unwrap_err();
    assert!(
        error.to_string().contains("时间戳损坏"),
        "corruption must surface: {error}"
    );
}

fn insert_test_session(db: &Database, session_id: &str) {
    db.create_session(&Session {
        id: session_id.to_string(),
        name: None,
        project_root: None,
        provider: "mock".to_string(),
        model: "mock-model".to_string(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    })
    .unwrap();
}

#[test]
fn session_snapshot_reads_session_and_messages_in_one_transaction() {
    let temp = tempdir().unwrap();
    let db = Database::open(&temp.path().join("snapshot.db")).unwrap();
    insert_test_session(&db, "snapshot-session");
    db.save_message("snapshot-session", "user", Some("问题"), None, None, None)
        .unwrap();
    db.save_message(
        "snapshot-session",
        "assistant",
        Some("回答"),
        None,
        None,
        None,
    )
    .unwrap();
    db.save_message_with_metadata(
        "snapshot-session",
        "tool",
        Some("结果"),
        None,
        None,
        Some("tool-1"),
        Some(&json!({"ok": true})),
    )
    .unwrap();

    let snapshot = db
        .load_session_snapshot("snapshot-session")
        .unwrap()
        .expect("snapshot must exist");
    assert_eq!(snapshot.session.id, "snapshot-session");
    assert_eq!(snapshot.messages.len(), 3);
    assert_eq!(snapshot.messages[0].content.as_deref(), Some("问题"));
    assert_eq!(snapshot.messages[1].content.as_deref(), Some("回答"));
    assert_eq!(snapshot.messages[2].content.as_deref(), Some("结果"));
    assert_eq!(snapshot.messages[2].tool_call_id.as_deref(), Some("tool-1"));

    assert!(db
        .load_session_snapshot("missing-session")
        .unwrap()
        .is_none());
}

#[test]
fn snapshot_is_consistent_before_and_after_delete() {
    let temp = tempdir().unwrap();
    let db = Database::open(&temp.path().join("snapshot-delete.db")).unwrap();
    insert_test_session(&db, "del-session");
    db.save_message("del-session", "user", Some("before"), None, None, None)
        .unwrap();

    // 操作前快照：完整包含会话与消息
    let before = db
        .load_session_snapshot("del-session")
        .unwrap()
        .expect("before snapshot");
    assert_eq!(before.messages.len(), 1);

    // 删除会话后快照：不存在（操作后一致状态）
    db.delete_session("del-session").unwrap();
    assert!(db.load_session_snapshot("del-session").unwrap().is_none());
    assert!(before.messages[0].content.as_deref() == Some("before"));
}

#[test]
fn second_database_handle_is_rejected_for_same_session_and_data_stays_intact() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("lock.db");
    let db_a = Database::open(&path).unwrap();
    let db_b = Database::open(&path).unwrap();
    insert_test_session(&db_a, "lock-session");
    db_a.save_message("lock-session", "user", Some("保留的消息"), None, None, None)
        .unwrap();

    // 模拟进程 A（turn/压缩）持有跨进程锁
    let _lock_a = db_a.session_lock("lock-session").unwrap();

    // 进程 B 的同一 session 生命周期操作必须被拒绝
    let err = db_b
        .session_lock("lock-session")
        .expect_err("第二持有者必须被拒绝");
    assert!(err.to_string().contains("该会话正在其他进程中运行"));
    assert_eq!(db_b.load_messages("lock-session").unwrap().len(), 1);

    // 锁释放后进程 B 可以正常进入并修改
    drop(_lock_a);
    let _lock_b = db_b.session_lock("lock-session").unwrap();
    db_b.save_message(
        "lock-session",
        "user",
        Some("进程 B 新增"),
        None,
        None,
        None,
    )
    .unwrap();
    let messages = db_a.load_messages("lock-session").unwrap();
    assert_eq!(messages.len(), 2);
    assert!(messages
        .iter()
        .any(|m| m.content.as_deref() == Some("保留的消息")));
    assert!(messages
        .iter()
        .any(|m| m.content.as_deref() == Some("进程 B 新增")));
}

#[test]
fn clear_like_rewrite_cannot_overwrite_newer_messages_while_turn_lock_held() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("turn-lock.db");
    let db_turn = Database::open(&path).unwrap();
    let db_other = Database::open(&path).unwrap();
    insert_test_session(&db_turn, "turn-session");
    db_turn
        .save_message("turn-session", "user", Some("旧消息"), None, None, None)
        .unwrap();

    // 进程 A 的 turn 持锁
    let _turn_lock = db_turn.session_lock("turn-session").unwrap();

    // 进程 B 的 clear/compact/delete 入口（统一经 session_lock）必须被拒绝，
    // 旧消息不会被覆盖，也不会有并发快照重写发生
    let clear_err = db_other
        .session_lock("turn-session")
        .expect_err("clear 与 turn 并发必须被拒绝");
    assert!(clear_err.to_string().contains("该会话正在其他进程中运行"));
    let messages = db_turn.load_messages("turn-session").unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].content.as_deref(), Some("旧消息"));

    // turn 期间写入的新消息在锁释放后依然完整存在
    db_turn
        .save_message(
            "turn-session",
            "user",
            Some("turn 期间的新消息"),
            None,
            None,
            None,
        )
        .unwrap();
    drop(_turn_lock);
    let messages = db_other.load_messages("turn-session").unwrap();
    assert_eq!(messages.len(), 2);
    assert!(messages
        .iter()
        .any(|m| m.content.as_deref() == Some("turn 期间的新消息")));
}
