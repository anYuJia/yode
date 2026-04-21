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

#[cfg(test)]
mod tests {
    use super::entry_points;

    #[test]
    fn go_entry_points_detect_common_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("main.go"), "package main\n").unwrap();
        std::fs::create_dir_all(dir.path().join("cmd")).unwrap();
        std::fs::write(dir.path().join("cmd").join("main.go"), "package main\n").unwrap();

        let entries = entry_points(dir.path());
        assert!(entries.iter().any(|path| path.ends_with("main.go")));
        assert!(entries.iter().any(|path| path.ends_with("cmd/main.go")));
    }
}
