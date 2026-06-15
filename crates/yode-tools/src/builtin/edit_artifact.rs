use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};

use crate::tool::ToolContext;

pub(crate) struct EditDiffArtifact {
    pub path: Option<String>,
    pub added_lines: usize,
    pub removed_lines: usize,
}

pub(crate) async fn persist_edit_diff_artifact(
    ctx: &ToolContext,
    file_path: &str,
    removed: &[String],
    added: &[String],
) -> EditDiffArtifact {
    let removed_lines = removed.len();
    let added_lines = added.len();
    let Some(root) = ctx.working_dir.as_ref() else {
        return EditDiffArtifact {
            path: None,
            added_lines,
            removed_lines,
        };
    };

    let dir = root.join(".yode").join("edit-diffs");
    if tokio::fs::create_dir_all(&dir).await.is_err() {
        return EditDiffArtifact {
            path: None,
            added_lines,
            removed_lines,
        };
    }

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    let slug = sanitize_path_segment(file_path);
    let path = dir.join(format!("{timestamp}-{slug}.diff"));
    let content = render_simple_diff(file_path, removed, added);

    match tokio::fs::write(&path, content).await {
        Ok(()) => EditDiffArtifact {
            path: Some(display_artifact_path(root, &path)),
            added_lines,
            removed_lines,
        },
        Err(_) => EditDiffArtifact {
            path: None,
            added_lines,
            removed_lines,
        },
    }
}

pub(crate) fn diff_artifact_metadata(artifact: EditDiffArtifact) -> Value {
    json!({
        "diff_artifact_path": artifact.path,
        "full_added_line_count": artifact.added_lines,
        "full_removed_line_count": artifact.removed_lines,
    })
}

fn render_simple_diff(file_path: &str, removed: &[String], added: &[String]) -> String {
    let mut output = String::new();
    output.push_str(&format!("--- {file_path}\n"));
    output.push_str(&format!("+++ {file_path}\n"));
    output.push_str("@@ edit preview artifact @@\n");
    for line in removed {
        output.push('-');
        output.push_str(line);
        output.push('\n');
    }
    for line in added {
        output.push('+');
        output.push_str(line);
        output.push('\n');
    }
    output
}

fn display_artifact_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .map(|relative| relative.display().to_string())
        .unwrap_or_else(|_| path.display().to_string())
}

fn sanitize_path_segment(file_path: &str) -> String {
    let name = PathBuf::from(file_path)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("edit")
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '.' || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>();
    if name.trim_matches('-').is_empty() {
        "edit".to_string()
    } else {
        name
    }
}
