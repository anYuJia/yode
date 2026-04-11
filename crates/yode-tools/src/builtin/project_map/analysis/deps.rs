use std::path::Path;

use super::{ProjectType, walk_files};

pub(in crate::builtin::project_map) fn analyze_dependencies(
    dir: &Path,
    project_type: &ProjectType,
) -> Vec<(String, Vec<String>)> {
    match project_type {
        ProjectType::Rust | ProjectType::RustWorkspace => analyze_rust_deps(dir),
        _ => Vec::new(),
    }
}

fn analyze_rust_deps(dir: &Path) -> Vec<(String, Vec<String>)> {
    let mut result = Vec::new();
    let mut cargo_files = Vec::new();
    walk_files(dir, &mut |path| {
        if path
            .file_name()
            .map(|name| name == "Cargo.toml")
            .unwrap_or(false)
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
                if trimmed.starts_with("[dependencies]")
                    || trimmed.starts_with("[dev-dependencies]")
                {
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
