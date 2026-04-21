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

#[cfg(test)]
mod tests {
    use super::entry_points;

    #[test]
    fn node_entry_points_detect_common_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src").join("index.ts"), "export {};\n").unwrap();
        std::fs::write(dir.path().join("src").join("app.ts"), "export {};\n").unwrap();

        let entries = entry_points(dir.path());
        assert!(entries.iter().any(|path| path.ends_with("src/index.ts")));
        assert!(entries.iter().any(|path| path.ends_with("src/app.ts")));
    }
}
