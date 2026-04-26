use super::*;

use serde::Deserialize;
use tracing::{info, warn};
use walkdir::WalkDir;

#[derive(Debug, Default, Deserialize)]
struct MemoryFrontmatter {
    name: Option<String>,
    description: Option<String>,
    #[serde(rename = "type")]
    memory_type: Option<String>,
    scope: Option<String>,
}

pub fn load_memory_context(project_root: &Path) -> Option<String> {
    let mut sections = Vec::new();

    let root_memory = project_root.join("MEMORY.md");
    if let Some(content) = read_text_file(&root_memory) {
        sections.push((
            "MEMORY.md".to_string(),
            render_memory_body(content, &root_memory),
        ));
    }

    for dir in [
        project_root.join(".yode").join("memory"),
        project_root.join(".claude").join("memory"),
        project_root.join(".claude").join("memories"),
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
        "The following memory entries were persisted from earlier work in this workspace. Treat them as previously learned context, not current ground truth.\n\n",
    );
    result.push_str("## How To Use Memory\n\n");
    result.push_str(
        "- Use memory as historical context for what was true when the memory was written.\n",
    );
    result.push_str(
        "- If the user explicitly asks you to check, recall, or remember prior context, consult memory.\n",
    );
    result.push_str(
        "- If the user says to ignore or not use memory, proceed as if this section were empty. Do not cite, compare against, or mention memory content.\n",
    );
    result.push_str(
        "- Before recommending from memory, verify that named files, functions, flags, commands, and resources still exist in the current workspace.\n",
    );
    result.push_str(
        "- If a memory conflicts with the current code, git state, or artifacts you can read now, trust the current state.\n",
    );
    result.push_str(
        "- For recent or current repository state, prefer reading files, artifacts, or git history over relying solely on memory summaries.\n\n",
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
            entries.push((name, render_memory_body(content, path)));
            info!("Loaded memory file: {}", path.display());
        }
    }

    entries.sort_by(|a, b| a.0.cmp(&b.0));
    entries
}

fn render_memory_body(content: String, path: &Path) -> String {
    let (frontmatter, body) = split_memory_frontmatter(&content, path)
        .unwrap_or_else(|| (MemoryFrontmatter::default(), content));
    let mut rendered = String::new();

    if let Some(name) = frontmatter
        .name
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        rendered.push_str("Name: ");
        rendered.push_str(name.trim());
        rendered.push('\n');
    }
    if let Some(description) = frontmatter
        .description
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        rendered.push_str("Description: ");
        rendered.push_str(description.trim());
        rendered.push('\n');
    }
    if let Some(memory_type) = frontmatter
        .memory_type
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        rendered.push_str("Type: ");
        rendered.push_str(memory_type.trim());
        rendered.push('\n');
    }
    if let Some(scope) = frontmatter
        .scope
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        rendered.push_str("Scope: ");
        rendered.push_str(scope.trim());
        rendered.push('\n');
    }

    let limited_body = limit_memory_content(body, path);
    if !rendered.is_empty() && !limited_body.trim().is_empty() {
        rendered.push('\n');
    }
    rendered.push_str(limited_body.trim());
    rendered
}

fn split_memory_frontmatter(content: &str, path: &Path) -> Option<(MemoryFrontmatter, String)> {
    let mut lines = content.lines();
    if lines.next()? != "---" {
        return None;
    }

    let mut yaml_lines = Vec::new();
    let mut closing_found = false;
    for line in lines.by_ref() {
        if line == "---" {
            closing_found = true;
            break;
        }
        yaml_lines.push(line);
        if yaml_lines.len() >= 40 {
            break;
        }
    }

    if !closing_found {
        return None;
    }

    let yaml = yaml_lines.join("\n");
    match serde_yaml_ng::from_str::<MemoryFrontmatter>(&yaml) {
        Ok(frontmatter) => {
            let body = lines.collect::<Vec<_>>().join("\n");
            Some((frontmatter, body.trim_start().to_string()))
        }
        Err(err) => {
            warn!(
                "Failed to parse memory frontmatter in {}: {}",
                path.display(),
                err
            );
            None
        }
    }
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
