use super::*;

#[tokio::test]
async fn test_handle_interrupted_stream_persists_partial_message() {
    let mut engine = make_engine(vec![], vec![]);
    let (tx, mut rx) = mpsc::unbounded_channel();
    let buffers = super::super::streaming_turn_runtime::StreamTurnBuffers {
        full_text: "partial text".to_string(),
        full_reasoning: "partial reasoning".to_string(),
        tool_calls: Vec::new(),
        final_response: None,
    };

    let handled = engine
        .handle_interrupted_stream(false, true, &buffers, &tx)
        .await;

    assert!(handled);
    assert!(engine
        .messages
        .iter()
        .any(|message| message.content.as_deref() == Some("partial text")));
    assert!(matches!(rx.recv().await, Some(EngineEvent::TextComplete(_))));
}

#[tokio::test]
async fn test_handle_interrupted_stream_ignores_normal_flow() {
    let mut engine = make_engine(vec![], vec![]);
    let (tx, _rx) = mpsc::unbounded_channel();
    let buffers = super::super::streaming_turn_runtime::StreamTurnBuffers::default();

    let handled = engine
        .handle_interrupted_stream(false, false, &buffers, &tx)
        .await;

    assert!(!handled);
    assert_eq!(engine.messages.len(), 1);
}
