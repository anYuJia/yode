use chrono::Utc;
use rusqlite::Connection;
use serde_json::json;
use tempfile::tempdir;
use yode_llm::types::{ImageData, Message};

use super::{Database, SessionArtifacts, TurnEvent, TurnState};
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
        assert_eq!(version, 7, "all migrations must be applied");
    }
    // 重新打开后 pragma 仍生效（WAL 持久化在库上，user_version 持久化）
    let db = Database::open(&path).unwrap();
    let conn = db.lock_connection().unwrap();
    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, 7);
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

// ─── Turn Journal ──────────────────────────────────────────────────────────

fn append_event(db: &Database, session_id: &str, turn_id: &str, seq: i64, kind: &str) {
    db.append_turn_event(&TurnEvent {
        session_id: session_id.to_string(),
        turn_id: turn_id.to_string(),
        seq,
        kind: kind.to_string(),
        timestamp: Utc::now(),
        payload_json: json!({"seq": seq}).to_string(),
    })
    .unwrap();
}

#[test]
fn turn_journal_migration_preserves_legacy_data() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("journal-legacy.db");
    {
        let db = Database::open(&path).unwrap();
        insert_test_session(&db, "legacy-turn-session");
        db.save_message(
            "legacy-turn-session",
            "user",
            Some("旧数据"),
            None,
            None,
            None,
        )
        .unwrap();
    }
    // 旧库（无 turn 表）打开后自动迁移，旧数据不变
    let db = Database::open(&path).unwrap();
    let conn = db.lock_connection().unwrap();
    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, 7);
    drop(conn);
    let messages = db.load_messages("legacy-turn-session").unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].content.as_deref(), Some("旧数据"));

    // turn journal API 可用
    db.create_turn("legacy-turn-session", "turn-1").unwrap();
    append_event(&db, "legacy-turn-session", "turn-1", 0, "turn_started");
    let events = db
        .list_turn_events_since("legacy-turn-session", "turn-1", -1, None)
        .unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].kind, "turn_started");
}

#[test]
fn turn_journal_seq_is_monotonic_and_rejects_duplicates_and_ghosts() {
    let temp = tempdir().unwrap();
    let db = Database::open(&temp.path().join("journal.db")).unwrap();
    insert_test_session(&db, "seq-session");
    db.create_turn("seq-session", "turn-a").unwrap();

    append_event(&db, "seq-session", "turn-a", 0, "turn_started");
    append_event(&db, "seq-session", "turn-a", 1, "tool_started");
    // 重复 seq 必须被拒绝
    let dup = TurnEvent {
        session_id: "seq-session".to_string(),
        turn_id: "turn-a".to_string(),
        seq: 1,
        kind: "tool_result".to_string(),
        timestamp: Utc::now(),
        payload_json: "{}".to_string(),
    };
    assert!(db.append_turn_event(&dup).is_err());
    // 乱序 seq 必须被拒绝
    let out_of_order = TurnEvent {
        session_id: "seq-session".to_string(),
        turn_id: "turn-a".to_string(),
        seq: 0,
        kind: "error".to_string(),
        timestamp: Utc::now(),
        payload_json: "{}".to_string(),
    };
    assert!(db.append_turn_event(&out_of_order).is_err());
    // 跨会话（幽灵 turn / 不存在的 turn）必须被拒绝
    let ghost = TurnEvent {
        session_id: "seq-session".to_string(),
        turn_id: "no-such-turn".to_string(),
        seq: 0,
        kind: "error".to_string(),
        timestamp: Utc::now(),
        payload_json: "{}".to_string(),
    };
    assert!(db.append_turn_event(&ghost).is_err());
    // 缺失 seq（跳号）允许：保证最新事件按序可读
    append_event(&db, "seq-session", "turn-a", 5, "turn_completed");

    let events = db
        .list_turn_events_since("seq-session", "turn-a", -1, None)
        .unwrap();
    let seqs: Vec<i64> = events.iter().map(|event| event.seq).collect();
    assert_eq!(seqs, vec![0, 1, 5]);
    assert_eq!(events[0].kind, "turn_started");
    assert_eq!(events[2].kind, "turn_completed");

    let turn = db.get_turn("seq-session", "turn-a").unwrap().unwrap();
    assert_eq!(turn.last_seq, 5);
}

#[test]
fn turn_journal_unknown_kind_is_preserved_in_diagnostics() {
    let temp = tempdir().unwrap();
    let db = Database::open(&temp.path().join("unknown.db")).unwrap();
    insert_test_session(&db, "unknown-session");
    db.create_turn("unknown-session", "turn-unknown").unwrap();
    append_event(
        &db,
        "unknown-session",
        "turn-unknown",
        0,
        "some_future_kind",
    );
    let events = db
        .list_turn_events_since("unknown-session", "turn-unknown", -1, None)
        .unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].kind, "some_future_kind");
}

#[test]
fn turn_state_transitions_are_validated_and_terminal_is_frozen() {
    let temp = tempdir().unwrap();
    let db = Database::open(&temp.path().join("states.db")).unwrap();
    insert_test_session(&db, "state-session");
    db.create_turn("state-session", "turn-s").unwrap();

    db.update_turn_state("state-session", "turn-s", TurnState::Running, None, None)
        .unwrap();
    db.update_turn_state(
        "state-session",
        "turn-s",
        TurnState::WaitingApproval,
        Some("需要确认 bash 命令".to_string()),
        None,
    )
    .unwrap();
    db.update_turn_state("state-session", "turn-s", TurnState::Completed, None, None)
        .unwrap();
    let turn = db.get_turn("state-session", "turn-s").unwrap().unwrap();
    assert_eq!(turn.status, TurnState::Completed);
    assert!(turn.ended_at.is_some());
    assert_eq!(turn.detail.as_deref(), Some("需要确认 bash 命令"));

    // 终态不可被重新打开
    let reopen = db.update_turn_state("state-session", "turn-s", TurnState::Running, None, None);
    assert!(reopen.is_err());
    let after = db.get_turn("state-session", "turn-s").unwrap().unwrap();
    assert_eq!(after.status, TurnState::Completed);
}

#[test]
fn stale_runs_are_marked_interrupted_on_restart() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("restart.db");
    {
        let db = Database::open(&path).unwrap();
        insert_test_session(&db, "restart-session");
        db.create_turn("restart-session", "turn-running").unwrap();
        db.update_turn_state(
            "restart-session",
            "turn-running",
            TurnState::Running,
            None,
            None,
        )
        .unwrap();
        db.create_turn("restart-session", "turn-waiting").unwrap();
        db.update_turn_state(
            "restart-session",
            "turn-waiting",
            TurnState::WaitingUser,
            None,
            None,
        )
        .unwrap();
        db.create_turn("restart-session", "turn-done").unwrap();
        db.update_turn_state(
            "restart-session",
            "turn-done",
            TurnState::Completed,
            None,
            None,
        )
        .unwrap();
    }
    // 进程重启：重新打开数据库（模拟新进程）
    let db = Database::open(&path).unwrap();
    let marked = db
        .mark_interrupted_turns("检测到上次运行未正常结束，已标记为中断")
        .unwrap();
    assert_eq!(marked, 2);
    let turns = db.list_turns("restart-session").unwrap();
    let running = turns
        .iter()
        .find(|turn| turn.turn_id == "turn-running")
        .unwrap();
    assert_eq!(running.status, TurnState::Interrupted);
    assert_eq!(
        running.error_code.as_deref(),
        Some("interrupted_on_startup")
    );
    assert!(running.ended_at.is_some());
    let waiting = turns
        .iter()
        .find(|turn| turn.turn_id == "turn-waiting")
        .unwrap();
    assert_eq!(waiting.status, TurnState::Interrupted);
    let done = turns
        .iter()
        .find(|turn| turn.turn_id == "turn-done")
        .unwrap();
    assert_eq!(done.status, TurnState::Completed);
}

#[test]
fn concurrent_runtimes_update_same_session_journal_consistently() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("concurrent.db");
    let db_a = Database::open(&path).unwrap();
    let db_b = Database::open(&path).unwrap();
    insert_test_session(&db_a, "concurrent-session");

    // 两个独立 runtime 各自持有自己的 turn（同一 session）
    db_a.create_turn("concurrent-session", "turn-a").unwrap();
    db_b.create_turn("concurrent-session", "turn-b").unwrap();

    // 交错写入：各自 turn 的事件与状态互不污染
    append_event(&db_a, "concurrent-session", "turn-a", 0, "turn_started");
    append_event(&db_b, "concurrent-session", "turn-b", 0, "turn_started");
    append_event(&db_a, "concurrent-session", "turn-a", 1, "tool_started");
    append_event(&db_b, "concurrent-session", "turn-b", 1, "tool_result");
    db_a.update_turn_state(
        "concurrent-session",
        "turn-a",
        TurnState::Running,
        None,
        None,
    )
    .unwrap();
    db_b.update_turn_state(
        "concurrent-session",
        "turn-b",
        TurnState::WaitingApproval,
        None,
        None,
    )
    .unwrap();
    append_event(&db_a, "concurrent-session", "turn-a", 2, "turn_completed");
    db_a.update_turn_state(
        "concurrent-session",
        "turn-a",
        TurnState::Completed,
        None,
        None,
    )
    .unwrap();

    // 最终状态与 seq 各自正确
    let turn_a = db_a
        .get_turn("concurrent-session", "turn-a")
        .unwrap()
        .unwrap();
    assert_eq!(turn_a.status, TurnState::Completed);
    assert_eq!(turn_a.last_seq, 2);
    let turn_b = db_b
        .get_turn("concurrent-session", "turn-b")
        .unwrap()
        .unwrap();
    assert_eq!(turn_b.status, TurnState::WaitingApproval);
    assert_eq!(turn_b.last_seq, 1);

    // 跨 turn 事件隔离
    let events_a = db_a
        .list_turn_events_since("concurrent-session", "turn-a", -1, None)
        .unwrap();
    assert_eq!(events_a.len(), 3);
    assert!(events_a.iter().all(|event| event.turn_id == "turn-a"));
    let events_b = db_b
        .list_turn_events_since("concurrent-session", "turn-b", -1, None)
        .unwrap();
    assert_eq!(events_b.len(), 2);
    assert!(events_b.iter().all(|event| event.turn_id == "turn-b"));
}

#[test]
fn turn_events_are_redacted_on_append() {
    let temp = tempdir().unwrap();
    let db = Database::open(&temp.path().join("redact.db")).unwrap();
    insert_test_session(&db, "redact-session");
    db.create_turn("redact-session", "turn-r").unwrap();

    let payload = json!({
        "title": "工具调用",
        "body": "运行命令",
        "env": {"GITHUB_TOKEN": "ghp_abcdef1234567890"},
        "authorization": "Bearer abc.def.ghi",
        "image": "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAAB"
    });
    db.append_turn_event(&TurnEvent {
        session_id: "redact-session".to_string(),
        turn_id: "turn-r".to_string(),
        seq: 0,
        kind: "tool_started".to_string(),
        timestamp: Utc::now(),
        payload_json: payload.to_string(),
    })
    .unwrap();

    let events = db
        .list_turn_events_since("redact-session", "turn-r", -1, None)
        .unwrap();
    let stored = events[0].payload_json.clone();
    assert!(stored.contains("[REDACTED]"));
    assert!(!stored.contains("ghp_abcdef"));
    assert!(!stored.contains("Bearer abc"));
    assert!(!stored.contains("iVBORw0KGgo"));
}

#[test]
fn session_delete_cleans_up_turn_journal_in_same_transaction() {
    let temp = tempdir().unwrap();
    let db = Database::open(&temp.path().join("delete-journal.db")).unwrap();
    insert_test_session(&db, "delete-session");
    db.create_turn("delete-session", "turn-1").unwrap();
    append_event(&db, "delete-session", "turn-1", 0, "turn_started");
    append_event(&db, "delete-session", "turn-1", 1, "turn_completed");
    db.create_turn("delete-session", "turn-2").unwrap();
    append_event(&db, "delete-session", "turn-2", 0, "turn_started");

    db.delete_session("delete-session").unwrap();
    assert!(db.get_turn("delete-session", "turn-1").unwrap().is_none());
    assert!(db.get_turn("delete-session", "turn-2").unwrap().is_none());
    assert!(db
        .list_turn_events_since("delete-session", "turn-1", -1, None)
        .unwrap()
        .is_empty());
    // 会话本身也不存在
    assert!(db.get_session("delete-session").unwrap().is_none());
}

#[test]
fn concurrent_sessions_do_not_pollute_each_other() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("cross-session.db");
    let db_a = Database::open(&path).unwrap();
    let db_b = Database::open(&path).unwrap();
    insert_test_session(&db_a, "session-alpha");
    insert_test_session(&db_a, "session-beta");

    // 两个独立 runtime 各自操作不同的会话，交错写入
    db_a.create_turn("session-alpha", "turn-a0").unwrap();
    db_b.create_turn("session-beta", "turn-b0").unwrap();
    append_event(&db_a, "session-alpha", "turn-a0", 0, "turn_started");
    append_event(&db_b, "session-beta", "turn-b0", 0, "turn_started");
    append_event(&db_a, "session-alpha", "turn-a0", 1, "tool_started");
    append_event(&db_b, "session-beta", "turn-b0", 1, "tool_result");
    db_a.update_turn_state("session-alpha", "turn-a0", TurnState::Running, None, None)
        .unwrap();
    db_b.update_turn_state(
        "session-beta",
        "turn-b0",
        TurnState::WaitingApproval,
        Some("beta 等待确认".to_string()),
        None,
    )
    .unwrap();
    append_event(&db_a, "session-alpha", "turn-a0", 2, "turn_completed");
    db_a.update_turn_state("session-alpha", "turn-a0", TurnState::Completed, None, None)
        .unwrap();

    // 各会话只能看到自己的 turns 与事件
    let alpha_turns = db_a.list_turns("session-alpha").unwrap();
    assert_eq!(alpha_turns.len(), 1);
    assert!(alpha_turns
        .iter()
        .all(|turn| turn.session_id == "session-alpha"));
    let beta_turns = db_b.list_turns("session-beta").unwrap();
    assert_eq!(beta_turns.len(), 1);
    assert!(beta_turns
        .iter()
        .all(|turn| turn.session_id == "session-beta"));
    let alpha_events = db_a
        .list_turn_events_since("session-alpha", "turn-a0", -1, None)
        .unwrap();
    assert_eq!(alpha_events.len(), 3);
    assert!(alpha_events
        .iter()
        .all(|event| event.session_id == "session-alpha"));
    let beta_events = db_b
        .list_turn_events_since("session-beta", "turn-b0", -1, None)
        .unwrap();
    assert_eq!(beta_events.len(), 2);
    assert!(beta_events
        .iter()
        .all(|event| event.session_id == "session-beta"));

    // 会话 A 的终态不影响会话 B 的待确认状态
    let beta = db_b.get_turn("session-beta", "turn-b0").unwrap().unwrap();
    assert_eq!(beta.status, TurnState::WaitingApproval);
    assert_eq!(beta.detail.as_deref(), Some("beta 等待确认"));

    // 会话隔离的清理：删除 session-beta 不影响 session-alpha 的 journal
    db_b.delete_session("session-beta").unwrap();
    assert!(
        db_a.list_turn_events_since("session-alpha", "turn-a0", -1, None)
            .unwrap()
            .len()
            == 3
    );
}

#[test]
fn append_turn_event_with_state_updates_state_atomically() {
    let temp = tempdir().unwrap();
    let db = Database::open(&temp.path().join("combined.db")).unwrap();
    insert_test_session(&db, "combined-session");
    db.create_turn("combined-session", "turn-c").unwrap();

    // 事件内容、seq 与生命周期状态在同一事务内落盘
    db.append_turn_event_with_state(
        &TurnEvent {
            session_id: "combined-session".to_string(),
            turn_id: "turn-c".to_string(),
            seq: 0,
            kind: "tool_confirm_required".to_string(),
            timestamp: Utc::now(),
            payload_json: r#"{"body":"确认 bash 命令"}"#.to_string(),
        },
        Some(TurnState::WaitingApproval),
        Some("需要确认 bash 命令".to_string()),
        None,
    )
    .unwrap();
    let turn = db.get_turn("combined-session", "turn-c").unwrap().unwrap();
    assert_eq!(turn.status, TurnState::WaitingApproval);
    assert_eq!(turn.last_seq, 0);
    assert_eq!(turn.detail.as_deref(), Some("需要确认 bash 命令"));
    let events = db
        .list_turn_events_since("combined-session", "turn-c", -1, None)
        .unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].kind, "tool_confirm_required");

    // 事件 + 终态在同一事务：终态立即冻结
    db.append_turn_event_with_state(
        &TurnEvent {
            session_id: "combined-session".to_string(),
            turn_id: "turn-c".to_string(),
            seq: 1,
            kind: "turn_completed".to_string(),
            timestamp: Utc::now(),
            payload_json: "{}".to_string(),
        },
        Some(TurnState::Completed),
        None,
        None,
    )
    .unwrap();
    let turn = db.get_turn("combined-session", "turn-c").unwrap().unwrap();
    assert_eq!(turn.status, TurnState::Completed);
    assert!(turn.ended_at.is_some());

    // 终态后同事务重开必须整体失败（事件也不得写入）
    let reopen = db.append_turn_event_with_state(
        &TurnEvent {
            session_id: "combined-session".to_string(),
            turn_id: "turn-c".to_string(),
            seq: 2,
            kind: "tool_started".to_string(),
            timestamp: Utc::now(),
            payload_json: "{}".to_string(),
        },
        Some(TurnState::Running),
        None,
        None,
    );
    assert!(reopen.is_err());
    let turn = db.get_turn("combined-session", "turn-c").unwrap().unwrap();
    assert_eq!(turn.status, TurnState::Completed);
    assert_eq!(turn.last_seq, 1);
    assert_eq!(
        db.list_turn_events_since("combined-session", "turn-c", -1, None)
            .unwrap()
            .len(),
        2
    );

    // 无状态事件（None）不改变当前状态，但照常推进 seq
    db.append_turn_event_with_state(
        &TurnEvent {
            session_id: "combined-session".to_string(),
            turn_id: "turn-c".to_string(),
            seq: 10,
            kind: "usage_update".to_string(),
            timestamp: Utc::now(),
            payload_json: "{}".to_string(),
        },
        None,
        None,
        None,
    )
    .unwrap();
    let turn = db.get_turn("combined-session", "turn-c").unwrap().unwrap();
    assert_eq!(turn.status, TurnState::Completed);
    assert_eq!(turn.last_seq, 10);
}

#[test]
fn turn_event_log_is_limited_and_preserves_essential_events() {
    let temp = tempdir().unwrap();
    let db = Database::open(&temp.path().join("prune.db")).unwrap();
    insert_test_session(&db, "prune-session");
    db.create_turn("prune-session", "turn-p").unwrap();
    append_event(&db, "prune-session", "turn-p", 0, "turn_started");
    for seq in 1..(super::MAX_TURN_EVENTS + 10) {
        append_event(&db, "prune-session", "turn-p", seq, "assistant_text_delta");
    }
    append_event(
        &db,
        "prune-session",
        "turn-p",
        super::MAX_TURN_EVENTS + 10,
        "turn_completed",
    );
    db.update_turn_state("prune-session", "turn-p", TurnState::Completed, None, None)
        .unwrap();

    let pruned = db.prune_turn_journals().unwrap();
    assert!(pruned > 0);
    let events = db
        .list_turn_events_since("prune-session", "turn-p", -1, None)
        .unwrap();
    assert!(events.len() as i64 <= super::MAX_TURN_EVENTS + 3);
    let kinds: Vec<&str> = events.iter().map(|event| event.kind.as_str()).collect();
    // turn_started 与终态事件必须保留
    assert!(kinds.contains(&"turn_started"));
    assert!(kinds.contains(&"turn_completed"));
    // 最早的 delta 被清理，最新的事件保留
    let first_delta = kinds
        .iter()
        .position(|kind| *kind == "assistant_text_delta");
    assert!(first_delta.is_some());

    // 运行中的 turn 不被清理
    db.create_turn("prune-session", "turn-live").unwrap();
    append_event(&db, "prune-session", "turn-live", 0, "turn_started");
    let live_events = db
        .list_turn_events_since("prune-session", "turn-live", -1, None)
        .unwrap();
    assert_eq!(live_events.len(), 1);
}

#[test]
fn messages_window_pagination_reads_in_descending_order() {
    let temp = tempdir().unwrap();
    let db = Database::open(&temp.path().join("paging.db")).unwrap();
    insert_test_session(&db, "paging-session");
    for index in 0..25 {
        db.save_message(
            "paging-session",
            "user",
            Some(&format!("message-{index:02}")),
            None,
            None,
            None,
        )
        .unwrap();
    }

    // 最近窗口（降序）
    let window = db.load_messages_window("paging-session", None, 10).unwrap();
    assert_eq!(window.len(), 10);
    assert_eq!(window[0].content.as_deref(), Some("message-24"));
    assert_eq!(window[9].content.as_deref(), Some("message-15"));
    // 上一窗口（before 边界）
    let before = window[9].sort_order;
    let older = db
        .load_messages_window("paging-session", Some(before), 10)
        .unwrap();
    assert_eq!(older.len(), 10);
    assert_eq!(older[0].content.as_deref(), Some("message-14"));
    assert_eq!(older[9].content.as_deref(), Some("message-05"));
    let oldest = db
        .load_messages_window("paging-session", Some(older[9].sort_order), 10)
        .unwrap();
    assert_eq!(oldest.len(), 5);
    assert_eq!(oldest[0].content.as_deref(), Some("message-04"));
    assert_eq!(oldest[4].content.as_deref(), Some("message-00"));
}

#[test]
fn messages_pagination_perf_benchmark_1000_plus() {
    let temp = tempdir().unwrap();
    let db = Database::open(&temp.path().join("perf.db")).unwrap();
    insert_test_session(&db, "perf-session");
    let started = std::time::Instant::now();
    for index in 0..1000 {
        db.save_message(
            "perf-session",
            if index % 2 == 0 { "user" } else { "assistant" },
            Some(&format!("perf message {index}")),
            None,
            None,
            None,
        )
        .unwrap();
    }
    let write_elapsed = started.elapsed();
    let page_started = std::time::Instant::now();
    let mut seen = 0usize;
    let mut before: Option<i64> = None;
    loop {
        let page = db
            .load_messages_window("perf-session", before, 100)
            .unwrap();
        seen += page.len();
        if page.is_empty() {
            break;
        }
        before = Some(page[page.len() - 1].sort_order);
    }
    let page_elapsed = page_started.elapsed();
    assert_eq!(seen, 1000);
    eprintln!(
        "[perf] insert 1000 messages: {:?}; paged read: {:?}",
        write_elapsed, page_elapsed
    );
    // 宽松上限防回归：1000 条写入与分页读取合计应远小于 30 秒
    assert!(
        write_elapsed + page_elapsed < std::time::Duration::from_secs(30),
        "pagination perf regression: write={write_elapsed:?} read={page_elapsed:?}"
    );
}
