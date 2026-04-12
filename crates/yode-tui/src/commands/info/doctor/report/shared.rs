pub(super) fn render_section(title: &str, checks: &[String]) -> String {
    if checks.is_empty() {
        return String::new();
    }
    format!("{}\n{}\n", title, checks.join("\n"))
}

pub(super) fn format_artifact_entry(path: &std::path::Path) -> String {
    let metadata = std::fs::metadata(path).ok();
    let size = metadata
        .as_ref()
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    let modified = metadata
        .and_then(|metadata| metadata.modified().ok())
        .and_then(|stamp| stamp.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|stamp| stamp.as_secs().to_string())
        .unwrap_or_else(|| "unknown".to_string());
    format!("  - {} ({} bytes, mtime={})", path.display(), size, modified)
}
