use std::path::Path;

use yode_core::engine::EngineRuntimeState;
use yode_core::permission::{PermissionRule, RuleBehavior, RuleSource};

use super::artifact_preview::preview_markdown;

#[allow(dead_code)]
pub(crate) fn hook_failure_summary(state: &EngineRuntimeState) -> String {
    format!(
        "hook failure: {} [{}] {} / timeout={} ({})",
        state.last_hook_failure_command.as_deref().unwrap_or("none"),
        state.last_hook_failure_event.as_deref().unwrap_or("none"),
        state.last_hook_failure_reason.as_deref().unwrap_or("none"),
        state.hook_timeout_count,
        state.last_hook_timeout_command.as_deref().unwrap_or("none"),
    )
}

pub(crate) fn preview_runtime_artifact(path: Option<&str>, section_hint: &str) -> String {
    let Some(path) = path else {
        return "none".to_string();
    };
    preview_markdown(Path::new(path), section_hint).unwrap_or_else(|| {
        std::fs::read_to_string(path)
            .ok()
            .map(|content| {
                content
                    .lines()
                    .map(str::trim)
                    .filter(|line| !line.is_empty())
                    .take(3)
                    .collect::<Vec<_>>()
                    .join(" | ")
            })
            .filter(|preview| !preview.is_empty())
            .unwrap_or_else(|| "none".to_string())
    })
}

#[allow(dead_code)]
pub(crate) fn permission_rule_diff_summary(rules: &[PermissionRule]) -> String {
    if rules.is_empty() {
        return "none".to_string();
    }

    let mut allow = 0usize;
    let mut deny = 0usize;
    let mut ask = 0usize;
    let mut session = 0usize;
    let mut user = 0usize;
    let mut project = 0usize;
    let mut cli = 0usize;

    for rule in rules {
        match rule.behavior {
            RuleBehavior::Allow => allow += 1,
            RuleBehavior::Deny => deny += 1,
            RuleBehavior::Ask => ask += 1,
        }
        match rule.source {
            RuleSource::Session => session += 1,
            RuleSource::UserConfig => user += 1,
            RuleSource::ProjectConfig => project += 1,
            RuleSource::CliArg => cli += 1,
        }
    }

    format!(
        "allow={} deny={} ask={} / session={} user={} project={} cli={}",
        allow, deny, ask, session, user, project, cli
    )
}

#[allow(dead_code)]
pub(crate) fn repeated_denial_recovery_hint(
    denial_prefixes: &[String],
    suggestions: &[String],
) -> String {
    if denial_prefixes.is_empty() && suggestions.is_empty() {
        return "none".to_string();
    }

    let prefix_summary = if denial_prefixes.is_empty() {
        "no grouped prefixes".to_string()
    } else {
        denial_prefixes
            .iter()
            .take(2)
            .cloned()
            .collect::<Vec<_>>()
            .join(" | ")
    };
    let suggestion_summary = if suggestions.is_empty() {
        "no rule suggestions".to_string()
    } else {
        suggestions
            .iter()
            .take(2)
            .cloned()
            .collect::<Vec<_>>()
            .join(" | ")
    };

    format!("prefixes: {} / suggestions: {}", prefix_summary, suggestion_summary)
}

#[cfg(test)]
mod tests {
    use yode_core::permission::{PermissionRule, RuleBehavior, RuleSource};

    use super::{
        permission_rule_diff_summary, repeated_denial_recovery_hint,
    };

    #[test]
    fn permission_rule_diff_summary_counts_behavior_and_sources() {
        let summary = permission_rule_diff_summary(&[
            PermissionRule {
                source: RuleSource::Session,
                behavior: RuleBehavior::Allow,
                tool_name: "bash".to_string(),
                pattern: None,
            },
            PermissionRule {
                source: RuleSource::UserConfig,
                behavior: RuleBehavior::Deny,
                tool_name: "edit_file".to_string(),
                pattern: None,
            },
            PermissionRule {
                source: RuleSource::CliArg,
                behavior: RuleBehavior::Ask,
                tool_name: "write_file".to_string(),
                pattern: None,
            },
        ]);
        assert!(summary.contains("allow=1"));
        assert!(summary.contains("deny=1"));
        assert!(summary.contains("ask=1"));
        assert!(summary.contains("session=1"));
        assert!(summary.contains("user=1"));
        assert!(summary.contains("cli=1"));
    }

    #[test]
    fn repeated_denial_recovery_hint_compacts_inputs() {
        let hint = repeated_denial_recovery_hint(
            &["git push -> count=3".to_string()],
            &["git push x3 -> consider allow rule".to_string()],
        );
        assert!(hint.contains("prefixes:"));
        assert!(hint.contains("suggestions:"));
    }
}
