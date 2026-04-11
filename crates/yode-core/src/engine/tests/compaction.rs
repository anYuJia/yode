use super::*;

#[tokio::test]
async fn test_autocompact_circuit_breaker_trips_after_repeated_failures() {
    let mut engine = make_engine(vec![], vec![]);
    engine.messages = vec![
        Message::system("system"),
        Message::user("u1"),
        Message::assistant("a1"),
        Message::user("u2"),
        Message::assistant("a2"),
        Message::user("u3"),
        Message::assistant("a3"),
    ];
    engine.current_query_source = QuerySource::User;

    let (tx, mut rx) = mpsc::unbounded_channel();
    for _ in 0..MAX_CONSECUTIVE_COMPACTION_FAILURES {
        engine.maybe_compact_context(160_000, &tx).await;
    }

    assert!(engine.autocompact_disabled);
    assert_eq!(
        engine.compaction_failures,
        MAX_CONSECUTIVE_COMPACTION_FAILURES
    );
    assert!(engine.messages.iter().any(|msg| {
        msg.content
            .as_deref()
            .unwrap_or_default()
            .contains("Auto-compact disabled")
    }));
    let runtime = engine.runtime_state();
    assert_eq!(
        runtime.last_compaction_breaker_reason.as_deref(),
        Some("compression made no changes")
    );
    assert!(matches!(rx.try_recv(), Ok(EngineEvent::Error(_))));
}

#[test]
fn test_recovery_artifact_written_on_state_transition() {
    let mut engine = make_engine(vec![], vec![]);
    engine.last_failed_signature = Some("bash:{\"command\":\"cargo test\"}".to_string());
    engine.error_buckets.insert(ToolErrorType::Validation, 2);
    engine.update_recovery_state();

    assert_eq!(engine.recovery_state, RecoveryState::SingleStepMode);
    let artifact = engine
        .last_recovery_artifact_path
        .as_ref()
        .expect("recovery artifact should exist");
    let content = std::fs::read_to_string(artifact).unwrap();
    assert!(content.contains("SingleStepMode"));
    assert!(content.contains("Breadcrumbs"));
}

#[tokio::test]
async fn test_compact_query_source_skips_autocompact() {
    let mut engine = make_engine(vec![], vec![]);
    let big = "x".repeat(18_000);
    engine.messages = vec![
        Message::system("system"),
        Message::user(&big),
        Message::assistant(&big),
        Message::tool_result("tc1", &big),
        Message::user(&big),
        Message::assistant(&big),
        Message::user("recent1"),
        Message::assistant("recent2"),
        Message::user("recent3"),
        Message::assistant("recent4"),
        Message::user("recent5"),
        Message::assistant("recent6"),
    ];
    engine.current_query_source = QuerySource::Compact;

    let before_len = engine.messages.len();
    let (tx, _rx) = mpsc::unbounded_channel();
    engine.maybe_compact_context(160_000, &tx).await;

    assert_eq!(engine.messages.len(), before_len);
    assert_eq!(engine.compaction_failures, 0);
    assert!(!engine.messages.iter().any(|msg| {
        msg.content
            .as_deref()
            .unwrap_or_default()
            .starts_with("[Context summary]")
    }));
}

#[tokio::test]
async fn test_force_compact_ignores_auto_compact_guard() {
    let mut engine = make_engine(vec![], vec![]);
    let big = "x".repeat(18_000);
    engine.messages = vec![
        Message::system("system"),
        Message::user(&big),
        Message::assistant(&big),
        Message::tool_result("tc1", &big),
        Message::user(&big),
        Message::assistant(&big),
        Message::user("recent1"),
        Message::assistant("recent2"),
        Message::user("recent3"),
        Message::assistant("recent4"),
        Message::user("recent5"),
        Message::assistant("recent6"),
    ];
    engine.current_query_source = QuerySource::Compact;
    engine.autocompact_disabled = true;

    let (tx, _rx) = mpsc::unbounded_channel();
    let changed = engine.force_compact(tx).await;

    assert!(changed);
    let runtime = engine.runtime_state();
    assert_eq!(runtime.total_compactions, 1);
    assert_eq!(runtime.manual_compactions, 1);
    assert!(
        engine.messages.len() < 12
            || engine.messages.iter().any(|msg| {
                msg.content
                    .as_deref()
                    .unwrap_or_default()
                    .contains("[compressed]")
            })
    );
}
