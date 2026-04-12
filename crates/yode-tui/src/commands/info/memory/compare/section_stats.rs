use std::collections::BTreeMap;

pub(super) fn build_section_summary(left: &str, right: &str) -> String {
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
        lines.push(format!("  {} blocks: {} -> {}", role, left_count, right_count));
    }

    format!("{}\n", lines.join("\n"))
}

pub(super) fn compare_too_large(left_chars: usize, right_chars: usize, max_total: usize) -> bool {
    left_chars.saturating_add(right_chars) > max_total
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TranscriptSectionStats {
    summary_anchor_lines: usize,
    message_lines: usize,
    role_counts: BTreeMap<String, usize>,
}

fn transcript_section_stats(content: &str) -> TranscriptSectionStats {
    let mut stats = TranscriptSectionStats {
        summary_anchor_lines: 0,
        message_lines: 0,
        role_counts: BTreeMap::new(),
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
                    *stats.role_counts.entry(role.trim().to_string()).or_insert(0) += 1;
                }
            }
            _ => {}
        }
    }

    stats
}
