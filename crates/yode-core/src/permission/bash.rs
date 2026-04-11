use super::classifier::bash_risk_rationale;
use super::{CommandClassifier, CommandRiskLevel, PermissionAction};

pub(crate) struct BashDiscoveryRedirect {
    pub(crate) command_name: &'static str,
    pub(crate) alternative: &'static str,
}

pub(crate) fn discovery_redirect(command_lower: &str) -> Option<BashDiscoveryRedirect> {
    let forbidden_binaries = ["find", "grep", "rg", "ag", "ack"];
    let forbidden_match = forbidden_binaries.iter().find_map(|binary| {
        let pattern = format!(r"(\s|^|&&|;|\|){}(\s|$)", binary);
        regex::Regex::new(&pattern)
            .ok()
            .filter(|regex| regex.is_match(command_lower))
            .map(|_| *binary)
    });

    if let Some(binary) = forbidden_match {
        let alternative = if binary == "find" { "glob" } else { "grep" };
        return Some(BashDiscoveryRedirect {
            command_name: binary,
            alternative,
        });
    }

    let is_recursive_ls = command_lower.contains("ls ")
        && (command_lower.contains("-r") || command_lower.contains("-lar"));
    if is_recursive_ls {
        return Some(BashDiscoveryRedirect {
            command_name: "ls -R",
            alternative: "ls (without -R) or project_map",
        });
    }

    None
}

pub(crate) fn auto_mode_bash_decision(
    command: &str,
) -> Option<(PermissionAction, CommandRiskLevel, String)> {
    let risk = CommandClassifier::classify(command);
    match risk {
        CommandRiskLevel::Safe => Some((
            PermissionAction::Allow,
            risk,
            format!(
                "Auto mode classifier marked this bash command as safe. {}",
                bash_risk_rationale(command, risk)
            ),
        )),
        CommandRiskLevel::Destructive => Some((
            PermissionAction::Deny,
            risk,
            format!(
                "Auto mode classifier marked this bash command as destructive. {}",
                bash_risk_rationale(command, risk)
            ),
        )),
        CommandRiskLevel::PotentiallyRisky => Some((
            PermissionAction::Confirm,
            risk,
            format!(
                "Auto mode classifier marked this bash command as potentially risky. {}",
                bash_risk_rationale(command, risk)
            ),
        )),
        CommandRiskLevel::Unknown => None,
    }
}

pub(crate) fn destructive_guard_reason() -> &'static str {
    "Dangerous bash command blocked by destructive-command guard. Use a safer non-destructive probe first."
}

pub(crate) fn destructive_guard_suggestion() -> &'static str {
    "This command is classified as destructive and cannot be executed. Stop and propose a safer fallback such as `git status`, `git diff`, `ls`, or a dry-run variant before attempting any mutation again."
}

pub(crate) fn command_prefix(command: &str) -> Option<String> {
    let parts = command
        .split_whitespace()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    let first = parts.first()?;
    let second = parts.get(1).copied();
    let prefix = match (*first, second) {
        ("git", Some(subcommand)) => format!("git {}", subcommand),
        ("cargo", Some(subcommand)) => format!("cargo {}", subcommand),
        ("go", Some(subcommand)) => format!("go {}", subcommand),
        ("npm", Some(subcommand)) => format!("npm {}", subcommand),
        ("pnpm", Some(subcommand)) => format!("pnpm {}", subcommand),
        ("yarn", Some(subcommand)) => format!("yarn {}", subcommand),
        ("uv", Some(subcommand)) => format!("uv {}", subcommand),
        _ => first.to_string(),
    };
    Some(prefix)
}

pub(crate) fn safe_readonly_prefixes() -> &'static [&'static str] {
    &[
        "pwd",
        "ls",
        "git status",
        "git diff",
        "git log",
        "cargo check",
        "cargo test --help",
        "go test -run",
        "npm test -- --help",
    ]
}
