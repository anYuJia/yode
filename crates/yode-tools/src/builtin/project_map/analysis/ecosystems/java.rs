use std::path::{Path, PathBuf};

pub(in crate::builtin::project_map) fn entry_points(dir: &Path) -> Vec<PathBuf> {
    let mut entries = Vec::new();
    let path = dir.join("src/main/java/Main.java");
    if path.exists() {
        entries.push(path);
    }
    entries
}

#[cfg(test)]
mod tests {
    use super::entry_points;

    #[test]
    fn java_entry_points_detect_main_class() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("src/main/java")).unwrap();
        std::fs::write(
            dir.path().join("src/main/java/Main.java"),
            "class Main {}\n",
        )
        .unwrap();

        let entries = entry_points(dir.path());
        assert_eq!(entries.len(), 1);
        assert!(entries[0].ends_with("src/main/java/Main.java"));
    }
}
