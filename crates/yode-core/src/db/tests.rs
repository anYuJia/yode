use chrono::Utc;
use serde_json::json;
use tempfile::tempdir;
use yode_llm::types::{ImageData, Message};

use super::{Database, SessionArtifacts};
use crate::session::Session;

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
