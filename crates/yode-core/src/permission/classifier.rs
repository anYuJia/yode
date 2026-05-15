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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandSemanticCategory {
    ReadOnly,
    PackageInstall,
    Network,
    GitMutating,
    Destructive,
    Interactive,
    Unknown,
}

impl CommandSemanticCategory {
    pub fn label(self) -> &'static str {
        match self {
            Self::ReadOnly => "read-only",
            Self::PackageInstall => "package-install",
            Self::Network => "network",
            Self::GitMutating => "git-mutating",
            Self::Destructive => "destructive",
            Self::Interactive => "interactive",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandSemanticAnalysis {
    pub category: CommandSemanticCategory,
    pub risk: CommandRiskLevel,
    pub segment: String,
    pub reason: &'static str,
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

const READ_ONLY_COMMANDS: &[&str] = &[
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

const GIT_READ_ONLY_SUBCOMMANDS: &[&str] = &[
    "status",
    "log",
    "diff",
    "show",
    "branch",
    "remote",
    "tag",
    "rev-parse",
    "ls-files",
    "grep",
];

const GIT_MUTATING_SUBCOMMANDS: &[&str] = &[
    "add",
    "am",
    "apply",
    "bisect",
    "checkout",
    "cherry-pick",
    "clean",
    "commit",
    "merge",
    "mv",
    "pull",
    "push",
    "rebase",
    "reset",
    "restore",
    "revert",
    "rm",
    "stash",
    "switch",
    "worktree",
];

const PACKAGE_INSTALL_COMMANDS: &[&str] = &[
    "apt", "apt-get", "brew", "cargo", "gem", "go", "npm", "pip", "pip3", "pnpm", "uv", "yarn",
];

const NETWORK_COMMANDS: &[&str] = &[
    "curl", "wget", "ssh", "scp", "sftp", "rsync", "nc", "netcat", "telnet",
];

const INTERACTIVE_COMMANDS: &[&str] = &[
    "vim", "vi", "nano", "emacs", "less", "more", "top", "htop", "python", "python3", "node",
    "irb", "psql", "mysql",
];

pub struct CommandClassifier;

impl CommandClassifier {
    /// Classify a bash command's risk level.
    pub fn classify(command: &str) -> CommandRiskLevel {
        Self::analyze(command).risk
    }

    pub fn analyze(command: &str) -> CommandSemanticAnalysis {
        let mut best: Option<CommandSemanticAnalysis> = None;

        for segment in split_command_segments(command) {
            let analysis = classify_segment(&segment, command);
            if best
                .as_ref()
                .is_none_or(|current| risk_rank(analysis.risk) > risk_rank(current.risk))
            {
                best = Some(analysis);
            }
        }

        best.unwrap_or_else(|| CommandSemanticAnalysis {
            category: CommandSemanticCategory::Unknown,
            risk: CommandRiskLevel::Unknown,
            segment: command.trim().to_string(),
            reason: "empty command",
        })
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

pub(crate) fn bash_semantic_rationale(analysis: &CommandSemanticAnalysis) -> &'static str {
    match analysis.category {
        CommandSemanticCategory::ReadOnly => {
            "All command segments matched structured read-only semantics."
        }
        CommandSemanticCategory::PackageInstall => {
            "It installs packages or dependencies and can change the workspace or system."
        }
        CommandSemanticCategory::Network => {
            "It opens a network connection and may fetch or transmit external content."
        }
        CommandSemanticCategory::GitMutating => {
            "It mutates git state and can change local or remote repository history."
        }
        CommandSemanticCategory::Destructive => analysis.reason,
        CommandSemanticCategory::Interactive => {
            "It is likely to wait for interactive input or take over the terminal."
        }
        CommandSemanticCategory::Unknown => "Its safety could not be classified confidently.",
    }
}

fn classify_segment(segment: &str, full_command: &str) -> CommandSemanticAnalysis {
    let segment = segment.trim();
    let cmd_lower = segment.to_lowercase();
    let full_lower = full_command.to_lowercase();

    if pipes_remote_content_to_shell(&full_lower) {
        return segment_analysis(
            CommandSemanticCategory::Destructive,
            CommandRiskLevel::Destructive,
            segment,
            "remote content is piped directly into a shell",
        );
    }

    for pattern in DESTRUCTIVE_PATTERNS {
        if cmd_lower.contains(pattern) {
            return segment_analysis(
                CommandSemanticCategory::Destructive,
                CommandRiskLevel::Destructive,
                segment,
                "matches a destructive command pattern",
            );
        }
    }

    let tokens = tokenize_segment(segment);
    let Some(command) = command_name(&tokens) else {
        return segment_analysis(
            CommandSemanticCategory::Unknown,
            CommandRiskLevel::Unknown,
            segment,
            "empty command segment",
        );
    };

    if command == "rm" && has_recursive_force_flags(&tokens) {
        return segment_analysis(
            CommandSemanticCategory::Destructive,
            CommandRiskLevel::Destructive,
            segment,
            "recursive forced deletion requires an explicit destructive confirmation",
        );
    }

    if command == "git" {
        return classify_git_segment(segment, &tokens);
    }

    if is_sed_or_awk_file_edit(&command, &tokens, segment) {
        return segment_analysis(
            CommandSemanticCategory::Destructive,
            CommandRiskLevel::Destructive,
            segment,
            "sed/awk file edits should use edit_file instead of shell mutation",
        );
    }

    if is_package_install(&command, &tokens) {
        return segment_analysis(
            CommandSemanticCategory::PackageInstall,
            CommandRiskLevel::PotentiallyRisky,
            segment,
            "package installation mutates dependency state",
        );
    }

    if NETWORK_COMMANDS.contains(&command.as_str()) {
        return segment_analysis(
            CommandSemanticCategory::Network,
            CommandRiskLevel::PotentiallyRisky,
            segment,
            "network command",
        );
    }

    if INTERACTIVE_COMMANDS.contains(&command.as_str()) {
        return segment_analysis(
            CommandSemanticCategory::Interactive,
            CommandRiskLevel::PotentiallyRisky,
            segment,
            "interactive command",
        );
    }

    for pattern in RISKY_PATTERNS {
        if cmd_lower.contains(pattern) {
            return segment_analysis(
                CommandSemanticCategory::Unknown,
                CommandRiskLevel::PotentiallyRisky,
                segment,
                "matches a risky mutation pattern",
            );
        }
    }

    if is_read_only_segment(&command, &tokens, &cmd_lower) {
        return segment_analysis(
            CommandSemanticCategory::ReadOnly,
            CommandRiskLevel::Safe,
            segment,
            "structured read-only command",
        );
    }

    segment_analysis(
        CommandSemanticCategory::Unknown,
        CommandRiskLevel::Unknown,
        segment,
        "no structured rule matched",
    )
}

fn classify_git_segment(segment: &str, tokens: &[String]) -> CommandSemanticAnalysis {
    let subcommand = tokens.get(1).map(|value| value.as_str()).unwrap_or("");
    if subcommand == "reset" && tokens.iter().any(|token| token == "--hard") {
        return segment_analysis(
            CommandSemanticCategory::Destructive,
            CommandRiskLevel::Destructive,
            segment,
            "git reset --hard discards uncommitted work",
        );
    }
    if subcommand == "clean" && tokens.iter().any(|token| token.contains('f')) {
        return segment_analysis(
            CommandSemanticCategory::Destructive,
            CommandRiskLevel::Destructive,
            segment,
            "git clean with force can delete untracked files",
        );
    }
    if subcommand == "push"
        && tokens
            .iter()
            .any(|token| matches!(token.as_str(), "--force" | "--force-with-lease" | "-f"))
    {
        return segment_analysis(
            CommandSemanticCategory::GitMutating,
            CommandRiskLevel::PotentiallyRisky,
            segment,
            "force push rewrites remote history",
        );
    }
    if GIT_READ_ONLY_SUBCOMMANDS.contains(&subcommand)
        || (subcommand == "stash" && tokens.get(2).is_some_and(|token| token == "list"))
    {
        return segment_analysis(
            CommandSemanticCategory::ReadOnly,
            CommandRiskLevel::Safe,
            segment,
            "git read-only subcommand",
        );
    }
    if GIT_MUTATING_SUBCOMMANDS.contains(&subcommand) {
        return segment_analysis(
            CommandSemanticCategory::GitMutating,
            CommandRiskLevel::PotentiallyRisky,
            segment,
            "git mutating subcommand",
        );
    }
    segment_analysis(
        CommandSemanticCategory::Unknown,
        CommandRiskLevel::Unknown,
        segment,
        "unknown git subcommand",
    )
}

fn segment_analysis(
    category: CommandSemanticCategory,
    risk: CommandRiskLevel,
    segment: &str,
    reason: &'static str,
) -> CommandSemanticAnalysis {
    CommandSemanticAnalysis {
        category,
        risk,
        segment: segment.to_string(),
        reason,
    }
}

fn risk_rank(risk: CommandRiskLevel) -> u8 {
    match risk {
        CommandRiskLevel::Safe => 0,
        CommandRiskLevel::Unknown => 1,
        CommandRiskLevel::PotentiallyRisky => 2,
        CommandRiskLevel::Destructive => 3,
    }
}

fn split_command_segments(command: &str) -> Vec<String> {
    let mut segments = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut escaped = false;
    let mut chars = command.chars().peekable();

    while let Some(ch) = chars.next() {
        if escaped {
            current.push(ch);
            escaped = false;
            continue;
        }
        if ch == '\\' {
            current.push(ch);
            escaped = true;
            continue;
        }
        if let Some(q) = quote {
            current.push(ch);
            if ch == q {
                quote = None;
            }
            continue;
        }
        if ch == '\'' || ch == '"' {
            quote = Some(ch);
            current.push(ch);
            continue;
        }
        if matches!(ch, ';' | '\n' | '\r' | '|') {
            push_segment(&mut segments, &mut current);
            if ch == '|' && chars.peek() == Some(&'|') {
                let _ = chars.next();
            }
            continue;
        }
        if ch == '&' && chars.peek() == Some(&'&') {
            let _ = chars.next();
            push_segment(&mut segments, &mut current);
            continue;
        }
        current.push(ch);
    }
    push_segment(&mut segments, &mut current);
    if segments.is_empty() {
        vec![command.trim().to_string()]
    } else {
        segments
    }
}

fn push_segment(segments: &mut Vec<String>, current: &mut String) {
    let segment = current.trim();
    if !segment.is_empty() {
        segments.push(segment.to_string());
    }
    current.clear();
}

fn tokenize_segment(segment: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut escaped = false;

    for ch in segment.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if let Some(q) = quote {
            if ch == q {
                quote = None;
            } else {
                current.push(ch);
            }
            continue;
        }
        if ch == '\'' || ch == '"' {
            quote = Some(ch);
            continue;
        }
        if ch.is_whitespace() {
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current).to_ascii_lowercase());
            }
            continue;
        }
        current.push(ch);
    }
    if !current.is_empty() {
        tokens.push(current.to_ascii_lowercase());
    }
    tokens
}

fn command_name(tokens: &[String]) -> Option<String> {
    let mut index = 0;
    while tokens
        .get(index)
        .is_some_and(|token| token.contains('=') && !token.starts_with('-'))
    {
        index += 1;
    }
    tokens.get(index).cloned()
}

fn pipes_remote_content_to_shell(command: &str) -> bool {
    (command.contains("curl ") || command.contains("wget "))
        && (command.contains("| sh")
            || command.contains("| bash")
            || command.contains("|sh")
            || command.contains("|bash"))
}

fn has_recursive_force_flags(tokens: &[String]) -> bool {
    let mut recursive = false;
    let mut force = false;
    for token in tokens.iter().skip(1) {
        if !token.starts_with('-') {
            continue;
        }
        recursive |= token.contains('r') || token.contains('R');
        force |= token.contains('f');
    }
    recursive && force
}

fn is_sed_or_awk_file_edit(command: &str, tokens: &[String], segment: &str) -> bool {
    if command != "sed" && command != "awk" {
        return false;
    }
    tokens
        .iter()
        .skip(1)
        .any(|token| token == "-i" || token.starts_with("-i"))
        || segment.contains(" >")
        || segment.contains(">>")
}

fn is_package_install(command: &str, tokens: &[String]) -> bool {
    if !PACKAGE_INSTALL_COMMANDS.contains(&command) {
        return false;
    }
    match command {
        "cargo" => tokens.get(1).is_some_and(|sub| sub == "install"),
        "go" => tokens
            .get(1)
            .is_some_and(|sub| sub == "install" || sub == "get"),
        "npm" | "pnpm" | "yarn" => tokens
            .get(1)
            .is_some_and(|sub| matches!(sub.as_str(), "install" | "add" | "i")),
        "uv" => tokens
            .windows(2)
            .any(|pair| pair[0] == "pip" && pair[1] == "install"),
        _ => tokens
            .iter()
            .skip(1)
            .any(|token| matches!(token.as_str(), "install" | "add")),
    }
}

fn is_read_only_segment(command: &str, tokens: &[String], cmd_lower: &str) -> bool {
    if command == "git" {
        return tokens
            .get(1)
            .is_some_and(|subcommand| GIT_READ_ONLY_SUBCOMMANDS.contains(&subcommand.as_str()));
    }

    READ_ONLY_COMMANDS
        .iter()
        .any(|prefix| cmd_lower == *prefix || cmd_lower.starts_with(&format!("{prefix} ")))
}
