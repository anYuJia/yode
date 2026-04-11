use std::path::Path;

use super::walk_files;

pub(in crate::builtin::project_map) fn build_module_tree(dir: &Path, max_depth: usize) -> String {
    let mut output = String::new();
    build_tree_recursive(dir, dir, 0, max_depth, &mut output);
    output
}

fn build_tree_recursive(
    root: &Path,
    dir: &Path,
    depth: usize,
    max_depth: usize,
    output: &mut String,
) {
    if depth > max_depth {
        return;
    }

    let mut entries: Vec<_> = match std::fs::read_dir(dir) {
        Ok(entries) => entries.filter_map(|entry| entry.ok()).collect(),
        Err(_) => return,
    };
    entries.sort_by_key(|entry| entry.file_name());

    for entry in entries {
        let path = entry.path();
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        if name_str.starts_with('.')
            || name_str == "target"
            || name_str == "node_modules"
            || name_str == "dist"
            || name_str == "__pycache__"
        {
            continue;
        }

        if path.is_dir() {
            let indent = "  ".repeat(depth);
            let rel = path.strip_prefix(root).unwrap_or(&path);
            let file_count = count_code_files(&path);
            if file_count > 0 {
                output.push_str(&format!(
                    "{}├── {} ({} files)\n",
                    indent,
                    rel.display(),
                    file_count
                ));
            } else {
                output.push_str(&format!("{}├── {}\n", indent, rel.display()));
            }

            build_tree_recursive(root, &path, depth + 1, max_depth, output);
        }
    }
}

fn count_code_files(dir: &Path) -> usize {
    let mut count = 0;
    walk_files(dir, &mut |path| {
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            if matches!(
                ext,
                "rs" | "js" | "ts" | "jsx" | "tsx" | "py" | "go" | "java"
            ) {
                count += 1;
            }
        }
    });
    count
}
