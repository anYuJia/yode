use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result};

use crate::protocol::{DesktopActionResult, DesktopWorktree};

pub(super) fn current_git_branch(workspace_path: &Path) -> Result<Option<String>> {
    let output = Command::new("git")
        .args(["branch", "--show-current"])
        .current_dir(workspace_path)
        .output()
        .context("无法读取当前 git 分支")?;
    if !output.status.success() {
        return Ok(None);
    }
    let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if branch.is_empty() {
        return Ok(None);
    }
    Ok(Some(branch))
}

pub(super) fn list_git_worktrees(workspace_path: &Path) -> Result<Vec<DesktopWorktree>> {
    let output = Command::new("git")
        .args(["worktree", "list", "--porcelain"])
        .current_dir(workspace_path)
        .output()
        .context("无法读取 git worktree 列表")?;
    if !output.status.success() {
        return Ok(Vec::new());
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let mut result = Vec::new();
    let mut current_path: Option<String> = None;
    let mut current_branch: Option<String> = None;
    let mut detached = false;
    for line in text.lines().chain(std::iter::once("")) {
        if let Some(path) = line.strip_prefix("worktree ") {
            if let Some(previous_path) = current_path.take() {
                result.push(worktree_record(
                    previous_path,
                    current_branch.take(),
                    detached,
                    workspace_path,
                ));
                detached = false;
            }
            current_path = Some(path.to_string());
        } else if let Some(branch) = line.strip_prefix("branch ") {
            current_branch = Some(branch.trim_start_matches("refs/heads/").to_string());
        } else if line == "detached" {
            detached = true;
        } else if line.is_empty() {
            if let Some(previous_path) = current_path.take() {
                result.push(worktree_record(
                    previous_path,
                    current_branch.take(),
                    detached,
                    workspace_path,
                ));
                detached = false;
            }
        }
    }
    Ok(result)
}

pub(super) fn prune_idle_worktrees(workspace_path: &Path) -> Result<DesktopActionResult> {
    let output = Command::new("git")
        .args(["worktree", "prune", "--verbose"])
        .current_dir(workspace_path)
        .output()
        .context("无法执行 git worktree prune")?;
    Ok(DesktopActionResult {
        ok: output.status.success(),
        message: if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if stdout.is_empty() {
                "没有需要清理的闲置工作树。".to_string()
            } else {
                stdout
            }
        } else {
            String::from_utf8_lossy(&output.stderr).trim().to_string()
        },
        path: Some(workspace_path.display().to_string()),
    })
}

pub(super) fn delete_worktree(workspace_path: &Path, path: String) -> Result<DesktopActionResult> {
    let output = Command::new("git")
        .args(["worktree", "remove", "--force", &path])
        .current_dir(workspace_path)
        .output()
        .with_context(|| format!("无法删除工作树 {}", path))?;
    Ok(DesktopActionResult {
        ok: output.status.success(),
        message: if output.status.success() {
            format!("已删除工作树 {}", path)
        } else {
            String::from_utf8_lossy(&output.stderr).trim().to_string()
        },
        path: Some(path),
    })
}

fn worktree_record(
    path: String,
    branch: Option<String>,
    detached: bool,
    workspace_path: &Path,
) -> DesktopWorktree {
    let status = if Path::new(&path) == workspace_path {
        "Active"
    } else {
        "Idle"
    };
    DesktopWorktree {
        id: path.clone(),
        branch: branch.unwrap_or_else(|| {
            if detached {
                "detached".to_string()
            } else {
                "unknown".to_string()
            }
        }),
        size: human_size(directory_size(Path::new(&path)).unwrap_or(0)),
        path,
        status: status.to_string(),
    }
}

fn directory_size(path: &Path) -> Result<u64> {
    let mut total = 0u64;
    let mut stack = vec![path.to_path_buf()];
    while let Some(path) = stack.pop() {
        let Ok(metadata) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        if metadata.is_file() {
            total = total.saturating_add(metadata.len());
        } else if metadata.is_dir() {
            let Ok(entries) = std::fs::read_dir(&path) else {
                continue;
            };
            for entry in entries.flatten() {
                stack.push(entry.path());
            }
        }
    }
    Ok(total)
}

fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut value = bytes as f64;
    let mut unit = 0usize;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{} {}", bytes, UNITS[unit])
    } else {
        format!("{:.1} {}", value, UNITS[unit])
    }
}
