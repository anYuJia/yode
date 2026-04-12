use yode_tools::builtin::review_common::review_output_has_findings;

pub(crate) fn compact_review_status_badge(content: &str) -> &'static str {
    let body = extract_review_result_body(content).unwrap_or(content);
    if body.trim().is_empty() {
        "unk"
    } else if review_output_has_findings(body) {
        "find"
    } else {
        "clean"
    }
}

pub(crate) fn review_summary_pane(path: &std::path::Path, content: &str) -> String {
    let body = extract_review_result_body(content).unwrap_or(content);
    let preview = body
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with("```"))
        .take(4)
        .collect::<Vec<_>>()
        .join(" | ");
    format!(
        "Review summary pane:\n  Path:   {}\n  Badge:  {}\n  Preview: {}",
        path.display(),
        compact_review_status_badge(content),
        if preview.is_empty() { "none" } else { &preview }
    )
}

pub(crate) fn fold_review_preview_for_workspace(content: &str) -> String {
    const HEAD: usize = 60;
    const TAIL: usize = 20;
    const MAX_LINES: usize = 100;

    let lines = content.lines().collect::<Vec<_>>();
    if lines.len() <= MAX_LINES {
        return content.to_string();
    }

    let mut output = lines[..HEAD.min(lines.len())].join("\n");
    let omitted = lines.len().saturating_sub(HEAD + TAIL);
    output.push_str(&format!(
        "\n\n... [review workspace preview folded: {} middle lines omitted] ...\n\n",
        omitted
    ));
    output.push_str(&lines[lines.len() - TAIL.min(lines.len())..].join("\n"));
    output
}

pub(crate) fn extract_review_result_body(content: &str) -> Option<&str> {
    let start = content.find("```text\n")?;
    let body_start = start + "```text\n".len();
    let end = content[body_start..].find("\n```")?;
    Some(&content[body_start..body_start + end])
}

#[cfg(test)]
mod tests {
    use super::{
        compact_review_status_badge, extract_review_result_body, fold_review_preview_for_workspace,
        review_summary_pane,
    };

    #[test]
    fn compact_badge_prefers_short_labels() {
        assert_eq!(compact_review_status_badge("```text\nNo issues found.\n```"), "clean");
        assert_eq!(compact_review_status_badge("```text\n1. Missing test\n```"), "find");
    }

    #[test]
    fn summary_pane_includes_path_and_preview() {
        let path = std::path::Path::new("/tmp/review.md");
        let pane = review_summary_pane(path, "```text\nNo issues found.\nResidual risk: none.\n```");
        assert!(pane.contains("/tmp/review.md"));
        assert!(pane.contains("Preview"));
    }

    #[test]
    fn fold_preview_limits_long_review_output() {
        let content = (0..120)
            .map(|idx| format!("line {}", idx))
            .collect::<Vec<_>>()
            .join("\n");
        let folded = fold_review_preview_for_workspace(&content);
        assert!(folded.contains("review workspace preview folded"));
    }

    #[test]
    fn extract_review_result_reads_text_block() {
        assert_eq!(
            extract_review_result_body("before\n```text\nhello\n```\nafter"),
            Some("hello")
        );
    }
}
