use std::sync::LazyLock;

use regex::Regex;

static PS_SEARCH_COMMANDS: &[&str] = &["select-string", "findstr", "where.exe"];
static PS_READ_COMMANDS: &[&str] = &[
    "get-content",
    "get-item",
    "get-itemproperty",
    "test-path",
    "resolve-path",
    "get-process",
    "get-service",
    "get-childitem",
    "get-location",
    "get-filehash",
    "get-acl",
    "format-hex",
    "get-command",
    "get-help",
    "get-module",
    "get-alias",
];
static PS_READONLY_NAVIGATION_COMMANDS: &[&str] =
    &["set-location", "push-location", "pop-location"];
static PS_GIT_READONLY_SUBCOMMANDS: &[&str] = &["status", "diff", "log", "show", "rev-parse"];

static DESTRUCTIVE_PATTERNS: LazyLock<Vec<(Regex, &'static str)>> = LazyLock::new(|| {
    vec![
        (
            Regex::new(r"(?i)\b(remove-item|rm|del|rd|rmdir|ri)\b.*\-(recurse|force)").unwrap(),
            "Note: may recursively or forcibly remove files",
        ),
        (
            Regex::new(r"(?i)\bformat-volume\b").unwrap(),
            "Note: may format a disk volume",
        ),
        (
            Regex::new(r"(?i)\bclear-disk\b").unwrap(),
            "Note: may clear a disk",
        ),
        (
            Regex::new(r"(?i)\bstop-computer\b").unwrap(),
            "Note: will shut down the computer",
        ),
        (
            Regex::new(r"(?i)\brestart-computer\b").unwrap(),
            "Note: will restart the computer",
        ),
        (
            Regex::new(r"(?i)\bgit\s+reset\s+--hard\b").unwrap(),
            "Note: may discard uncommitted changes",
        ),
        (
            Regex::new(r"(?i)\bgit\s+push\b.*(--force|--force-with-lease|-f)\b").unwrap(),
            "Note: may overwrite remote history",
        ),
    ]
});

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PowerShellAnalysis {
    pub(super) command_type: &'static str,
    pub(super) read_only: bool,
    pub(super) read_only_reason: Option<String>,
    pub(super) destructive_warning: Option<&'static str>,
    pub(super) suggestion: Option<String>,
}

pub(super) fn analyze_powershell_command(command: &str) -> PowerShellAnalysis {
    let command_type = classify_powershell_command(command);
    let read_only_validation = validate_read_only_powershell_command(command);
    let read_only = read_only_validation.is_safe;
    let destructive_warning = get_destructive_command_warning(command);
    let suggestion = suggest_safe_rewrite(command, command_type);
    PowerShellAnalysis {
        command_type,
        read_only,
        read_only_reason: read_only_validation.reason,
        destructive_warning,
        suggestion,
    }
}

pub(super) fn classify_powershell_command(command: &str) -> &'static str {
    let segments = split_powershell_segments(command);
    let commands = segments
        .iter()
        .filter_map(|segment| first_segment_command(segment))
        .collect::<Vec<_>>();

    if commands
        .iter()
        .any(|cmd| PS_SEARCH_COMMANDS.iter().any(|candidate| *candidate == cmd))
    {
        "search"
    } else if commands.iter().any(|cmd| {
        PS_READ_COMMANDS.iter().any(|candidate| *candidate == cmd)
            || PS_READONLY_NAVIGATION_COMMANDS
                .iter()
                .any(|candidate| *candidate == cmd)
    }) {
        "read"
    } else {
        "generic"
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ReadOnlyValidation {
    is_safe: bool,
    reason: Option<String>,
}

fn validate_read_only_powershell_command(command: &str) -> ReadOnlyValidation {
    let segments = split_powershell_segments(command);
    if segments.is_empty() {
        return ReadOnlyValidation {
            is_safe: false,
            reason: Some("empty command".to_string()),
        };
    }

    for segment in segments {
        let tokens = tokenize_powershell_segment(&segment);
        let Some(cmd) = tokens.first() else {
            continue;
        };
        let Some(config) = read_only_config(cmd) else {
            return ReadOnlyValidation {
                is_safe: false,
                reason: Some(format!("non-read-only command: {}", cmd)),
            };
        };

        if tokens
            .iter()
            .skip(1)
            .any(|token| looks_like_redirection(token))
        {
            return ReadOnlyValidation {
                is_safe: false,
                reason: Some(format!("redirection detected in {}", cmd)),
            };
        }

        if !config.allow_all_flags {
            for token in tokens.iter().skip(1) {
                if is_flag_token(token)
                    && !config
                        .safe_flags
                        .iter()
                        .any(|flag| flag.eq_ignore_ascii_case(token))
                {
                    return ReadOnlyValidation {
                        is_safe: false,
                        reason: Some(format!("unsafe flag {} for {}", token, cmd)),
                    };
                }
            }
        }

        if cmd == "git" && !validate_git_read_only_tokens(&tokens) {
            return ReadOnlyValidation {
                is_safe: false,
                reason: Some("non-read-only git subcommand".to_string()),
            };
        }
    }

    ReadOnlyValidation {
        is_safe: true,
        reason: Some("validated read-only command".to_string()),
    }
}

pub(super) fn get_destructive_command_warning(command: &str) -> Option<&'static str> {
    DESTRUCTIVE_PATTERNS.iter().find_map(|entry| {
        let (pattern, warning) = entry;
        pattern.is_match(command).then_some(*warning)
    })
}

pub(super) fn suggest_safe_rewrite(command: &str, command_type: &str) -> Option<String> {
    match command_type {
        "search" => Some(
            "Prefer `grep` or `glob` tools for structured search results when PowerShell is not specifically required."
                .to_string(),
        ),
        "read" => Some(
            "Prefer `read_file` for file reads so the agent keeps precise file context.".to_string(),
        ),
        _ => {
            let first = first_segment_command(command).unwrap_or_default();
            if first == "set-location" || first == "push-location" || first == "pop-location" {
                Some(
                    "Prefer absolute paths and avoid shell-only cwd changes unless the user specifically wants a PowerShell workflow."
                        .to_string(),
                )
            } else {
                None
            }
        }
    }
}

#[derive(Clone, Copy)]
struct ReadOnlyCommandConfig {
    safe_flags: &'static [&'static str],
    allow_all_flags: bool,
}

fn read_only_config(command: &str) -> Option<ReadOnlyCommandConfig> {
    let cmd = command.to_ascii_lowercase();
    match cmd.as_str() {
        "select-string" => Some(ReadOnlyCommandConfig {
            safe_flags: &[
                "-Path",
                "-Pattern",
                "-SimpleMatch",
                "-CaseSensitive",
                "-Quiet",
                "-List",
                "-NotMatch",
                "-AllMatches",
                "-Encoding",
                "-Context",
                "-Raw",
            ],
            allow_all_flags: false,
        }),
        "get-content" => Some(ReadOnlyCommandConfig {
            safe_flags: &[
                "-Path",
                "-LiteralPath",
                "-TotalCount",
                "-Head",
                "-Tail",
                "-Raw",
                "-Encoding",
                "-Delimiter",
                "-ReadCount",
            ],
            allow_all_flags: false,
        }),
        "get-item" => Some(ReadOnlyCommandConfig {
            safe_flags: &["-Path", "-LiteralPath", "-Force", "-Stream"],
            allow_all_flags: false,
        }),
        "get-itemproperty" => Some(ReadOnlyCommandConfig {
            safe_flags: &["-Path", "-LiteralPath", "-Name"],
            allow_all_flags: false,
        }),
        "test-path" => Some(ReadOnlyCommandConfig {
            safe_flags: &[
                "-Path",
                "-LiteralPath",
                "-PathType",
                "-Filter",
                "-Include",
                "-Exclude",
                "-IsValid",
            ],
            allow_all_flags: false,
        }),
        "resolve-path" => Some(ReadOnlyCommandConfig {
            safe_flags: &["-Path", "-LiteralPath", "-Relative"],
            allow_all_flags: false,
        }),
        "get-filehash" => Some(ReadOnlyCommandConfig {
            safe_flags: &["-Path", "-LiteralPath", "-Algorithm"],
            allow_all_flags: false,
        }),
        "get-acl" => Some(ReadOnlyCommandConfig {
            safe_flags: &["-Path", "-LiteralPath", "-Audit"],
            allow_all_flags: false,
        }),
        "get-command" | "get-help" | "get-module" | "get-alias" => Some(ReadOnlyCommandConfig {
            safe_flags: &[],
            allow_all_flags: true,
        }),
        "get-process" | "get-service" | "get-location" | "format-hex" | "where.exe" | "findstr" => {
            Some(ReadOnlyCommandConfig {
                safe_flags: &[],
                allow_all_flags: true,
            })
        }
        "get-childitem" => Some(ReadOnlyCommandConfig {
            safe_flags: &[
                "-Path",
                "-LiteralPath",
                "-Filter",
                "-Include",
                "-Exclude",
                "-Recurse",
                "-Depth",
                "-Name",
                "-Force",
                "-Directory",
                "-File",
            ],
            allow_all_flags: false,
        }),
        "set-location" | "push-location" => Some(ReadOnlyCommandConfig {
            safe_flags: &["-Path", "-LiteralPath", "-PassThru", "-StackName"],
            allow_all_flags: false,
        }),
        "pop-location" => Some(ReadOnlyCommandConfig {
            safe_flags: &["-PassThru", "-StackName"],
            allow_all_flags: false,
        }),
        "git" => Some(ReadOnlyCommandConfig {
            safe_flags: &[],
            allow_all_flags: true,
        }),
        _ => None,
    }
}

fn split_powershell_segments(command: &str) -> Vec<String> {
    command
        .split([';', '\n', '\r', '|'])
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn tokenize_powershell_segment(segment: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;

    for ch in segment.chars() {
        match quote {
            Some(q) if ch == q => quote = None,
            Some(_) => current.push(ch),
            None if ch == '"' || ch == '\'' => quote = Some(ch),
            None if ch.is_whitespace() => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            None => current.push(ch),
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

fn first_segment_command(segment: &str) -> Option<String> {
    tokenize_powershell_segment(segment)
        .into_iter()
        .next()
        .map(|token| token.to_ascii_lowercase())
}

fn is_flag_token(token: &str) -> bool {
    token.starts_with('-') && token.len() > 1
}

fn looks_like_redirection(token: &str) -> bool {
    token == ">" || token == ">>" || token == "2>" || token == "2>>"
}

fn validate_git_read_only_tokens(tokens: &[String]) -> bool {
    let Some(subcommand) = tokens.get(1) else {
        return false;
    };
    PS_GIT_READONLY_SUBCOMMANDS
        .iter()
        .any(|allowed| allowed.eq_ignore_ascii_case(subcommand))
}
