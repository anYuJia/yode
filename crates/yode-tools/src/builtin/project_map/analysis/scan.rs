use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::{walk_files, ProjectStats, ProjectType};
use super::ecosystems::{go, java, node, python, rust};

pub(in crate::builtin::project_map) fn detect_project_type(dir: &Path) -> ProjectType {
    if dir.join("Cargo.toml").exists() {
        return if rust::is_workspace(dir) {
            ProjectType::RustWorkspace
        } else {
            ProjectType::Rust
        };
    }
    if dir.join("package.json").exists() {
        return ProjectType::Node;
    }
    if dir.join("go.mod").exists() {
        return ProjectType::Go;
    }
    if dir.join("pyproject.toml").exists()
        || dir.join("setup.py").exists()
        || dir.join("requirements.txt").exists()
    {
        return ProjectType::Python;
    }
    if dir.join("pom.xml").exists() || dir.join("build.gradle").exists() {
        return ProjectType::Java;
    }
    ProjectType::Unknown
}

pub(in crate::builtin::project_map) fn scan_project_stats(dir: &Path) -> ProjectStats {
    let mut file_count = 0;
    let mut total_lines = 0;
    let mut lang_lines: HashMap<String, usize> = HashMap::new();

    walk_files(dir, &mut |path| {
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            let lang = match ext {
                "rs" => "Rust",
                "js" | "jsx" | "mjs" => "JavaScript",
                "ts" | "tsx" => "TypeScript",
                "py" => "Python",
                "go" => "Go",
                "java" => "Java",
                "toml" | "yml" | "yaml" | "json" => "Config",
                "md" => "Markdown",
                _ => return,
            };
            file_count += 1;
            if let Ok(content) = std::fs::read_to_string(path) {
                let lines = content.lines().count();
                total_lines += lines;
                *lang_lines.entry(lang.to_string()).or_insert(0) += lines;
            }
        }
    });

    let mut lines_by_language: Vec<(String, usize)> = lang_lines.into_iter().collect();
    lines_by_language.sort_by(|a, b| b.1.cmp(&a.1));

    ProjectStats {
        file_count,
        total_lines,
        lines_by_language,
    }
}

pub(in crate::builtin::project_map) fn find_entry_points(
    dir: &Path,
    project_type: &ProjectType,
) -> Vec<PathBuf> {
    match project_type {
        ProjectType::Rust | ProjectType::RustWorkspace => rust::entry_points(dir),
        ProjectType::Node => node::entry_points(dir),
        ProjectType::Go => go::entry_points(dir),
        ProjectType::Python => python::entry_points(dir),
        ProjectType::Java => java::entry_points(dir),
        ProjectType::Unknown => Vec::new(),
    }
}

pub(in crate::builtin::project_map) fn find_config_files(dir: &Path) -> Vec<PathBuf> {
    let config_patterns = [
        ".env",
        ".env.example",
        "docker-compose.yml",
        "docker-compose.yaml",
        "Dockerfile",
        ".dockerignore",
        ".github",
    ];
    let config_extensions = ["toml", "yml", "yaml"];

    let mut configs = Vec::new();
    for pattern in &config_patterns {
        let path = dir.join(pattern);
        if path.exists() {
            configs.push(path);
        }
    }

    let config_dir = dir.join("config");
    if config_dir.is_dir() {
        if let Ok(entries) = std::fs::read_dir(&config_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                        if config_extensions.contains(&ext) {
                            configs.push(path);
                        }
                    }
                }
            }
        }
    }

    configs
}
