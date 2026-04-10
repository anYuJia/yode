use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

pub fn persist_review_artifact(
    working_dir: &Path,
    kind: &str,
    title: &str,
    body: &str,
) -> Result<PathBuf> {
    let dir = working_dir.join(".yode").join("reviews");
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("Failed to create review artifact dir: {}", dir.display()))?;

    let timestamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let path = dir.join(format!("{}-{}.md", kind, timestamp));
    let content = format!(
        "# Review Artifact\n\n- Kind: {}\n- Title: {}\n- Timestamp: {}\n\n## Result\n\n```text\n{}\n```\n",
        kind,
        title,
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
        body.trim()
    );
    std::fs::write(&path, content)
        .with_context(|| format!("Failed to write review artifact: {}", path.display()))?;
    Ok(path)
}

pub fn review_output_has_findings(output: &str) -> bool {
    let normalized = output.trim().to_lowercase();
    if normalized.starts_with("no issues found.") || normalized.starts_with("no issues found") {
        return false;
    }
    if normalized.starts_with("no findings") || normalized.contains("no issues found") {
        return false;
    }
    true
}

pub fn review_findings_count(output: &str) -> usize {
    let trimmed = output.trim();
    if trimmed.is_empty() || !review_output_has_findings(trimmed) {
        return 0;
    }

    let structured = trimmed
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter(|line| is_structured_finding_line(line))
        .count();

    if structured == 0 {
        1
    } else {
        structured
    }
}

fn is_structured_finding_line(line: &str) -> bool {
    if line.starts_with("- ") || line.starts_with("* ") {
        return true;
    }

    let digit_count = line.chars().take_while(|c| c.is_ascii_digit()).count();
    digit_count > 0
        && line
            .chars()
            .nth(digit_count)
            .map(|ch| ch == '.')
            .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::review_findings_count;

    #[test]
    fn review_findings_count_detects_clean_output() {
        assert_eq!(
            review_findings_count("No issues found.\nResidual risk: none."),
            0
        );
    }

    #[test]
    fn review_findings_count_counts_numbered_findings() {
        assert_eq!(
            review_findings_count("1. Missing test\n2. Risky assumption"),
            2
        );
    }

    #[test]
    fn review_findings_count_defaults_to_one_for_unstructured_findings() {
        assert_eq!(review_findings_count("Missing regression test"), 1);
    }
}
