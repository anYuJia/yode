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
