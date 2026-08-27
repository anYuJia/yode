use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use tokio::process::Command;
use tokio::sync::Mutex;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentWorktreeLease {
    pub id: String,
    pub repo_root: PathBuf,
    pub path: PathBuf,
    pub branch: String,
    pub base_commit: String,
    pub task: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorktreeFinalizeStatus {
    NoChanges,
    Merged,
    ParentDirty,
    Conflict,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorktreeFinalizeResult {
    pub status: WorktreeFinalizeStatus,
    pub branch: String,
    pub commit: Option<String>,
    pub message: String,
    pub worktree_path: PathBuf,
    pub retained: bool,
}

#[derive(Debug, Clone, Default)]
pub struct WorktreeCoordinator;

impl WorktreeCoordinator {
    pub async fn allocate(repo: &Path, task: &str) -> Result<AgentWorktreeLease> {
        let repo_root = git_output(repo, &["rev-parse", "--show-toplevel"]).await?;
        let repo_root = PathBuf::from(repo_root.trim());
        let base_commit = git_output(&repo_root, &["rev-parse", "HEAD"]).await?;
        let id = Uuid::new_v4().to_string();
        let short = &id[..8];
        let slug = sanitize_slug(task);
        let branch = format!(
            "yode/agent/{}-{}",
            if slug.is_empty() { "task" } else { &slug },
            short
        );
        let path = repo_root
            .join(".yode")
            .join("agent-worktrees")
            .join(format!(
                "{}-{}",
                if slug.is_empty() { "task" } else { &slug },
                short
            ));
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        git_status(
            &repo_root,
            &[
                "worktree",
                "add",
                "-b",
                &branch,
                &path.display().to_string(),
                "HEAD",
            ],
        )
        .await
        .with_context(|| format!("failed to allocate worktree for {task}"))?;
        Ok(AgentWorktreeLease {
            id,
            repo_root,
            path,
            branch,
            base_commit: base_commit.trim().to_string(),
            task: task.to_string(),
        })
    }

    pub async fn finalize(
        lease: &AgentWorktreeLease,
        commit_message: &str,
        remove_after_merge: bool,
    ) -> Result<WorktreeFinalizeResult> {
        let _merge_guard = merge_lock().lock().await;
        ensure_lease_valid(lease).await?;
        let changes = git_output(&lease.path, &["status", "--porcelain=v1"]).await?;
        if changes.trim().is_empty() {
            if remove_after_merge {
                cleanup_lease(lease).await?;
            }
            return Ok(WorktreeFinalizeResult {
                status: WorktreeFinalizeStatus::NoChanges,
                branch: lease.branch.clone(),
                commit: None,
                message: "Agent worktree produced no changes.".to_string(),
                worktree_path: lease.path.clone(),
                retained: !remove_after_merge,
            });
        }

        git_status(&lease.path, &["add", "-A"]).await?;
        let message = if commit_message.trim().is_empty() {
            format!("yode(agent): {}", lease.task)
        } else {
            commit_message.trim().to_string()
        };
        git_status(&lease.path, &["commit", "-m", &message])
            .await
            .context("failed to commit isolated agent changes")?;
        let commit = git_output(&lease.path, &["rev-parse", "HEAD"]).await?;
        let commit = commit.trim().to_string();

        let parent_dirty = git_output(&lease.repo_root, &["status", "--porcelain=v1"]).await?;
        if !parent_dirty.trim().is_empty() {
            return Ok(WorktreeFinalizeResult {
                status: WorktreeFinalizeStatus::ParentDirty,
                branch: lease.branch.clone(),
                commit: Some(commit),
                message: "Parent worktree is dirty; merge refused and agent branch retained."
                    .to_string(),
                worktree_path: lease.path.clone(),
                retained: true,
            });
        }

        let merge_message = format!("merge: integrate agent worktree {}", lease.branch);
        let merge = Command::new("git")
            .args(["merge", "--no-ff", &lease.branch, "-m", &merge_message])
            .current_dir(&lease.repo_root)
            .output()
            .await
            .context("failed to start agent worktree merge")?;
        if !merge.status.success() {
            let conflict = git_output(
                &lease.repo_root,
                &["diff", "--name-only", "--diff-filter=U"],
            )
            .await
            .unwrap_or_default();
            let _ = git_status(&lease.repo_root, &["merge", "--abort"]).await;
            return Ok(WorktreeFinalizeResult {
                status: if conflict.trim().is_empty() {
                    WorktreeFinalizeStatus::Failed
                } else {
                    WorktreeFinalizeStatus::Conflict
                },
                branch: lease.branch.clone(),
                commit: Some(commit),
                message: if conflict.trim().is_empty() {
                    format!(
                        "Agent merge failed; branch retained: {}",
                        String::from_utf8_lossy(&merge.stderr).trim()
                    )
                } else {
                    format!(
                        "Agent merge conflicted in: {}. Merge aborted; branch retained.",
                        conflict.lines().collect::<Vec<_>>().join(", ")
                    )
                },
                worktree_path: lease.path.clone(),
                retained: true,
            });
        }

        if remove_after_merge {
            cleanup_lease(lease).await?;
        }
        Ok(WorktreeFinalizeResult {
            status: WorktreeFinalizeStatus::Merged,
            branch: lease.branch.clone(),
            commit: Some(commit),
            message: "Agent branch merged into the parent worktree.".to_string(),
            worktree_path: lease.path.clone(),
            retained: !remove_after_merge,
        })
    }

    pub fn should_auto_isolate(
        run_in_background: bool,
        team_id: Option<&str>,
        allowed_tools: &[String],
    ) -> bool {
        if !(run_in_background || team_id.is_some()) {
            return false;
        }
        allowed_tools.iter().any(|tool| is_mutating_tool(tool))
    }
}

fn is_mutating_tool(name: &str) -> bool {
    matches!(
        name,
        "bash"
            | "write_file"
            | "edit_file"
            | "multi_edit"
            | "git_commit"
            | "git_checkout"
            | "worktree"
            | "notebook_edit"
            | "apply_patch"
    ) || name.starts_with("git_") && !matches!(name, "git_status" | "git_log" | "git_diff")
}

async fn ensure_lease_valid(lease: &AgentWorktreeLease) -> Result<()> {
    if !lease.path.is_dir() {
        bail!("agent worktree no longer exists: {}", lease.path.display());
    }
    let actual_root = git_output(&lease.path, &["rev-parse", "--show-toplevel"]).await?;
    let actual_root = PathBuf::from(actual_root.trim());
    let expected = lease
        .path
        .canonicalize()
        .unwrap_or_else(|_| lease.path.clone());
    let actual = actual_root.canonicalize().unwrap_or(actual_root);
    if actual != expected {
        bail!("agent worktree root changed unexpectedly");
    }
    let branch = git_output(&lease.path, &["branch", "--show-current"]).await?;
    if branch.trim() != lease.branch {
        bail!(
            "agent worktree branch changed from '{}' to '{}'",
            lease.branch,
            branch.trim()
        );
    }
    Ok(())
}

async fn cleanup_lease(lease: &AgentWorktreeLease) -> Result<()> {
    git_status(
        &lease.repo_root,
        &["worktree", "remove", &lease.path.display().to_string()],
    )
    .await?;
    git_status(&lease.repo_root, &["branch", "-D", &lease.branch]).await?;
    Ok(())
}

async fn git_output(cwd: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .await?;
    if !output.status.success() {
        bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

async fn git_status(cwd: &Path, args: &[&str]) -> Result<()> {
    git_output(cwd, args).await.map(|_| ())
}

fn merge_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn sanitize_slug(raw: &str) -> String {
    raw.chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .take(6)
        .collect::<Vec<_>>()
        .join("-")
        .chars()
        .take(36)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_isolation_only_for_parallel_mutating_agents() {
        assert!(WorktreeCoordinator::should_auto_isolate(
            true,
            None,
            &["edit_file".into()]
        ));
        assert!(WorktreeCoordinator::should_auto_isolate(
            false,
            Some("team"),
            &["bash".into()]
        ));
        assert!(!WorktreeCoordinator::should_auto_isolate(
            true,
            None,
            &["read_file".into()]
        ));
        assert!(!WorktreeCoordinator::should_auto_isolate(
            false,
            None,
            &["edit_file".into()]
        ));
    }

    #[test]
    fn slug_is_branch_safe() {
        assert_eq!(sanitize_slug("Fix UI / Settings!!!"), "fix-ui-settings");
    }
}
