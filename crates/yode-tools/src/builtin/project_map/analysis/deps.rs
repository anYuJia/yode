use std::path::Path;

use super::ProjectType;
use super::ecosystems::rust;

pub(in crate::builtin::project_map) fn analyze_dependencies(
    dir: &Path,
    project_type: &ProjectType,
) -> Vec<(String, Vec<String>)> {
    match project_type {
        ProjectType::Rust | ProjectType::RustWorkspace => rust::analyze_dependencies(dir),
        _ => Vec::new(),
    }
}
