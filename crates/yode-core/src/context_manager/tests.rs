use super::*;

use yode_llm::types::Message;

#[test]
fn test_model_limits_lookup() {
    let limits = ModelLimits::for_model("claude-sonnet-4-20250514");
    assert_eq!(limits.context_window, 200_000);

    let limits = ModelLimits::for_model("gpt-4o");
    assert_eq!(limits.context_window, 128_000);

    let limits = ModelLimits::for_model("unknown-model");
    assert_eq!(limits.context_window, 128_000);
}

#[test]
fn test_should_compress() {
    let mut cm = ContextManager::new("claude-sonnet-4");
    let msgs = vec![Message::user("hello")];
    assert!(!cm.should_compress(100_000, &msgs));
    assert!(cm.should_compress(160_000, &msgs));
    assert_eq!(cm.last_known_prompt_tokens, Some(160_000));
}

#[test]
fn test_compress_few_messages_noop() {
    let cm = ContextManager::new("claude-sonnet-4");
    let mut messages = vec![
        Message::system("system"),
        Message::user("hello"),
        Message::assistant("hi"),
    ];
    let removed = cm.compress(&mut messages);
    assert_eq!(removed, 0);
    assert_eq!(messages.len(), 3);
}

#[test]
fn test_compress_truncates_tool_results() {
    let cm = ContextManager::new("claude-sonnet-4");
    let long_content = "x".repeat(1000);
    let mut messages = vec![
        Message::system("system"),
        Message::user("q1"),
        Message::assistant("a1"),
        Message::tool_result("tc1", &long_content),
        Message::user("q2"),
        Message::assistant("a2"),
        Message::tool_result("tc2", &long_content),
        Message::user("q3"),
        Message::assistant("a3"),
        Message::user("q4"),
        Message::assistant("a4"),
        Message::user("q5"),
        Message::assistant("a5"),
        Message::user("q6"),
        Message::assistant("a6"),
    ];
    let report = cm.compress_with_report(&mut messages);
    assert_eq!(report.tool_results_truncated, 2);
    if let Some(ref content) = messages[3].content {
        assert!(content.len() < 1000);
        assert!(content.contains("[compressed]"));
    }
}

#[test]
fn test_message_priority() {
    assert_eq!(
        super::runtime::message_priority(&Message::system("sys")),
        99
    );
    assert_eq!(super::runtime::message_priority(&Message::user("hi")), 1);
    assert_eq!(
        super::runtime::message_priority(&Message::assistant("ok")),
        1
    );
    assert_eq!(
        super::runtime::message_priority(&Message::tool_result("id", "res")),
        2
    );
    assert_eq!(
        super::runtime::message_priority(&Message::system("[Context summary] previous turns")),
        2
    );
}

#[test]
fn test_estimate_tokens_without_cache() {
    let cm = ContextManager::new("claude-sonnet-4");
    let messages = vec![Message::user(&"x".repeat(400))];
    assert_eq!(cm.estimate_tokens(&messages), 100);
}

#[test]
fn test_estimate_tokens_with_cache_scales() {
    let mut cm = ContextManager::new("claude-sonnet-4");
    let baseline = vec![Message::user(&"x".repeat(1000))];
    cm.should_compress(10_000, &baseline);

    let messages = vec![Message::user(&"x".repeat(1000))];
    let est = cm.estimate_tokens(&messages);
    assert_eq!(est, 10_000);

    let messages = vec![Message::user(&"x".repeat(500))];
    let est = cm.estimate_tokens(&messages);
    assert_eq!(est, 5_000);

    let messages = vec![Message::user(&"x".repeat(2000))];
    let est = cm.estimate_tokens(&messages);
    assert_eq!(est, 20_000);
}

#[test]
fn test_message_estimated_char_count_seeds_cache() {
    let mut cm = ContextManager::new("claude-sonnet-4");
    let mut message = Message::assistant_with_reasoning(
        Some("answer".to_string()),
        Some("reason".to_string()),
    );
    message.tool_calls.push(yode_llm::types::ToolCall {
        id: "tc1".to_string(),
        name: "read_file".to_string(),
        arguments: "{\"file_path\":\"src/main.rs\"}".to_string(),
    });

    cm.should_compress(1234, &[message.clone()]);
    assert_eq!(cm.last_known_char_count, Some(message.estimated_char_count()));
}

#[test]
fn test_calibration_token_estimate_falls_back_without_cache() {
    assert_eq!(super::runtime::calibration_token_estimate(400, None, None), 100);
    assert_eq!(
        super::runtime::calibration_token_estimate(400, Some(1000), Some(200)),
        2000
    );
}

#[test]
fn test_context_summary_lines_include_tool_activity() {
    let mut tool_usage = std::collections::BTreeMap::new();
    tool_usage.insert("read_file".to_string(), 2);
    let lines = super::runtime::context_summary_lines(
        5,
        &["goal".to_string()],
        &["finding".to_string()],
        &tool_usage,
        1,
        2,
        Some("/tmp/turn.json"),
    );
    assert!(lines.iter().any(|line| line.contains("Earlier tool activity")));
    assert!(lines.iter().any(|line| line.contains("Tool results compacted")));
    assert!(lines.iter().any(|line| line.contains("Turn artifact: /tmp/turn.json")));
}

#[test]
fn test_compress_removes_low_priority_first() {
    let cm = ContextManager::new("gpt-3.5");
    let big = "x".repeat(15_000);
    let mut messages = vec![
        Message::system("system"),
        Message::user(&big),
        Message::tool_result("t1", &big),
        Message::assistant(&big),
        Message::user(&big),
        Message::assistant(&big),
        Message::user(&big),
        Message::assistant(&big),
        Message::user("recent1"),
        Message::assistant("recent2"),
        Message::user("recent3"),
        Message::assistant("recent4"),
        Message::user("recent5"),
        Message::assistant("recent6"),
    ];
    let original_len = messages.len();
    let report = cm.compress_with_report(&mut messages);
    assert!(report.removed > 0);
    assert!(messages.len() <= original_len);
    assert!(messages.iter().any(super::runtime::is_context_summary));
    assert!(matches!(messages[0].role, Role::System));
}

#[test]
fn test_compression_stress_realistic_conversation() {
    let mut cm = ContextManager::new("gpt-3.5");

    let mut messages = vec![Message::system(&"You are a coding assistant. ".repeat(100))];

    for i in 0..15 {
        messages.push(Message::user(&format!(
            "Please read file{}.rs and explain it",
            i
        )));

        let mut assistant = Message::assistant(&format!("Let me read file{}.rs for you.", i));
        assistant.tool_calls.push(yode_llm::types::ToolCall {
            id: format!("tc_{}", i),
            name: "read_file".to_string(),
            arguments: format!("{{\"path\": \"file{}.rs\"}}", i),
        });
        messages.push(assistant);

        messages.push(Message::tool_result(
            &format!("tc_{}", i),
            &format!(
                "// file{}.rs\n{}",
                i,
                "fn example() { /* lots of code here */ }\n".repeat(100)
            ),
        ));

        messages.push(Message::assistant(&format!(
            "File{}.rs contains an example function that {}. The implementation is straightforward.",
            i, "x".repeat(200)
        )));
    }

    let original_len = messages.len();
    assert!(original_len > 50);

    let total_chars: usize = messages
        .iter()
        .map(|m| {
            m.content.as_ref().map(|c| c.len()).unwrap_or(0)
                + m.tool_calls
                    .iter()
                    .map(|tc| tc.arguments.len() + tc.name.len())
                    .sum::<usize>()
        })
        .sum();
    let fake_prompt_tokens = (total_chars / 2) as u32;
    cm.should_compress(fake_prompt_tokens, &messages);

    let report = cm.compress_with_report(&mut messages);

    assert!(report.removed > 0);
    assert!(matches!(messages[0].role, Role::System));

    let last_msgs: Vec<_> = messages.iter().rev().take(PRESERVE_RECENT).collect();
    assert_eq!(last_msgs.len(), PRESERVE_RECENT);

    let truncated_count = messages
        .iter()
        .filter(|m| {
            matches!(m.role, Role::Tool)
                && m.content
                    .as_ref()
                    .map(|c| c.contains("[compressed]"))
                    .unwrap_or(false)
        })
        .count();
    assert!(truncated_count > 0 || report.removed > 5);
    assert!(messages.iter().any(super::runtime::is_context_summary));
}

#[test]
fn test_compression_preserves_message_integrity() {
    let cm = ContextManager::new("gpt-3.5");
    let mut messages = vec![
        Message::system("SYSTEM_MARKER"),
        Message::user(&"u".repeat(10_000)),
        Message::assistant(&"a".repeat(10_000)),
        Message::tool_result("t1", &"r".repeat(10_000)),
        Message::user(&"u2".repeat(5_000)),
        Message::assistant(&"a2".repeat(5_000)),
        Message::user(&"u3".repeat(5_000)),
        Message::assistant(&"a3".repeat(5_000)),
        Message::user("final_user"),
        Message::assistant("final_assistant"),
        Message::user("last1"),
        Message::assistant("last2"),
        Message::user("last3"),
        Message::assistant("last4"),
    ];

    let report = cm.compress_with_report(&mut messages);
    assert!(report.removed > 0 || report.tool_results_truncated > 0);
    assert_eq!(messages[0].content.as_deref(), Some("SYSTEM_MARKER"));

    for msg in &messages {
        assert!(matches!(
            msg.role,
            Role::System | Role::User | Role::Assistant | Role::Tool
        ));
    }

    for msg in &messages {
        if !matches!(msg.role, Role::Tool) {
            assert!(msg.content.is_some());
        }
    }
}

#[test]
fn test_compression_inserts_summary_anchor() {
    let mut cm = ContextManager::new("gpt-3.5");
    let big = "y".repeat(18_000);
    let mut messages = vec![
        Message::system("system"),
        Message::user("Investigate the updater failure on macOS"),
        Message::assistant("I will inspect updater extraction and retry handling."),
        Message::tool_result("tc1", &big),
        Message::assistant("The archive unpack fails under sandboxed temp directories."),
        Message::user(&big),
        Message::assistant("I will compact the earlier findings."),
        Message::user("recent1"),
        Message::assistant("recent2"),
        Message::user("recent3"),
        Message::assistant("recent4"),
        Message::user("recent5"),
        Message::assistant("recent6"),
    ];

    let total_chars: usize = messages
        .iter()
        .map(|m| {
            m.content.as_ref().map(|c| c.len()).unwrap_or(0)
                + m.tool_calls
                    .iter()
                    .map(|tc| tc.arguments.len() + tc.name.len())
                    .sum::<usize>()
        })
        .sum();
    cm.should_compress(total_chars as u32, &messages);

    let report = cm.compress_with_report(&mut messages);
    assert!(report.removed > 0);
    let summary = report.summary.expect("summary anchor should be inserted");
    assert!(summary.starts_with(CONTEXT_SUMMARY_PREFIX));
    assert!(messages.iter().any(super::runtime::is_context_summary));
}

#[test]
fn test_compression_summary_can_include_turn_artifact_link() {
    let mut cm = ContextManager::new("gpt-3.5");
    let big = "z".repeat(18_000);
    let mut messages = vec![
        Message::system("system"),
        Message::user("Investigate the failing startup profile export"),
        Message::assistant("I will inspect the latest startup bundle."),
        Message::tool_result("tc1", &big),
        Message::user(&big),
        Message::assistant("I will compact the earlier findings."),
        Message::user("recent1"),
        Message::assistant("recent2"),
        Message::user("recent3"),
        Message::assistant("recent4"),
        Message::user("recent5"),
        Message::assistant("recent6"),
    ];

    let total_chars: usize = messages
        .iter()
        .map(|m| {
            m.content.as_ref().map(|c| c.len()).unwrap_or(0)
                + m.tool_calls
                    .iter()
                    .map(|tc| tc.arguments.len() + tc.name.len())
                    .sum::<usize>()
        })
        .sum();
    cm.should_compress(total_chars as u32, &messages);

    let report = cm.compress_with_turn_artifact(&mut messages, Some("/tmp/latest-turn.json"));
    let summary = report.summary.expect("summary anchor should be inserted");
    assert!(summary.contains("Turn artifact: /tmp/latest-turn.json"));
}

#[test]
fn test_exceeds_threshold_estimate_uses_cached_ratio() {
    let mut cm = ContextManager::new("claude-sonnet-4");
    let baseline = vec![Message::user(&"x".repeat(1000))];
    cm.should_compress(160_000, &baseline);

    assert!(cm.exceeds_threshold_estimate(&baseline));

    let smaller = vec![Message::user(&"x".repeat(100))];
    assert!(!cm.exceeds_threshold_estimate(&smaller));
}
