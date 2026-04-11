use super::*;

#[test]
fn parse_compare_args_accepts_two_values_and_flags() {
    assert_eq!(
        parse_compare_args("compare 1 2"),
        Some(CompareArgs {
            left_target: "1".to_string(),
            right_target: "2".to_string(),
            options: CompareOptions::default(),
        })
    );
    assert_eq!(
        parse_compare_args("compare latest sample.md --hunks 2 --lines 20"),
        Some(CompareArgs {
            left_target: "latest".to_string(),
            right_target: "sample.md".to_string(),
            options: CompareOptions {
                diff_enabled: true,
                max_hunks: 2,
                max_lines: 20,
            },
        })
    );
    assert_eq!(
        parse_compare_args("compare latest latest-1 --no-diff"),
        Some(CompareArgs {
            left_target: "latest".to_string(),
            right_target: "latest-1".to_string(),
            options: CompareOptions {
                diff_enabled: false,
                ..CompareOptions::default()
            },
        })
    );
    assert_eq!(parse_compare_args("compare 1"), None);
    assert_eq!(parse_compare_args("list compare 1 2"), None);
}

#[test]
fn build_transcript_compare_output_highlights_differences() {
    let dir = std::env::temp_dir().join(format!(
        "yode-memory-command-compare-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let left_path = dir.join("left.md");
    let right_path = dir.join("right.md");
    let left = "# Compaction Transcript\n\n- Session: abc\n- Mode: auto\n- Timestamp: 2026-01-01 10:00:00\n- Removed messages: 7\n- Tool results truncated: 1\n- Failed tool results: 1\n\n## Summary Anchor\n\n```text\nLeft summary\n```\n\n## Messages\n\n### User\n\n```text\nhello\n```\n";
    let right = "# Compaction Transcript\n\n- Session: abc\n- Mode: manual\n- Timestamp: 2026-01-01 11:00:00\n- Removed messages: 3\n- Tool results truncated: 0\n- Failed tool results: 0\n\n## Summary Anchor\n\n```text\nRight summary\n```\n\n## Messages\n\n### User\n\n```text\nhello\n```\n";
    std::fs::write(&left_path, left).unwrap();
    std::fs::write(&right_path, right).unwrap();

    let output = build_transcript_compare_output(
        &left_path,
        left,
        &right_path,
        right,
        &CompareOptions::default(),
    );
    assert!(output.contains("Status: different"));
    assert!(output.contains("Mode               auto -> manual"));
    assert!(output.contains("Failed tool results 1 -> 0"));
    assert!(output.contains("A: Left summary"));
    assert!(output.contains("B: Right summary"));
    assert!(output.contains("Section summary:"));
    assert!(output.contains("User blocks:"));
    assert!(output.contains("Content diff:"));
    assert!(output.contains("Changed lines:"));
    assert!(output.contains("Hunk 1"));
    assert!(output.contains("First difference:"));

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn build_transcript_compare_output_respects_no_diff_flag() {
    let dir = std::env::temp_dir().join(format!(
        "yode-memory-command-compare-nodiff-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let left_path = dir.join("left.md");
    let right_path = dir.join("right.md");
    let left = "# Compaction Transcript\n\n## Summary Anchor\n\n```text\nLeft summary\n```\n\n## Messages\n\n### Assistant\n\n```text\nhello\n```\n";
    let right = "# Compaction Transcript\n\n## Summary Anchor\n\n```text\nRight summary\n```\n\n## Messages\n\n### Assistant\n\n```text\nworld\n```\n";
    std::fs::write(&left_path, left).unwrap();
    std::fs::write(&right_path, right).unwrap();

    let output = build_transcript_compare_output(
        &left_path,
        left,
        &right_path,
        right,
        &CompareOptions {
            diff_enabled: false,
            ..CompareOptions::default()
        },
    );
    assert!(output.contains("Content diff:\n  disabled by flag"));

    std::fs::remove_dir_all(&dir).ok();
}
