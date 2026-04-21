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

#[cfg(test)]
mod tests {
    use super::analyze_dependencies;
    use crate::builtin::project_map::analysis::ProjectType;

    #[test]
    fn analyze_dependencies_returns_empty_for_non_rust_projects() {
        let dir = tempfile::tempdir().unwrap();
        let deps = analyze_dependencies(dir.path(), &ProjectType::Node);
        assert!(deps.is_empty());
    }
}
