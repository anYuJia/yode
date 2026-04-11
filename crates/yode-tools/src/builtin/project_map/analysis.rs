use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub(super) enum ProjectType {
    Rust,
    RustWorkspace,
    Node,
    Go,
    Python,
    Java,
    Unknown,
}

impl ProjectType {
    pub(super) fn display_name(&self) -> &str {
        match self {
            ProjectType::Rust => "Rust project",
            ProjectType::RustWorkspace => "Rust workspace",
            ProjectType::Node => "Node.js project",
            ProjectType::Go => "Go project",
            ProjectType::Python => "Python project",
            ProjectType::Java => "Java project",
            ProjectType::Unknown => "Unknown",
        }
    }
}

pub(super) fn detect_project_type(dir: &Path) -> ProjectType {
    if dir.join("Cargo.toml").exists() {
        if let Ok(content) = std::fs::read_to_string(dir.join("Cargo.toml")) {
            if content.contains("[workspace]") {
                return ProjectType::RustWorkspace;
            }
        }
        return ProjectType::Rust;
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

pub(super) struct ProjectStats {
    pub(super) file_count: usize,
    pub(super) total_lines: usize,
    pub(super) lines_by_language: Vec<(String, usize)>,
}

pub(super) fn scan_project_stats(dir: &Path) -> ProjectStats {
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

pub(super) fn find_entry_points(dir: &Path, project_type: &ProjectType) -> Vec<PathBuf> {
    let candidates: &[&str] = match project_type {
        ProjectType::Rust | ProjectType::RustWorkspace => {
            &["src/main.rs", "src/lib.rs", "src/bin/main.rs"]
        }
        ProjectType::Node => &[
            "src/index.ts",
            "src/index.js",
            "index.ts",
            "index.js",
            "src/main.ts",
            "src/app.ts",
        ],
        ProjectType::Go => &["main.go", "cmd/main.go"],
        ProjectType::Python => &[
            "main.py",
            "app.py",
            "src/main.py",
            "__main__.py",
            "src/__main__.py",
        ],
        ProjectType::Java => &["src/main/java/Main.java"],
        ProjectType::Unknown => &[],
    };

    let mut entries = Vec::new();
    for candidate in candidates {
        let path = dir.join(candidate);
        if path.exists() {
            entries.push(path);
        }
    }

    if matches!(project_type, ProjectType::RustWorkspace) {
        if let Ok(content) = std::fs::read_to_string(dir.join("Cargo.toml")) {
            for line in content.lines() {
                let trimmed = line.trim().trim_matches('"').trim_matches(',');
                if trimmed.contains('/') || trimmed.starts_with("crates/") {
                    for entry in &["src/main.rs", "src/lib.rs"] {
                        let path = dir.join(trimmed).join(entry);
                        if path.exists() {
                            entries.push(path);
                        }
                    }
                }
            }
        }
    }

    entries
}

pub(super) fn build_module_tree(dir: &Path, max_depth: usize) -> String {
    let mut output = String::new();
    build_tree_recursive(dir, dir, 0, max_depth, &mut output);
    output
}

fn build_tree_recursive(
    root: &Path,
    dir: &Path,
    depth: usize,
    max_depth: usize,
    output: &mut String,
) {
    if depth > max_depth {
        return;
    }

    let mut entries: Vec<_> = match std::fs::read_dir(dir) {
        Ok(entries) => entries.filter_map(|entry| entry.ok()).collect(),
        Err(_) => return,
    };
    entries.sort_by_key(|entry| entry.file_name());

    for entry in entries {
        let path = entry.path();
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        if name_str.starts_with('.')
            || name_str == "target"
            || name_str == "node_modules"
            || name_str == "dist"
            || name_str == "__pycache__"
        {
            continue;
        }

        if path.is_dir() {
            let indent = "  ".repeat(depth);
            let rel = path.strip_prefix(root).unwrap_or(&path);
            let file_count = count_code_files(&path);
            if file_count > 0 {
                output.push_str(&format!(
                    "{}├── {} ({} files)\n",
                    indent,
                    rel.display(),
                    file_count
                ));
            } else {
                output.push_str(&format!("{}├── {}\n", indent, rel.display()));
            }

            build_tree_recursive(root, &path, depth + 1, max_depth, output);
        }
    }
}

fn count_code_files(dir: &Path) -> usize {
    let mut count = 0;
    walk_files(dir, &mut |path| {
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            if matches!(
                ext,
                "rs" | "js" | "ts" | "jsx" | "tsx" | "py" | "go" | "java"
            ) {
                count += 1;
            }
        }
    });
    count
}

pub(super) fn analyze_dependencies(
    dir: &Path,
    project_type: &ProjectType,
) -> Vec<(String, Vec<String>)> {
    match project_type {
        ProjectType::Rust | ProjectType::RustWorkspace => analyze_rust_deps(dir),
        _ => Vec::new(),
    }
}

fn analyze_rust_deps(dir: &Path) -> Vec<(String, Vec<String>)> {
    let mut result = Vec::new();
    let mut cargo_files = Vec::new();
    walk_files(dir, &mut |path| {
        if path
            .file_name()
            .map(|name| name == "Cargo.toml")
            .unwrap_or(false)
            && !path.to_string_lossy().contains("/target/")
        {
            cargo_files.push(path.to_path_buf());
        }
    });

    for cargo_path in cargo_files {
        if let Ok(content) = std::fs::read_to_string(&cargo_path) {
            let crate_dir = cargo_path.parent().unwrap_or(dir);
            let crate_name = crate_dir
                .strip_prefix(dir)
                .unwrap_or(crate_dir)
                .display()
                .to_string();

            if crate_name.is_empty() {
                continue;
            }

            let mut deps = Vec::new();
            let mut in_deps = false;

            for line in content.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with("[dependencies]")
                    || trimmed.starts_with("[dev-dependencies]")
                {
                    in_deps = true;
                    continue;
                }
                if trimmed.starts_with('[') {
                    in_deps = false;
                    continue;
                }
                if in_deps {
                    if let Some(dep_name) = trimmed.split('=').next() {
                        let dep_name = dep_name.trim();
                        if !dep_name.is_empty() && !dep_name.starts_with('#') {
                            deps.push(dep_name.to_string());
                        }
                    }
                }
            }

            if !deps.is_empty() {
                result.push((crate_name, deps));
            }
        }
    }

    result.sort_by(|a, b| a.0.cmp(&b.0));
    result
}

pub(super) fn find_config_files(dir: &Path) -> Vec<PathBuf> {
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

fn walk_files(dir: &Path, callback: &mut dyn FnMut(&Path)) {
    walk_files_recursive(dir, callback, 0, 10);
}

fn walk_files_recursive(
    dir: &Path,
    callback: &mut dyn FnMut(&Path),
    depth: usize,
    max_depth: usize,
) {
    if depth > max_depth {
        return;
    }

    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        if name_str.starts_with('.')
            || name_str == "target"
            || name_str == "node_modules"
            || name_str == "dist"
            || name_str == "__pycache__"
            || name_str == ".git"
        {
            continue;
        }

        if path.is_dir() {
            walk_files_recursive(&path, callback, depth + 1, max_depth);
        } else if path.is_file() {
            callback(&path);
        }
    }
}
