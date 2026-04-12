use std::path::{Path, PathBuf};

pub(in crate::builtin::project_map) fn entry_points(dir: &Path) -> Vec<PathBuf> {
    let mut entries = Vec::new();
    for candidate in [
        "main.py",
        "app.py",
        "src/main.py",
        "__main__.py",
        "src/__main__.py",
    ] {
        let path = dir.join(candidate);
        if path.exists() {
            entries.push(path);
        }
    }
    entries
}
