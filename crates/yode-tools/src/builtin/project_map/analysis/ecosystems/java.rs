use std::path::{Path, PathBuf};

pub(in crate::builtin::project_map) fn entry_points(dir: &Path) -> Vec<PathBuf> {
    let mut entries = Vec::new();
    let path = dir.join("src/main/java/Main.java");
    if path.exists() {
        entries.push(path);
    }
    entries
}
