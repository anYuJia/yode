pub(crate) fn latest_markdown_file(dir: &std::path::Path) -> Option<std::path::PathBuf> {
    let mut entries = std::fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("md"))
        .collect::<Vec<_>>();
    entries.sort_by(|a, b| b.file_name().cmp(&a.file_name()));
    entries.into_iter().next()
}

pub(crate) fn preview_markdown(path: &std::path::Path, section_hint: &str) -> Option<String> {
    let content = std::fs::read_to_string(path).ok()?;
    let preview_source = if let Some(start) = content.find(section_hint) {
        &content[start + section_hint.len()..]
    } else {
        &content
    };
    let squashed = preview_source
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#') && !line.starts_with("```"))
        .take(3)
        .collect::<Vec<_>>()
        .join(" | ");
    if squashed.is_empty() {
        None
    } else if squashed.chars().count() > 180 {
        Some(format!("{}...", squashed.chars().take(180).collect::<String>()))
    } else {
        Some(squashed)
    }
}
