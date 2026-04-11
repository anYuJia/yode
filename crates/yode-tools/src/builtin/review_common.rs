use anyhow::{Context, Result};
use serde_json::{json, Value};
use serde::{Deserialize, Serialize};
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
    let diff_path = dir.join(format!("{}-{}.diff.txt", kind, timestamp));
    let diff_artifact_path = capture_diff_artifact(working_dir, &diff_path).ok();
    let content = format!(
        "# Review Artifact\n\n- Kind: {}\n- Title: {}\n- Timestamp: {}\n- Diff Artifact: {}\n\n## Result\n\n```text\n{}\n```\n",
        kind,
        title,
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
        diff_artifact_path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "none".to_string()),
        body.trim()
    );
    std::fs::write(&path, content)
        .with_context(|| format!("Failed to write review artifact: {}", path.display()))?;
    Ok(path)
}

fn capture_diff_artifact(working_dir: &Path, diff_path: &Path) -> Result<PathBuf> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(working_dir)
        .args(["diff", "--stat"])
        .output()
        .with_context(|| format!("Failed to run git diff in {}", working_dir.display()))?;
    if !output.status.success() {
        anyhow::bail!("git diff --stat failed");
    }
    let diff = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if diff.is_empty() {
        anyhow::bail!("empty diff");
    }
    std::fs::write(diff_path, diff)
        .with_context(|| format!("Failed to write diff artifact: {}", diff_path.display()))?;
    Ok(diff_path.to_path_buf())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewStatusSnapshot {
    pub kind: String,
    pub title: String,
    pub timestamp: String,
    pub status: String,
    pub findings_count: usize,
    pub artifact_path: Option<String>,
}

pub fn persist_review_status(
    working_dir: &Path,
    kind: &str,
    title: &str,
    body: &str,
    artifact_path: Option<&Path>,
) -> Result<PathBuf> {
    let dir = working_dir.join(".yode").join("reviews");
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("Failed to create review status dir: {}", dir.display()))?;

    let path = dir.join("latest-status.json");
    let findings_count = review_findings_count(body);
    let snapshot = ReviewStatusSnapshot {
        kind: kind.to_string(),
        title: title.to_string(),
        timestamp: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        status: if findings_count == 0 {
            "clean".to_string()
        } else {
            "findings".to_string()
        },
        findings_count,
        artifact_path: artifact_path.map(|path| path.display().to_string()),
    };
    std::fs::write(&path, serde_json::to_string_pretty(&snapshot)?)
        .with_context(|| format!("Failed to write review status file: {}", path.display()))?;
    Ok(path)
}

pub fn review_metadata_payload(
    kind: &str,
    title: &str,
    body: &str,
    artifact_path: Option<&str>,
) -> Value {
    let findings_count = review_findings_count(body);
    json!({
        "review_artifact": {
            "kind": kind,
            "title": title,
            "status": if findings_count == 0 { "clean" } else { "findings" },
            "findings_count": findings_count,
            "artifact_path": artifact_path,
        }
    })
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
    use super::{
        persist_review_artifact, persist_review_status, review_findings_count,
        review_metadata_payload,
    };

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

    #[test]
    fn persist_review_status_marks_findings() {
        let dir = tempfile::tempdir().unwrap();
        let path = persist_review_status(
            dir.path(),
            "review",
            "current changes",
            "1. Missing regression test",
            None,
        )
        .unwrap();
        let snapshot: super::ReviewStatusSnapshot =
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        assert_eq!(snapshot.status, "findings");
        assert_eq!(snapshot.findings_count, 1);
    }

    #[test]
    fn persist_review_artifact_writes_diff_backlink_when_git_diff_exists() {
        let dir = tempfile::tempdir().unwrap();
        std::process::Command::new("git")
            .arg("init")
            .current_dir(dir.path())
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        std::fs::write(dir.path().join("a.txt"), "hello\n").unwrap();
        std::process::Command::new("git")
            .args(["add", "a.txt"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["commit", "-m", "baseline"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        std::fs::write(dir.path().join("a.txt"), "hello\nworld\n").unwrap();

        let artifact =
            persist_review_artifact(dir.path(), "review", "changes", "No issues found.").unwrap();
        let content = std::fs::read_to_string(&artifact).unwrap();
        assert!(content.contains("Diff Artifact: "));
        assert!(content.contains(".diff.txt"));
    }

    #[test]
    fn review_metadata_payload_wraps_shared_artifact_schema() {
        let payload = review_metadata_payload("review", "changes", "1. Missing test", Some("artifact.md"));
        let artifact = payload.get("review_artifact").unwrap();
        assert_eq!(artifact.get("kind").and_then(|v| v.as_str()), Some("review"));
        assert_eq!(artifact.get("status").and_then(|v| v.as_str()), Some("findings"));
        assert_eq!(artifact.get("artifact_path").and_then(|v| v.as_str()), Some("artifact.md"));
    }
}
