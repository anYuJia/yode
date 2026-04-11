use super::*;

use tracing::{info, warn};

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
pub(super) fn load_instruction_context_with_sources_test(
    project_root: &Path,
    home_dir: Option<PathBuf>,
    admin_root: Option<PathBuf>,
) -> Option<String> {
    load_instruction_context_with_sources(project_root, home_dir, admin_root)
}
