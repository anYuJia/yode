use std::path::PathBuf;

use anyhow::Result;

use super::DesktopRuntime;
use crate::protocol::{DesktopActionResult, DesktopWorktree};
use crate::worktree::{
    current_git_branch, delete_worktree, list_git_worktrees, prune_idle_worktrees,
};

impl DesktopRuntime {
    pub async fn git_current_branch(
        &self,
        workspace_path: Option<String>,
    ) -> Result<Option<String>> {
        let workspace_path = workspace_path
            .map(PathBuf::from)
            .unwrap_or_else(|| self.workspace_path.clone());
        current_git_branch(&workspace_path).await
    }

    pub async fn worktrees_list(&self) -> Result<Vec<DesktopWorktree>> {
        list_git_worktrees(&self.workspace_path).await
    }

    pub async fn worktrees_prune_idle(&self) -> Result<DesktopActionResult> {
        prune_idle_worktrees(&self.workspace_path).await
    }

    pub async fn worktree_delete(&self, path: String) -> Result<DesktopActionResult> {
        delete_worktree(&self.workspace_path, path).await
    }
}
