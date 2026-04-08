use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use tracing::{info, warn};
use walkdir::WalkDir;

const MAX_INSTRUCTION_CHARS: usize = 40_000;
const MAX_MEMORY_BYTES: usize = 25_000;
const MAX_MEMORY_LINES: usize = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum InstructionLayer {
    GlobalAdmin,
    User,
    Project,
    Local,
}

impl InstructionLayer {
    fn title(self) -> &'static str {
        match self {
            Self::GlobalAdmin => "Global Admin Instructions",
            Self::User => "User Instructions",
            Self::Project => "Project Instructions",
            Self::Local => "Local Instructions",
        }
    }
}

#[derive(Debug, Clone)]
struct InstructionEntry {
    layer: InstructionLayer,
    path: PathBuf,
}

#[derive(Debug, Default)]
struct LoadState {
    visited: HashSet<PathBuf>,
    total_chars: usize,
    truncated: bool,
}

pub fn load_instruction_context(project_root: &Path) -> Option<String> {
    load_instruction_context_with_sources(
        project_root,
        dirs::home_dir(),
        Some(PathBuf::from("/etc/claude-code")),
    )
}

fn load_instruction_context_with_sources(
    project_root: &Path,
    home_dir: Option<PathBuf>,
    admin_root: Option<PathBuf>,
) -> Option<String> {
    let entries =
        discover_instruction_entries(project_root, home_dir.as_deref(), admin_root.as_deref());
    if entries.is_empty() {
        return None;
    }

    let mut state = LoadState::default();
    let mut sections: BTreeMap<InstructionLayer, Vec<String>> = BTreeMap::new();

    for entry in entries {
        match expand_instruction_file(&entry.path, &home_dir, &mut state) {
            Some(content) if !content.trim().is_empty() => {
                let rendered = format!(
                    "### {}\n\n{}",
                    display_path(&entry.path, project_root, home_dir.as_deref()),
                    content.trim()
                );
                sections.entry(entry.layer).or_default().push(rendered);
                info!("Loaded instruction file: {}", entry.path.display());
            }
            _ => {}
        }

        if state.total_chars >= MAX_INSTRUCTION_CHARS {
            break;
        }
    }

    if sections.is_empty() {
        return None;
    }

    let mut result = String::new();
    result.push_str("# Instruction Memory\n\n");
    result.push_str(
        "The following instruction files OVERRIDE default behavior when they conflict with generic defaults.\n\n",
    );

    for (layer, content) in sections {
        if content.is_empty() {
            continue;
        }
        result.push_str("## ");
        result.push_str(layer.title());
        result.push_str("\n\n");
        result.push_str(&content.join("\n\n"));
        result.push_str("\n\n");
    }

    if state.truncated {
        result.push_str("[Instruction memory truncated at 40000 characters]\n");
    }

    Some(result.trim_end().to_string())
}

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

fn discover_instruction_entries(
    project_root: &Path,
    home_dir: Option<&Path>,
    admin_root: Option<&Path>,
) -> Vec<InstructionEntry> {
    let mut entries = Vec::new();

    if let Some(admin_root) = admin_root {
        let admin_file = admin_root.join("CLAUDE.md");
        if admin_file.exists() {
            entries.push(InstructionEntry {
                layer: InstructionLayer::GlobalAdmin,
                path: admin_file,
            });
        }
    }

    if let Some(home_dir) = home_dir {
        let user_file = home_dir.join(".claude").join("CLAUDE.md");
        if user_file.exists() {
            entries.push(InstructionEntry {
                layer: InstructionLayer::User,
                path: user_file,
            });
        }
    }

    for relative in [
        "YODE.md",
        "CLAUDE.md",
        "CLAUDE.ai.md",
        "GEMINI.md",
        "AGENTS.md",
        ".claude/CLAUDE.md",
        ".claude/instructions.md",
        ".yode/instructions.md",
        "docs/CLAUDE.md",
        "docs/YODE.md",
    ] {
        let path = project_root.join(relative);
        if path.exists() {
            entries.push(InstructionEntry {
                layer: InstructionLayer::Project,
                path,
            });
        }
    }

    let rules_dir = project_root.join(".claude").join("rules");
    if rules_dir.exists() {
        let mut rule_files = fs::read_dir(&rules_dir)
            .ok()
            .into_iter()
            .flat_map(|entries| entries.filter_map(Result::ok))
            .map(|entry| entry.path())
            .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("md"))
            .collect::<Vec<_>>();
        rule_files.sort();

        for path in rule_files {
            entries.push(InstructionEntry {
                layer: InstructionLayer::Project,
                path,
            });
        }
    }

    for relative in ["CLAUDE.local.md", "YODE.local.md"] {
        let path = project_root.join(relative);
        if path.exists() {
            entries.push(InstructionEntry {
                layer: InstructionLayer::Local,
                path,
            });
        }
    }

    entries
}

fn expand_instruction_file(
    path: &Path,
    home_dir: &Option<PathBuf>,
    state: &mut LoadState,
) -> Option<String> {
    if state.total_chars >= MAX_INSTRUCTION_CHARS {
        state.truncated = true;
        return None;
    }

    if !is_supported_text_file(path) {
        warn!("Skipping non-text instruction include: {}", path.display());
        return None;
    }

    let canonical = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    if !state.visited.insert(canonical) {
        warn!(
            "Skipping already processed instruction file: {}",
            path.display()
        );
        return None;
    }

    let content = read_text_file(path)?;
    let base_dir = path.parent().unwrap_or_else(|| Path::new("."));
    let mut output = String::new();
    let mut in_fence = false;

    for line in content.lines() {
        let trimmed = line.trim_start();

        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            push_with_budget(&mut output, line, state);
            push_with_budget(&mut output, "\n", state);
            in_fence = !in_fence;
            continue;
        }

        if !in_fence && trimmed.starts_with('@') {
            let raw_path = trimmed[1..].trim().trim_matches('"').trim_matches('\'');
            if !raw_path.is_empty() {
                let resolved = resolve_include_path(raw_path, base_dir, home_dir.as_deref());
                if let Some(included) = expand_instruction_file(&resolved, home_dir, state) {
                    push_with_budget(&mut output, included.trim_end(), state);
                    push_with_budget(&mut output, "\n", state);
                }
                continue;
            }
        }

        push_with_budget(&mut output, line, state);
        push_with_budget(&mut output, "\n", state);

        if state.total_chars >= MAX_INSTRUCTION_CHARS {
            break;
        }
    }

    Some(output)
}

fn resolve_include_path(raw_path: &str, base_dir: &Path, home_dir: Option<&Path>) -> PathBuf {
    if let Some(stripped) = raw_path.strip_prefix("~/") {
        if let Some(home_dir) = home_dir {
            return home_dir.join(stripped);
        }
    }

    let candidate = Path::new(raw_path);
    if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        base_dir.join(candidate)
    }
}

fn read_text_file(path: &Path) -> Option<String> {
    let bytes = fs::read(path).ok()?;
    if bytes.contains(&0) {
        return None;
    }
    String::from_utf8(bytes).ok()
}

fn is_supported_text_file(path: &Path) -> bool {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    if matches!(
        file_name,
        "CLAUDE.md"
            | "CLAUDE.ai.md"
            | "CLAUDE.local.md"
            | "YODE.md"
            | "YODE.local.md"
            | "GEMINI.md"
            | "AGENTS.md"
            | "instructions.md"
            | "MEMORY.md"
    ) {
        return true;
    }

    matches!(
        path.extension().and_then(|ext| ext.to_str()),
        Some("md" | "markdown" | "txt" | "toml" | "yaml" | "yml" | "json" | "rs" | "ts" | "js")
    )
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

fn push_with_budget(output: &mut String, text: &str, state: &mut LoadState) {
    if text.is_empty() || state.total_chars >= MAX_INSTRUCTION_CHARS {
        if !text.is_empty() {
            state.truncated = true;
        }
        return;
    }

    let remaining = MAX_INSTRUCTION_CHARS - state.total_chars;
    let text_chars = text.chars().count();

    if text_chars <= remaining {
        output.push_str(text);
        state.total_chars += text_chars;
        return;
    }

    output.extend(text.chars().take(remaining));
    state.total_chars = MAX_INSTRUCTION_CHARS;
    state.truncated = true;
}

#[cfg(test)]
mod tests {
    use super::{load_instruction_context_with_sources, load_memory_context};
    use std::fs;

    #[test]
    fn loads_layered_instructions_in_priority_order() {
        let temp = tempfile::tempdir().unwrap();
        let project = temp.path().join("project");
        let home = temp.path().join("home");
        let admin = temp.path().join("admin");

        fs::create_dir_all(project.join(".claude").join("rules")).unwrap();
        fs::create_dir_all(home.join(".claude")).unwrap();
        fs::create_dir_all(&admin).unwrap();

        fs::write(admin.join("CLAUDE.md"), "admin rule").unwrap();
        fs::write(home.join(".claude").join("CLAUDE.md"), "user rule").unwrap();
        fs::write(project.join("CLAUDE.md"), "project rule").unwrap();
        fs::write(project.join(".claude").join("rules").join("b.md"), "rule b").unwrap();
        fs::write(project.join(".claude").join("rules").join("a.md"), "rule a").unwrap();
        fs::write(project.join("CLAUDE.local.md"), "local rule").unwrap();

        let loaded = load_instruction_context_with_sources(
            &project,
            Some(home.clone()),
            Some(admin.clone()),
        )
        .unwrap();

        let admin_idx = loaded.find("admin rule").unwrap();
        let user_idx = loaded.find("user rule").unwrap();
        let project_idx = loaded.find("project rule").unwrap();
        let rule_a_idx = loaded.find("rule a").unwrap();
        let rule_b_idx = loaded.find("rule b").unwrap();
        let local_idx = loaded.find("local rule").unwrap();

        assert!(admin_idx < user_idx);
        assert!(user_idx < project_idx);
        assert!(project_idx < rule_a_idx);
        assert!(rule_a_idx < rule_b_idx);
        assert!(rule_b_idx < local_idx);
    }

    #[test]
    fn supports_include_without_expanding_code_fences() {
        let temp = tempfile::tempdir().unwrap();
        let project = temp.path().join("project");
        fs::create_dir_all(project.join("docs")).unwrap();

        fs::write(
            project.join("docs").join("shared.md"),
            "shared instructions",
        )
        .unwrap();
        fs::write(
            project.join("docs").join("ignored.md"),
            "should stay ignored",
        )
        .unwrap();
        fs::write(
            project.join("CLAUDE.md"),
            "intro\n@./docs/shared.md\n```md\n@./docs/ignored.md\n```\noutro\n",
        )
        .unwrap();

        let loaded = load_instruction_context_with_sources(&project, None, None).unwrap();
        assert!(loaded.contains("shared instructions"));
        assert!(loaded.contains("@./docs/ignored.md"));
        assert!(!loaded.contains("should stay ignored"));
    }

    #[test]
    fn prevents_circular_includes() {
        let temp = tempfile::tempdir().unwrap();
        let project = temp.path().join("project");
        fs::create_dir_all(&project).unwrap();

        fs::write(project.join("A.md"), "A top\n@./B.md\n").unwrap();
        fs::write(project.join("B.md"), "B top\n@./A.md\n").unwrap();
        fs::write(project.join("CLAUDE.md"), "@./A.md\n").unwrap();

        let loaded = load_instruction_context_with_sources(&project, None, None).unwrap();
        assert_eq!(loaded.matches("A top").count(), 1);
        assert_eq!(loaded.matches("B top").count(), 1);
    }

    #[test]
    fn loads_project_memory_from_supported_locations() {
        let temp = tempfile::tempdir().unwrap();
        let project = temp.path().join("project");
        fs::create_dir_all(project.join(".yode").join("memory").join("nested")).unwrap();
        fs::create_dir_all(project.join("memory")).unwrap();

        fs::write(project.join("MEMORY.md"), "root memory").unwrap();
        fs::write(
            project
                .join(".yode")
                .join("memory")
                .join("nested")
                .join("notes.md"),
            "nested memory",
        )
        .unwrap();
        fs::write(project.join("memory").join("legacy.md"), "legacy memory").unwrap();

        let loaded = load_memory_context(&project).unwrap();
        assert!(loaded.contains("root memory"));
        assert!(loaded.contains("nested memory"));
        assert!(loaded.contains("legacy memory"));
    }
}
