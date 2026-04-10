#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandRiskLevel {
    /// Safe read-only commands (ls, cat, grep, git status)
    Safe,
    /// Unknown risk level
    Unknown,
    /// Potentially risky (git push, npm install)
    PotentiallyRisky,
    /// Dangerous/destructive (rm -rf /, DROP TABLE)
    Destructive,
}

const DESTRUCTIVE_PATTERNS: &[&str] = &[
    "rm -rf /",
    "rm -rf /*",
    "mkfs",
    "dd if=/dev/zero",
    ":(){:|:&};:",
    "> /dev/sda",
    "chmod -R 777 /",
    "mv /* /dev/null",
];

const RISKY_PATTERNS: &[&str] = &[
    "git push --force",
    "git push -f",
    "git reset --hard",
    "git clean -fd",
    "git clean -fxd",
    "git checkout -- .",
    "drop table",
    "delete from",
    "truncate table",
    "npm publish",
    "cargo publish",
    "curl|sh",
    "curl|bash",
    "wget|sh",
    "wget|bash",
    "pip install",
    "npm install -g",
    "sudo",
    "chmod -R",
    "chown -R",
];

const SAFE_PREFIXES: &[&str] = &[
    "ls",
    "cat",
    "head",
    "tail",
    "grep",
    "rg",
    "find",
    "which",
    "whoami",
    "pwd",
    "echo",
    "date",
    "wc",
    "sort",
    "uniq",
    "tr",
    "tee",
    "git status",
    "git log",
    "git diff",
    "git branch",
    "git show",
    "git remote -v",
    "git tag",
    "git stash list",
    "cargo check",
    "cargo clippy",
    "cargo test",
    "cargo doc",
    "cargo metadata",
    "cargo tree",
    "rustc --version",
    "rustup show",
    "node --version",
    "npm --version",
    "bun --version",
    "python --version",
    "python3 --version",
    "go version",
    "uname",
    "env",
    "printenv",
    "file ",
    "stat ",
    "df -h",
    "du -sh",
    "ps aux",
    "top -l 1",
];

pub struct CommandClassifier;

impl CommandClassifier {
    /// Classify a bash command's risk level.
    pub fn classify(command: &str) -> CommandRiskLevel {
        let cmd_lower = command.to_lowercase().trim().to_string();

        for pattern in DESTRUCTIVE_PATTERNS {
            if cmd_lower.contains(pattern) {
                return CommandRiskLevel::Destructive;
            }
        }

        for pattern in RISKY_PATTERNS {
            if cmd_lower.contains(pattern) {
                return CommandRiskLevel::PotentiallyRisky;
            }
        }

        if (cmd_lower.contains("curl ") || cmd_lower.contains("wget "))
            && (cmd_lower.contains("| sh")
                || cmd_lower.contains("| bash")
                || cmd_lower.contains("|sh")
                || cmd_lower.contains("|bash"))
        {
            return CommandRiskLevel::Destructive;
        }

        for prefix in SAFE_PREFIXES {
            if cmd_lower.starts_with(prefix) {
                return CommandRiskLevel::Safe;
            }
        }

        CommandRiskLevel::Unknown
    }
}

pub(crate) fn bash_risk_rationale(command: &str, risk: CommandRiskLevel) -> &'static str {
    let cmd_lower = command.to_lowercase();
    match risk {
        CommandRiskLevel::Destructive => {
            if cmd_lower.contains("rm -rf") {
                "It performs recursive deletion and may irreversibly remove files."
            } else if cmd_lower.contains("git reset --hard")
                || cmd_lower.contains("git checkout --")
            {
                "It discards local changes and can permanently destroy uncommitted work."
            } else if (cmd_lower.contains("curl ") || cmd_lower.contains("wget "))
                && (cmd_lower.contains("| sh")
                    || cmd_lower.contains("| bash")
                    || cmd_lower.contains("|sh")
                    || cmd_lower.contains("|bash"))
            {
                "It pipes remote content directly into a shell, which is high-risk code execution."
            } else {
                "It matches a destructive command pattern that can cause irreversible changes."
            }
        }
        CommandRiskLevel::PotentiallyRisky => {
            if cmd_lower.contains("git push --force") || cmd_lower.contains("git push -f") {
                "It rewrites remote history and can disrupt collaborators."
            } else if cmd_lower.contains("cargo publish") || cmd_lower.contains("npm publish") {
                "It publishes artifacts externally and may have irreversible distribution effects."
            } else if cmd_lower.contains("sudo") {
                "It escalates privileges and can bypass normal workspace safety boundaries."
            } else {
                "It matches a risky command pattern that can mutate state outside a safe read-only flow."
            }
        }
        CommandRiskLevel::Safe => "It matches a safe read-only command prefix.",
        CommandRiskLevel::Unknown => "Its safety could not be classified confidently.",
    }
}
