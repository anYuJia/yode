use std::path::Path;

pub(crate) fn compare_target_choices(dir: &Path) -> Vec<String> {
    let mut choices = vec!["latest".to_string(), "latest-1".to_string()];
    let mut entries = std::fs::read_dir(dir)
        .ok()
        .into_iter()
        .flat_map(|entries| entries.filter_map(Result::ok))
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("md"))
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| right.file_name().cmp(&left.file_name()));
    choices.extend(
        entries
            .into_iter()
            .take(4)
            .filter_map(|path| path.file_name().and_then(|name| name.to_str()).map(str::to_string)),
    );
    choices
}

pub(crate) fn summary_anchor_jump_summary(content: &str) -> String {
    if let Some(start) = content.find("## Summary Anchor") {
        let preceding_lines = content[..start].lines().count();
        format!("summary anchor present near line {}", preceding_lines + 1)
    } else {
        "summary anchor missing".to_string()
    }
}

pub(crate) fn review_kind_badge(path: &Path) -> &'static str {
    let file_name = path.file_name().and_then(|name| name.to_str()).unwrap_or("");
    if file_name.starts_with("review-") {
        "review"
    } else if file_name.starts_with("verification-") {
        "verify"
    } else if file_name.starts_with("pipeline-") {
        "pipe"
    } else {
        "misc"
    }
}

pub(crate) fn review_metadata_section(path: &Path, content: &str) -> String {
    let lines = content.lines().count();
    let size = content.len();
    format!(
        "Review metadata\n  Path: {}\n  Kind: {}\n  Lines: {}\n  Bytes: {}",
        path.display(),
        review_kind_badge(path),
        lines,
        size
    )
}

pub(crate) fn transcript_review_cross_reference(
    transcript_path: Option<&Path>,
    review_path: Option<&Path>,
) -> Vec<String> {
    let mut targets = vec!["/memory latest".to_string(), "/reviews latest".to_string()];
    if let Some(path) = transcript_path {
        targets.push(format!(
            "/memory {}",
            path.file_name().and_then(|name| name.to_str()).unwrap_or("latest")
        ));
    }
    if let Some(path) = review_path {
        targets.push(format!(
            "/reviews {}",
            path.file_name().and_then(|name| name.to_str()).unwrap_or("latest")
        ));
    }
    targets
}

pub(crate) fn fold_workspace_diff_output(text: &str, max_lines: usize) -> String {
    let lines = text.lines().collect::<Vec<_>>();
    if lines.len() <= max_lines {
        return text.to_string();
    }
    let head = max_lines / 2;
    let tail = max_lines.saturating_sub(head);
    let omitted = lines.len().saturating_sub(head + tail);
    let mut out = lines[..head].join("\n");
    out.push_str(&format!(
        "\n\n... [workspace diff folded: {} middle lines omitted] ...\n\n",
        omitted
    ));
    out.push_str(&lines[lines.len() - tail..].join("\n"));
    out
}

pub(crate) fn residual_risk_banner(content: &str) -> Option<String> {
    content
        .lines()
        .find(|line| line.to_lowercase().contains("residual risk"))
        .map(|line| line.trim().to_string())
}

pub(crate) fn transcript_review_operator_guide(kind: &str) -> String {
    format!(
        "Operator guide: use `/memory compare latest latest-1` for transcript drift and `/reviews latest` for the newest {} artifact.",
        kind
    )
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{
        compare_target_choices, fold_workspace_diff_output, residual_risk_banner,
        review_kind_badge, review_metadata_section, summary_anchor_jump_summary,
        transcript_review_cross_reference, transcript_review_operator_guide,
    };

    #[test]
    fn compare_choices_include_aliases_and_recent_files() {
        let dir = std::env::temp_dir().join(format!("yode-compare-choices-{}", uuid::Uuid::new_v4()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.md"), "x").unwrap();
        let choices = compare_target_choices(&dir);
        assert!(choices.contains(&"latest".to_string()));
        assert!(choices.contains(&"a.md".to_string()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn summary_anchor_and_review_metadata_render() {
        assert!(summary_anchor_jump_summary("## Summary Anchor\n").contains("present"));
        assert_eq!(review_kind_badge(Path::new("/tmp/review-demo.md")), "review");
        assert!(review_metadata_section(Path::new("/tmp/review-demo.md"), "a\nb").contains("Kind: review"));
    }

    #[test]
    fn cross_refs_diff_fold_and_residual_risk_render() {
        let refs = transcript_review_cross_reference(
            Some(Path::new("/tmp/a.md")),
            Some(Path::new("/tmp/review.md")),
        );
        assert!(refs.iter().any(|value| value.contains("/memory a.md")));
        let folded = fold_workspace_diff_output(&(0..50).map(|i| format!("line {}", i)).collect::<Vec<_>>().join("\n"), 10);
        assert!(folded.contains("workspace diff folded"));
        assert_eq!(
            residual_risk_banner("Residual risk: medium").as_deref(),
            Some("Residual risk: medium")
        );
        assert!(transcript_review_operator_guide("review").contains("/memory compare"));
    }
}
