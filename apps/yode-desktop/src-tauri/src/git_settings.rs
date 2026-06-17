use anyhow::Result;

use crate::desktop_settings_store::{
    desktop_bool_setting, desktop_string_setting, desktop_u32_setting,
};
use crate::protocol::GitSettings;

pub(super) fn git_settings_from_desktop_settings(
    settings: &serde_json::Map<String, serde_json::Value>,
) -> Result<GitSettings> {
    Ok(normalize_git_settings(GitSettings {
        branch_prefix: desktop_string_setting(settings, "yode-git-branch-prefix", "yode/"),
        merge_method: desktop_string_setting(settings, "yode-git-merge-method", "merge"),
        show_pr_icons: desktop_bool_setting(settings, "yode-git-show-pr-icons", true),
        always_force_push: desktop_bool_setting(settings, "yode-git-always-force-push", false),
        create_draft_prs: desktop_bool_setting(settings, "yode-git-create-draft-prs", true),
        auto_delete_worktrees: desktop_bool_setting(
            settings,
            "yode-git-auto-delete-worktrees",
            true,
        ),
        auto_delete_limit: desktop_u32_setting(settings, "yode-git-auto-delete-limit", 15),
        commit_instructions: desktop_string_setting(settings, "yode-git-commit-instructions", ""),
        pr_instructions: desktop_string_setting(settings, "yode-git-pr-instructions", ""),
    }))
}

pub(super) fn normalize_git_settings(mut settings: GitSettings) -> GitSettings {
    settings.branch_prefix = normalize_branch_prefix(&settings.branch_prefix);
    settings.merge_method = match settings.merge_method.trim() {
        "squash" => "squash".to_string(),
        _ => "merge".to_string(),
    };
    settings.auto_delete_limit = settings.auto_delete_limit.clamp(1, 200);
    settings.commit_instructions = settings.commit_instructions.trim().to_string();
    settings.pr_instructions = settings.pr_instructions.trim().to_string();
    settings
}

pub(super) fn validate_git_settings(settings: &GitSettings) -> Result<()> {
    if settings.branch_prefix.is_empty() {
        anyhow::bail!("分支前缀不能为空。");
    }
    if !settings
        .branch_prefix
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '/' | '-' | '_' | '.'))
    {
        anyhow::bail!("分支前缀只能包含字母、数字、斜杠、横线、下划线和点。");
    }
    if !matches!(settings.merge_method.as_str(), "merge" | "squash") {
        anyhow::bail!("无效的合并方式。");
    }
    Ok(())
}

pub(super) fn apply_git_settings_env(settings: &GitSettings) {
    if let Ok(json) = serde_json::to_string(settings) {
        std::env::set_var("YODE_GIT_SETTINGS", json);
    }
}

fn normalize_branch_prefix(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return "yode/".to_string();
    }
    if trimmed.ends_with('/') || trimmed.ends_with('-') || trimmed.ends_with('_') {
        trimmed.to_string()
    } else {
        format!("{trimmed}/")
    }
}

#[cfg(test)]
mod tests {
    use crate::protocol::GitSettings;

    use super::*;

    fn git_settings(branch_prefix: &str, merge_method: &str, limit: u32) -> GitSettings {
        GitSettings {
            branch_prefix: branch_prefix.to_string(),
            merge_method: merge_method.to_string(),
            show_pr_icons: true,
            always_force_push: false,
            create_draft_prs: true,
            auto_delete_worktrees: true,
            auto_delete_limit: limit,
            commit_instructions: "  commit notes  ".to_string(),
            pr_instructions: "  pr notes  ".to_string(),
        }
    }

    #[test]
    fn git_settings_normalize_prefix_merge_and_limit() {
        let settings = normalize_git_settings(git_settings("feature", "squash", 300));

        assert_eq!(settings.branch_prefix, "feature/");
        assert_eq!(settings.merge_method, "squash");
        assert_eq!(settings.auto_delete_limit, 200);
        assert_eq!(settings.commit_instructions, "commit notes");
        assert_eq!(settings.pr_instructions, "pr notes");
    }

    #[test]
    fn git_settings_validation_rejects_invalid_prefix() {
        let settings = normalize_git_settings(git_settings("bad prefix", "merge", 15));

        assert!(validate_git_settings(&settings).is_err());
    }
}
