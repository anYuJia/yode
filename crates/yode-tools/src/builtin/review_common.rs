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
