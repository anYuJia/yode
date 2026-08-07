//! 可重复的长会话基准快照（PERF-001）。
//!
//! 该测试输出 `# Long Session Benchmark Snapshot` 开头、可被
//! `scripts/benchmark-snapshot.sh` 解析的确定性报告：构造一个 2_000 条消息
//! 的长会话，测量数据库加载与内存占用，供 CI 与本地对比回归。

use std::time::Instant;

use crate::db::Database;
use crate::session::Session;
use chrono::Utc;

const LONG_SESSION_MESSAGES: usize = 2_000;

#[test]
fn print_long_session_benchmark_snapshot() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("benchmark.db");
    let db = Database::open(&db_path).unwrap();
    db.create_session(&Session {
        id: "benchmark-session".to_string(),
        name: None,
        project_root: None,
        provider: "mock".to_string(),
        model: "mock-model".to_string(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    })
    .unwrap();

    let payload = "x".repeat(200);
    let write_start = Instant::now();
    for index in 0..LONG_SESSION_MESSAGES {
        db.save_message(
            "benchmark-session",
            if index % 2 == 0 { "user" } else { "assistant" },
            Some(&payload),
            None,
            None,
            None,
        )
        .unwrap();
    }
    let write_elapsed = write_start.elapsed().as_millis();

    let load_start = Instant::now();
    let messages = db.load_messages("benchmark-session").unwrap();
    let load_elapsed = load_start.elapsed().as_millis();
    let loaded_bytes = messages
        .iter()
        .map(|message| message.content.as_deref().map_or(0, str::len))
        .sum::<usize>();

    let estimated_memory_bytes = messages
        .iter()
        .map(|message| {
            message.content.as_deref().map_or(0, str::len)
                + message.reasoning.as_deref().map_or(0, str::len)
                + message.tool_calls_json.as_deref().map_or(0, str::len)
                + message.images_json.as_deref().map_or(0, str::len)
        })
        .sum::<usize>();

    println!("# Long Session Benchmark Snapshot");
    println!("- messages: {LONG_SESSION_MESSAGES}");
    println!("- db_write_ms: {write_elapsed}");
    println!("- db_load_ms: {load_elapsed}");
    println!("- loaded_content_bytes: {loaded_bytes}");
    println!("- estimated_message_memory_bytes: {estimated_memory_bytes}");
    println!(
        "- sampled_at: {}",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
    );

    assert_eq!(messages.len(), LONG_SESSION_MESSAGES);
}
