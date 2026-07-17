use anyhow::Result;
use serde_json::json;

use super::DesktopRuntime;
use crate::desktop_settings_store::{read_desktop_settings_async, write_desktop_settings_async};
use crate::git_settings::{
    apply_git_settings_env, git_settings_from_desktop_settings, normalize_git_settings,
    validate_git_settings,
};
use crate::protocol::GitSettings;

impl DesktopRuntime {
    pub async fn git_settings_get(&self) -> Result<GitSettings> {
        git_settings_from_desktop_settings(&read_desktop_settings_async().await?)
    }

    pub async fn git_settings_apply(&self, settings: GitSettings) -> Result<GitSettings> {
        let normalized = normalize_git_settings(settings);
        validate_git_settings(&normalized)?;
        let mut desktop_settings = read_desktop_settings_async().await?;
        desktop_settings.insert(
            "yode-git-branch-prefix".to_string(),
            json!(normalized.branch_prefix),
        );
        desktop_settings.insert(
            "yode-git-merge-method".to_string(),
            json!(normalized.merge_method),
        );
        desktop_settings.insert(
            "yode-git-show-pr-icons".to_string(),
            json!(normalized.show_pr_icons),
        );
        desktop_settings.insert(
            "yode-git-always-force-push".to_string(),
            json!(normalized.always_force_push),
        );
        desktop_settings.insert(
            "yode-git-create-draft-prs".to_string(),
            json!(normalized.create_draft_prs),
        );
        desktop_settings.insert(
            "yode-git-auto-delete-worktrees".to_string(),
            json!(normalized.auto_delete_worktrees),
        );
        desktop_settings.insert(
            "yode-git-auto-delete-limit".to_string(),
            json!(normalized.auto_delete_limit),
        );
        desktop_settings.insert(
            "yode-git-commit-instructions".to_string(),
            json!(normalized.commit_instructions),
        );
        desktop_settings.insert(
            "yode-git-pr-instructions".to_string(),
            json!(normalized.pr_instructions),
        );
        write_desktop_settings_async(&desktop_settings).await?;
        apply_git_settings_env(&normalized);
        Ok(normalized)
    }
}
