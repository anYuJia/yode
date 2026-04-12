use std::path::{Path, PathBuf};

pub(in crate::builtin::project_map) fn entry_points(dir: &Path) -> Vec<PathBuf> {
    let mut entries = Vec::new();
    for candidate in ["main.go", "cmd/main.go"] {
        let path = dir.join(candidate);
        if path.exists() {
            entries.push(path);
        }
    }
    entries
}
