use std::path::Path;

use similar::{ChangeTag, DiffOp, TextDiff};

use super::transcripts::{extract_summary_preview, read_transcript_metadata};
use super::MAX_COMPARE_CONTENT_CHARS;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CompareArgs {
    pub left_target: String,
    pub right_target: String,
    pub options: CompareOptions,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CompareOptions {
    pub diff_enabled: bool,
    pub max_hunks: usize,
    pub max_lines: usize,
}

impl Default for CompareOptions {
    fn default() -> Self {
        Self {
            diff_enabled: true,
            max_hunks: 3,
            max_lines: 60,
        }
    }
}

pub(super) fn parse_compare_args(args: &str) -> Option<CompareArgs> {
    let rest = args.strip_prefix("compare ")?;
    let tokens = rest.split_whitespace().collect::<Vec<_>>();
    if tokens.len() < 2 {
        return None;
    }
    let mut compare = CompareArgs {
        left_target: tokens[0].to_string(),
        right_target: tokens[1].to_string(),
        options: CompareOptions::default(),
    };

    let mut idx = 2usize;
    while idx < tokens.len() {
        match tokens[idx] {
            "--no-diff" => {
                compare.options.diff_enabled = false;
                idx += 1;
            }
            "--hunks" => {
                let value = tokens.get(idx + 1)?.parse::<usize>().ok()?;
                if value == 0 {
                    return None;
                }
                compare.options.max_hunks = value;
                idx += 2;
            }
            "--lines" => {
                let value = tokens.get(idx + 1)?.parse::<usize>().ok()?;
                if value == 0 {
                    return None;
                }
                compare.options.max_lines = value;
                idx += 2;
            }
            _ => return None,
        }
    }

    Some(compare)
}

pub(super) fn build_transcript_compare_output(
    left_path: &Path,
    left_content: &str,
    right_path: &Path,
    right_content: &str,
    options: &CompareOptions,
) -> String {
    let left_meta = read_transcript_metadata(left_path).unwrap_or_default();
    let right_meta = read_transcript_metadata(right_path).unwrap_or_default();
    let left_summary = extract_summary_preview(left_content).unwrap_or_else(|| "none".to_string());
    let right_summary =
        extract_summary_preview(right_content).unwrap_or_else(|| "none".to_string());
    let left_lines = left_content.lines().count();
    let right_lines = right_content.lines().count();
    let left_chars = left_content.chars().count();
    let right_chars = right_content.chars().count();
    let left_messages = count_transcript_messages(left_content);
    let right_messages = count_transcript_messages(right_content);
    let identical = left_content == right_content;
    let compare_too_large = left_chars.saturating_add(right_chars) > MAX_COMPARE_CONTENT_CHARS;

    let mut output = String::new();
    output.push_str("Transcript comparison\n");
    output.push_str(&format!("A: {}\n", left_path.display()));
    output.push_str(&format!("B: {}\n", right_path.display()));
    output.push_str(&format!(
        "Status: {}\n\n",
        if identical { "identical" } else { "different" }
    ));
    output.push_str(&format!(
        "Diff window: hunks={} lines={}\n\n",
        options.max_hunks, options.max_lines
    ));

    output.push_str("Metadata:\n");
    output.push_str(&format_compare_field(
        "Mode",
        left_meta.mode.as_deref().unwrap_or("unknown"),
        right_meta.mode.as_deref().unwrap_or("unknown"),
    ));
    output.push_str(&format_compare_field(
        "Timestamp",
        left_meta.timestamp.as_deref().unwrap_or("unknown"),
        right_meta.timestamp.as_deref().unwrap_or("unknown"),
    ));
    output.push_str(&format_compare_field(
        "Removed",
        &left_meta.removed.unwrap_or(0).to_string(),
        &right_meta.removed.unwrap_or(0).to_string(),
    ));
    output.push_str(&format_compare_field(
        "Truncated",
        &left_meta.truncated.unwrap_or(0).to_string(),
        &right_meta.truncated.unwrap_or(0).to_string(),
    ));
    output.push_str(&format_compare_field(
        "Failed tool results",
        &left_meta.failed_tool_results.unwrap_or(0).to_string(),
        &right_meta.failed_tool_results.unwrap_or(0).to_string(),
    ));
    output.push_str(&format_compare_field(
        "Session memory path",
        left_meta.session_memory_path.as_deref().unwrap_or("none"),
        right_meta.session_memory_path.as_deref().unwrap_or("none"),
    ));
    output.push_str(&format_compare_field(
        "Files read",
        left_meta.files_read_summary.as_deref().unwrap_or("none"),
        right_meta.files_read_summary.as_deref().unwrap_or("none"),
    ));
    output.push_str(&format_compare_field(
        "Files modified",
        left_meta.files_modified_summary.as_deref().unwrap_or("none"),
        right_meta.files_modified_summary.as_deref().unwrap_or("none"),
    ));
    output.push_str(&format_compare_field(
        "Summary anchor",
        if left_meta.has_summary { "yes" } else { "no" },
        if right_meta.has_summary { "yes" } else { "no" },
    ));
    output.push_str(&format_compare_field(
        "Message sections",
        &left_messages.to_string(),
        &right_messages.to_string(),
    ));
    output.push_str(&format_compare_field(
        "Lines",
        &left_lines.to_string(),
        &right_lines.to_string(),
    ));
    output.push_str(&format_compare_field(
        "Chars",
        &left_chars.to_string(),
        &right_chars.to_string(),
    ));

    output.push_str("\nSummary preview:\n");
    output.push_str(&format!("  A: {}\n", left_summary));
    output.push_str(&format!("  B: {}\n", right_summary));

    output.push_str("\nSection summary:\n");
    output.push_str(&build_section_summary(left_content, right_content));

    if compare_too_large {
        output.push_str("\nContent diff:\n");
        output.push_str(&format!(
            "  skipped: content too large for interactive diff preview ({} chars > {}). Use --no-diff, narrower targets, or inspect one transcript directly.\n",
            left_chars + right_chars,
            MAX_COMPARE_CONTENT_CHARS
        ));
    } else if options.diff_enabled {
        if let Some(diff_preview) = build_diff_preview(left_content, right_content, options) {
            output.push_str("\nContent diff:\n");
            output.push_str(&diff_preview);
        }
    } else {
        output.push_str("\nContent diff:\n");
        output.push_str("  disabled by flag\n");
    }

    if let Some((line_no, left_line, right_line)) = first_difference(left_content, right_content) {
        output.push_str("\nFirst difference:\n");
        output.push_str(&format!("  Line: {}\n", line_no));
        output.push_str(&format!(
            "  A: {}\n",
            summarize_compare_line(left_line.unwrap_or("<no line>"))
        ));
        output.push_str(&format!(
            "  B: {}\n",
            summarize_compare_line(right_line.unwrap_or("<no line>"))
        ));
    }

    output
}

fn build_diff_preview(left: &str, right: &str, options: &CompareOptions) -> Option<String> {
    let diff = TextDiff::from_lines(left, right);
    let groups = diff.grouped_ops(2);

    let mut added = 0usize;
    let mut removed = 0usize;
    for op in diff.ops() {
        for change in diff.iter_changes(op) {
            match change.tag() {
                ChangeTag::Insert => added += 1,
                ChangeTag::Delete => removed += 1,
                ChangeTag::Equal => {}
            }
        }
    }

    if added == 0 && removed == 0 {
        return None;
    }

    let mut output = String::new();
    output.push_str(&format!("  Changed lines: +{} / -{}\n", added, removed));

    let mut shown_lines = 0usize;
    for (idx, group) in groups.iter().take(options.max_hunks).enumerate() {
        let (old_start, old_count, new_start, new_count) = diff_group_header(group);
        output.push_str(&format!(
            "  Hunk {} @@ -{},{} +{},{} @@\n",
            idx + 1,
            old_start,
            old_count,
            new_start,
            new_count
        ));

        for op in group {
            for change in diff.iter_changes(op) {
                let prefix = match change.tag() {
                    ChangeTag::Delete => '-',
                    ChangeTag::Insert => '+',
                    ChangeTag::Equal => ' ',
                };
                output.push_str(&format!(
                    "    {}{}\n",
                    prefix,
                    summarize_compare_line(change.to_string().trim_end_matches('\n'))
                ));
                shown_lines += 1;
                if shown_lines >= options.max_lines {
                    output.push_str(
                        "    ... diff preview truncated ... use --lines N or --hunks N to expand ...\n",
                    );
                    return Some(output);
                }
            }
        }
    }

    if groups.len() > options.max_hunks {
        output.push_str(&format!(
            "  ... {} more hunks omitted ... use --hunks N to expand ...\n",
            groups.len() - options.max_hunks
        ));
    }

    Some(output)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TranscriptSectionStats {
    summary_anchor_lines: usize,
    message_lines: usize,
    role_counts: std::collections::BTreeMap<String, usize>,
}

fn build_section_summary(left: &str, right: &str) -> String {
    let left_stats = transcript_section_stats(left);
    let right_stats = transcript_section_stats(right);

    let mut lines = Vec::new();
    lines.push(format!(
        "  Summary Anchor lines: {} -> {}",
        left_stats.summary_anchor_lines, right_stats.summary_anchor_lines
    ));
    lines.push(format!(
        "  Messages lines:       {} -> {}",
        left_stats.message_lines, right_stats.message_lines
    ));

    let mut roles = left_stats
        .role_counts
        .keys()
        .chain(right_stats.role_counts.keys())
        .cloned()
        .collect::<Vec<_>>();
    roles.sort();
    roles.dedup();
    for role in roles {
        let left_count = left_stats.role_counts.get(&role).copied().unwrap_or(0);
        let right_count = right_stats.role_counts.get(&role).copied().unwrap_or(0);
        lines.push(format!(
            "  {} blocks: {} -> {}",
            role, left_count, right_count
        ));
    }

    format!("{}\n", lines.join("\n"))
}

fn transcript_section_stats(content: &str) -> TranscriptSectionStats {
    let mut stats = TranscriptSectionStats {
        summary_anchor_lines: 0,
        message_lines: 0,
        role_counts: std::collections::BTreeMap::new(),
    };

    let mut current_section: Option<&str> = None;
    for line in content.lines() {
        if let Some(section) = line.strip_prefix("## ") {
            current_section = Some(section.trim());
            continue;
        }

        match current_section {
            Some("Summary Anchor") if !line.trim().is_empty() => {
                stats.summary_anchor_lines += 1;
            }
            Some("Messages") if !line.trim().is_empty() => {
                stats.message_lines += 1;
                if let Some(role) = line.strip_prefix("### ") {
                    *stats
                        .role_counts
                        .entry(role.trim().to_string())
                        .or_insert(0) += 1;
                }
            }
            _ => {}
        }
    }

    stats
}

fn diff_group_header(group: &[DiffOp]) -> (usize, usize, usize, usize) {
    let first = group.first().expect("diff group should not be empty");
    let last = group.last().expect("diff group should not be empty");
    let old = first.old_range().start..last.old_range().end;
    let new = first.new_range().start..last.new_range().end;
    (
        old.start + 1,
        old.end.saturating_sub(old.start),
        new.start + 1,
        new.end.saturating_sub(new.start),
    )
}

fn format_compare_field(label: &str, left: &str, right: &str) -> String {
    if left == right {
        format!("  {:<18} {}\n", label, left)
    } else {
        format!("  {:<18} {} -> {}\n", label, left, right)
    }
}

fn count_transcript_messages(content: &str) -> usize {
    content
        .lines()
        .filter(|line| line.starts_with("### "))
        .count()
}

fn first_difference<'a>(
    left: &'a str,
    right: &'a str,
) -> Option<(usize, Option<&'a str>, Option<&'a str>)> {
    let left_lines = left.lines().collect::<Vec<_>>();
    let right_lines = right.lines().collect::<Vec<_>>();
    let max_len = left_lines.len().max(right_lines.len());

    for idx in 0..max_len {
        let left_line = left_lines.get(idx).copied();
        let right_line = right_lines.get(idx).copied();
        if left_line != right_line {
            return Some((idx + 1, left_line, right_line));
        }
    }

    None
}

fn summarize_compare_line(line: &str) -> String {
    let squashed = line.split_whitespace().collect::<Vec<_>>().join(" ");
    if squashed.chars().count() <= 180 {
        return squashed;
    }

    let truncated = squashed.chars().take(180).collect::<String>();
    format!("{}...", truncated)
}
