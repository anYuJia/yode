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

#[cfg(test)]
mod tests {
    use super::entry_points;

    #[test]
    fn python_entry_points_detect_common_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("main.py"), "print('hi')\n").unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src").join("__main__.py"), "print('hi')\n").unwrap();

        let entries = entry_points(dir.path());
        assert!(entries.iter().any(|path| path.ends_with("main.py")));
        assert!(entries.iter().any(|path| path.ends_with("src/__main__.py")));
    }
}
