use super::*;

use tracing::{info, warn};
use walkdir::WalkDir;

pub fn load_memory_context(project_root: &Path) -> Option<String> {
    let mut sections = Vec::new();

    let root_memory = project_root.join("MEMORY.md");
    if let Some(content) = read_text_file(&root_memory) {
        sections.push((
            "MEMORY.md".to_string(),
            limit_memory_content(content, &root_memory),
        ));
    }

    for dir in [
        project_root.join(".yode").join("memory"),
        project_root.join("memory"),
    ] {
        sections.extend(load_memory_dir(project_root, &dir));
    }

    if sections.is_empty() {
        return None;
    }

    let mut result = String::new();
    result.push_str("# Persistent Memory\n\n");
    result.push_str(
        "The following memory entries were persisted from earlier work in this workspace. Treat them as previously learned context.\n\n",
    );

    for (name, content) in sections {
        if content.trim().is_empty() {
            continue;
        }
        result.push_str("## ");
        result.push_str(&name);
        result.push_str("\n\n");
        result.push_str(content.trim());
        result.push_str("\n\n");
    }

    Some(result.trim_end().to_string())
}

fn load_memory_dir(project_root: &Path, memory_dir: &Path) -> Vec<(String, String)> {
    if !memory_dir.exists() {
        return Vec::new();
    }

    let mut entries = Vec::new();

    for entry in WalkDir::new(memory_dir)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
    {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("md") {
            continue;
        }

        if let Some(content) = read_text_file(path) {
            let name = display_path(path, project_root, dirs::home_dir().as_deref());
            entries.push((name, limit_memory_content(content, path)));
            info!("Loaded memory file: {}", path.display());
        }
    }

    entries.sort_by(|a, b| a.0.cmp(&b.0));
    entries
}

fn limit_memory_content(mut content: String, path: &Path) -> String {
    let mut lines = content.lines().collect::<Vec<_>>();
    if lines.len() > MAX_MEMORY_LINES {
        lines.truncate(MAX_MEMORY_LINES);
        content = lines.join("\n");
        content.push_str("\n[Memory truncated by line limit]");
    }

    if content.len() <= MAX_MEMORY_BYTES {
        return content;
    }

    let mut truncated = content
        .char_indices()
        .take_while(|(idx, _)| *idx < MAX_MEMORY_BYTES)
        .map(|(_, ch)| ch)
        .collect::<String>();

    if let Some(last_newline) = truncated.rfind('\n') {
        truncated.truncate(last_newline);
    }

    warn!(
        "Memory file {} exceeded {} bytes and was truncated for prompt injection",
        path.display(),
        MAX_MEMORY_BYTES
    );
    truncated.push_str("\n[Memory truncated by byte limit]");
    truncated
}

fn read_text_file(path: &Path) -> Option<String> {
    let bytes = fs::read(path).ok()?;
    if bytes.contains(&0) {
        return None;
    }
    String::from_utf8(bytes).ok()
}

fn display_path(path: &Path, project_root: &Path, home_dir: Option<&Path>) -> String {
    if let Some(home_dir) = home_dir {
        if let Ok(relative) = path.strip_prefix(home_dir) {
            return format!("~/{}", relative.display());
        }
    }

    if let Ok(relative) = path.strip_prefix(project_root) {
        return relative.display().to_string();
    }

    path.display().to_string()
}
