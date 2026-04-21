use std::path::{Path, PathBuf};

use super::super::walk_files;

pub(in crate::builtin::project_map) fn is_workspace(dir: &Path) -> bool {
    dir.join("Cargo.toml")
        .exists()
        && std::fs::read_to_string(dir.join("Cargo.toml"))
            .map(|content| content.contains("[workspace]"))
            .unwrap_or(false)
}

pub(in crate::builtin::project_map) fn entry_points(dir: &Path) -> Vec<PathBuf> {
    let mut entries = Vec::new();
    for candidate in ["src/main.rs", "src/lib.rs", "src/bin/main.rs"] {
        let path = dir.join(candidate);
        if path.exists() {
            entries.push(path);
        }
    }

    if is_workspace(dir) {
        if let Ok(content) = std::fs::read_to_string(dir.join("Cargo.toml")) {
            for line in content.lines() {
                let trimmed = line.trim().trim_matches('"').trim_matches(',');
                if trimmed.contains('/') || trimmed.starts_with("crates/") {
                    for candidate in ["src/main.rs", "src/lib.rs"] {
                        let path = dir.join(trimmed).join(candidate);
                        if path.exists() {
                            entries.push(path);
                        }
                    }
                }
            }
        }
    }

    entries
}

pub(in crate::builtin::project_map) fn analyze_dependencies(dir: &Path) -> Vec<(String, Vec<String>)> {
    let mut result = Vec::new();
    let mut cargo_files = Vec::new();
    walk_files(dir, &mut |path| {
        if path.file_name().map(|name| name == "Cargo.toml").unwrap_or(false)
            && !path.to_string_lossy().contains("/target/")
        {
            cargo_files.push(path.to_path_buf());
        }
    });

    for cargo_path in cargo_files {
        if let Ok(content) = std::fs::read_to_string(&cargo_path) {
            let crate_dir = cargo_path.parent().unwrap_or(dir);
            let crate_name = crate_dir
                .strip_prefix(dir)
                .unwrap_or(crate_dir)
                .display()
                .to_string();
            if crate_name.is_empty() {
                continue;
            }

            let mut deps = Vec::new();
            let mut in_deps = false;
            for line in content.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with("[dependencies]") || trimmed.starts_with("[dev-dependencies]") {
                    in_deps = true;
                    continue;
                }
                if trimmed.starts_with('[') {
                    in_deps = false;
                    continue;
                }
                if in_deps {
                    if let Some(dep_name) = trimmed.split('=').next() {
                        let dep_name = dep_name.trim();
                        if !dep_name.is_empty() && !dep_name.starts_with('#') {
                            deps.push(dep_name.to_string());
                        }
                    }
                }
            }

            if !deps.is_empty() {
                result.push((crate_name, deps));
            }
        }
    }

    result.sort_by(|a, b| a.0.cmp(&b.0));
    result
}

#[cfg(test)]
mod tests {
    use super::{analyze_dependencies, entry_points, is_workspace};

    #[test]
    fn rust_workspace_and_dependency_analysis_are_detected() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("crates").join("app").join("src")).unwrap();
        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[workspace]\ncrates/app\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("crates").join("app").join("Cargo.toml"),
            "[package]\nname='app'\n[dependencies]\nserde = \"1\"\nanyhow = \"1\"\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("crates").join("app").join("src").join("main.rs"),
            "fn main() {}\n",
        )
        .unwrap();

        assert!(is_workspace(dir.path()));
        let entries = entry_points(dir.path());
        assert!(entries
            .iter()
            .any(|path| path.ends_with("crates/app/src/main.rs")));

        let deps = analyze_dependencies(dir.path());
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].0, "crates/app");
        assert!(deps[0].1.contains(&"serde".to_string()));
        assert!(deps[0].1.contains(&"anyhow".to_string()));
    }
}
