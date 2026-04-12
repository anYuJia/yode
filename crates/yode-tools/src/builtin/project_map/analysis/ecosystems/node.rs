use std::path::{Path, PathBuf};

pub(in crate::builtin::project_map) fn entry_points(dir: &Path) -> Vec<PathBuf> {
    let mut entries = Vec::new();
    for candidate in [
        "src/index.ts",
        "src/index.js",
        "index.ts",
        "index.js",
        "src/main.ts",
        "src/app.ts",
    ] {
        let path = dir.join(candidate);
        if path.exists() {
            entries.push(path);
        }
    }
    entries
}
