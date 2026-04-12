mod deps;
mod ecosystems;
mod scan;
mod tree;

pub(in crate::builtin::project_map) use deps::analyze_dependencies;
pub(in crate::builtin::project_map) use scan::{
    detect_project_type, find_config_files, find_entry_points, scan_project_stats,
};
pub(in crate::builtin::project_map) use tree::build_module_tree;

use std::path::Path;

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

pub(super) struct ProjectStats {
    pub(super) file_count: usize,
    pub(super) total_lines: usize,
    pub(super) lines_by_language: Vec<(String, usize)>,
}

pub(super) fn walk_files(dir: &Path, callback: &mut dyn FnMut(&Path)) {
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
