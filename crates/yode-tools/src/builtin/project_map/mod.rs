use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};

use crate::tool::{Tool, ToolCapabilities, ToolContext, ToolResult};

pub struct ProjectMapTool;

#[async_trait]
impl Tool for ProjectMapTool {
    fn name(&self) -> &str {
        "project_map"
    }

    fn user_facing_name(&self) -> &str {
        "Project Map"
    }

    fn activity_description(&self, _params: &Value) -> String {
        "Analyzing project structure".to_string()
    }

    fn description(&self) -> &str {
        "Generate a project structure model including type detection, module map, entry points, \
         config files, and dependency analysis. Use this FIRST when analyzing an unfamiliar codebase \
         to build a mental model before diving into specifics."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "depth": {
                    "type": "integer",
                    "description": "Directory scan depth (default: 2)",
                    "default": 2
                },
                "include_deps": {
                    "type": "boolean",
                    "description": "Whether to analyze module-level dependencies (default: true)",
                    "default": true
                }
            }
        })
    }

    fn capabilities(&self) -> ToolCapabilities {
        ToolCapabilities {
            read_only: true,
            requires_confirmation: false,
            supports_auto_execution: true,
        }
    }

    async fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolResult> {
        let working_dir = ctx
            .working_dir
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Working directory not set"))?;

        let depth = params.get("depth").and_then(|v| v.as_u64()).unwrap_or(2) as usize;
        let include_deps = params
            .get("include_deps")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let mut output = String::new();

        // Phase 1: Detect project type
        let project_type = detect_project_type(working_dir);
        output.push_str("## Project Overview\n");
        output.push_str(&format!("- Type: {}\n", project_type.display_name()));

        // Phase 2: Scan project scale
        let stats = scan_project_stats(working_dir);
        output.push_str(&format!(
            "- Scale: {} files, ~{}K lines\n",
            stats.file_count,
            stats.total_lines / 1000
        ));
        for (lang, count) in &stats.lines_by_language {
            output.push_str(&format!("  - {}: {} lines\n", lang, count));
        }

        // Phase 3: Find entry points
        let entries = find_entry_points(working_dir, &project_type);
        if !entries.is_empty() {
            output.push_str("\n## Entry Points\n");
            for entry in &entries {
                let rel = entry.strip_prefix(working_dir).unwrap_or(entry).display();
                output.push_str(&format!("- {}\n", rel));
            }
        }

        // Phase 4: Build module map
        let tree = build_module_tree(working_dir, depth);
        if !tree.is_empty() {
            output.push_str("\n## Module Map\n");
            output.push_str(&tree);
        }

        // Phase 5: Analyze dependencies
        if include_deps {
            let deps = analyze_dependencies(working_dir, &project_type);
            if !deps.is_empty() {
                output.push_str("\n## Dependencies\n");
                for (module, dep_list) in &deps {
                    output.push_str(&format!(
                        "- {} → depends on: {}\n",
                        module,
                        dep_list.join(", ")
                    ));
                }
            }
        }

        // Phase 6: Config files
        let configs = find_config_files(working_dir);
        if !configs.is_empty() {
            output.push_str("\n## Config Files\n");
            for cfg in &configs {
                let rel = cfg.strip_prefix(working_dir).unwrap_or(cfg).display();
                output.push_str(&format!("- {}\n", rel));
            }
        }

        let metadata = json!({
            "project_type": project_type.display_name(),
            "file_count": stats.file_count,
            "total_lines": stats.total_lines,
        });

        Ok(ToolResult::success_with_metadata(output, metadata))
    }
}

#[derive(Debug)]
enum ProjectType {
    Rust,
    RustWorkspace,
    Node,
    Go,
    Python,
    Java,
    Unknown,
}

impl ProjectType {
    fn display_name(&self) -> &str {
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

fn detect_project_type(dir: &Path) -> ProjectType {
    if dir.join("Cargo.toml").exists() {
        // Check if it's a workspace
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

struct ProjectStats {
    file_count: usize,
    total_lines: usize,
    lines_by_language: Vec<(String, usize)>,
}

fn scan_project_stats(dir: &Path) -> ProjectStats {
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

fn find_entry_points(dir: &Path, project_type: &ProjectType) -> Vec<PathBuf> {
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
    for c in candidates {
        let path = dir.join(c);
        if path.exists() {
            entries.push(path);
        }
    }

    // For Rust workspaces, also check crate entry points
    if matches!(project_type, ProjectType::RustWorkspace) {
        if let Ok(content) = std::fs::read_to_string(dir.join("Cargo.toml")) {
            // Simple members parsing
            for line in content.lines() {
                let trimmed = line.trim().trim_matches('"').trim_matches(',');
                if trimmed.contains('/') || trimmed.starts_with("crates/") {
                    // Try common patterns
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

fn build_module_tree(dir: &Path, max_depth: usize) -> String {
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
        Ok(entries) => entries.filter_map(|e| e.ok()).collect(),
        Err(_) => return,
    };
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        let path = entry.path();
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        // Skip hidden dirs, target, node_modules, etc.
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

            // Count files in this directory
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

fn analyze_dependencies(dir: &Path, project_type: &ProjectType) -> Vec<(String, Vec<String>)> {
    match project_type {
        ProjectType::Rust | ProjectType::RustWorkspace => analyze_rust_deps(dir),
        _ => Vec::new(), // TODO: add support for other languages
    }
}

fn analyze_rust_deps(dir: &Path) -> Vec<(String, Vec<String>)> {
    let mut result = Vec::new();

    // Find all Cargo.toml files with [dependencies] sections
    let mut cargo_files = Vec::new();
    walk_files(dir, &mut |path| {
        if path.file_name().map(|n| n == "Cargo.toml").unwrap_or(false) {
            // Skip target directory
            if !path.to_string_lossy().contains("/target/") {
                cargo_files.push(path.to_path_buf());
            }
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
                continue; // Skip root Cargo.toml for workspaces
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

fn find_config_files(dir: &Path) -> Vec<PathBuf> {
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

    // Check root-level config patterns
    for pattern in &config_patterns {
        let path = dir.join(pattern);
        if path.exists() {
            configs.push(path);
        }
    }

    // Check config/ directory
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

/// Walk files recursively, respecting .gitignore-like patterns.
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
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        // Skip hidden, target, node_modules, etc.
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
