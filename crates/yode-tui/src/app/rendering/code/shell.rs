use super::{CodeToken, CodeTokenKind};
use super::languages::{
    consume_quoted_string, is_word_char, starts_comment, CommentStyle,
    is_shell_keyword,
};

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellLineKind {
    Blank,
    Prompt,
    Continuation,
    Command,
    Info,
    Success,
    Warning,
    Output,
    Error,
    Comment,
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ShellSessionState {
    #[default]
    Idle,
    AfterCommand,
    InOutput,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ShellPromptKind {
    Primary,
    Continuation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ShellPromptMatch {
    pub(super) prefix_len: usize,
    pub(super) kind: ShellPromptKind,
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ShellOutputStyle {
    Info,
    Success,
    Warning,
    Error,
}

#[cfg(test)]
pub fn tokenize_shell_session_line(
    line: &str,
    session_state: ShellSessionState,
) -> (ShellLineKind, Vec<CodeToken>, ShellSessionState) {
    use super::languages::split_leading_whitespace;
    let (indent, content) = split_leading_whitespace(line);
    let kind = classify_shell_line(content, session_state);
    let next_session_state = match kind {
        ShellLineKind::Blank | ShellLineKind::Comment => session_state,
        ShellLineKind::Prompt | ShellLineKind::Continuation | ShellLineKind::Command => {
            ShellSessionState::AfterCommand
        }
        ShellLineKind::Info
        | ShellLineKind::Success
        | ShellLineKind::Warning
        | ShellLineKind::Output
        | ShellLineKind::Error => ShellSessionState::InOutput,
    };

    let mut tokens = Vec::new();
    if !indent.is_empty() {
        tokens.push(CodeToken {
            text: indent.to_string(),
            kind: CodeTokenKind::Plain,
        });
    }

    match kind {
        ShellLineKind::Blank => {}
        ShellLineKind::Info | ShellLineKind::Success | ShellLineKind::Warning => {
            tokens.extend(tokenize_shell_output_line(content, kind))
        }
        ShellLineKind::Output => {
            tokens.extend(tokenize_shell_output_line(content, ShellLineKind::Output))
        }
        ShellLineKind::Error => {
            tokens.extend(tokenize_shell_output_line(content, ShellLineKind::Error))
        }
        ShellLineKind::Comment => tokens.push(CodeToken {
            text: content.to_string(),
            kind: CodeTokenKind::Comment,
        }),
        ShellLineKind::Prompt | ShellLineKind::Continuation => {
            tokens.extend(tokenize_shell_command_line(
                content,
                shell_prompt_prefix(content).map(|prompt| prompt.prefix_len),
            ));
        }
        ShellLineKind::Command => {
            tokens.extend(tokenize_shell_command_line(content, None));
        }
    }

    (kind, tokens, next_session_state)
}

pub(super) fn tokenize_shell_command_line(line: &str, prompt_prefix_len: Option<usize>) -> Vec<CodeToken> {
    let mut tokens = Vec::new();
    let mut command_seen = false;
    let content = if let Some(prefix_len) = prompt_prefix_len {
        tokens.push(CodeToken {
            text: line[..prefix_len].to_string(),
            kind: CodeTokenKind::ShellPrompt,
        });
        &line[prefix_len..]
    } else {
        line
    };

    let chars: Vec<char> = content.chars().collect();
    let len = chars.len();
    let mut index = 0;

    while index < len {
        if chars[index].is_whitespace() {
            let start = index;
            while index < len && chars[index].is_whitespace() {
                index += 1;
            }
            tokens.push(CodeToken {
                text: chars[start..index].iter().collect(),
                kind: CodeTokenKind::Plain,
            });
            continue;
        }

        if starts_comment(&chars, index, CommentStyle::ShellHash) {
            tokens.push(CodeToken {
                text: chars[index..].iter().collect(),
                kind: CodeTokenKind::Comment,
            });
            break;
        }

        if chars[index] == '$' {
            let start = index;
            index += 1;
            if index < len && chars[index] == '{' {
                index += 1;
                while index < len && chars[index] != '}' {
                    index += 1;
                }
                if index < len {
                    index += 1;
                }
            } else {
                while index < len
                    && (is_word_char(chars[index])
                        || chars[index].is_ascii_digit()
                        || matches!(chars[index], '?' | '!' | '@' | '*' | '#' | '$'))
                {
                    index += 1;
                }
            }
            tokens.push(CodeToken {
                text: chars[start..index.min(len)].iter().collect(),
                kind: CodeTokenKind::Variable,
            });
            continue;
        }

        if matches!(chars[index], '"' | '\'' | '`') {
            let start = index;
            index = consume_quoted_string(&chars, index);
            tokens.push(CodeToken {
                text: chars[start..index.min(len)].iter().collect(),
                kind: CodeTokenKind::String,
            });
            continue;
        }

        if is_shell_flag_start(&chars, index) {
            let start = index;
            index = consume_shell_atom(&chars, index);
            tokens.push(CodeToken {
                text: chars[start..index].iter().collect(),
                kind: CodeTokenKind::ShellFlag,
            });
            continue;
        }

        if is_shell_atom_start(chars[index]) {
            let start = index;
            index = consume_shell_atom(&chars, index);
            let atom: String = chars[start..index].iter().collect();
            let kind = if !command_seen && !looks_like_shell_assignment(&atom) {
                command_seen = true;
                CodeTokenKind::ShellCommand
            } else if looks_like_shell_output_path(&atom) {
                CodeTokenKind::ShellPath
            } else if is_shell_keyword(&atom) {
                CodeTokenKind::Keyword
            } else if atom.chars().all(|ch| ch.is_ascii_digit()) {
                CodeTokenKind::Number
            } else {
                CodeTokenKind::Plain
            };
            tokens.push(CodeToken { text: atom, kind });
            continue;
        }

        if chars[index].is_ascii_digit() {
            let start = index;
            index += 1;
            while index < len
                && (chars[index].is_ascii_digit()
                    || matches!(chars[index], '.' | '_' | 'x' | 'X' | 'a'..='f' | 'A'..='F'))
            {
                index += 1;
            }
            tokens.push(CodeToken {
                text: chars[start..index].iter().collect(),
                kind: CodeTokenKind::Number,
            });
            continue;
        }

        if is_shell_output_atom_start(chars[index]) {
            let start = index;
            index = consume_shell_output_atom(&chars, index);
            let atom: String = chars[start..index].iter().collect();
            let lower = atom.to_ascii_lowercase();
            let kind = if looks_like_shell_output_path(&atom) {
                CodeTokenKind::ShellPath
            } else if matches!(lower.as_str(), "ok" | "success" | "succeeded" | "passed") {
                CodeTokenKind::ShellSuccess
            } else if matches!(lower.as_str(), "warning" | "warn" | "deprecated") {
                CodeTokenKind::ShellWarning
            } else if matches!(lower.as_str(), "error" | "failed" | "fatal" | "panic") {
                CodeTokenKind::ShellError
            } else if matches!(lower.as_str(), "info" | "note") {
                CodeTokenKind::ShellInfo
            } else {
                CodeTokenKind::Plain
            };
            tokens.push(CodeToken { text: atom, kind });
            continue;
        }

        if matches!(
            chars[index],
            '=' | '+'
                | '-'
                | '*'
                | '/'
                | '!'
                | '<'
                | '>'
                | '|'
                | '&'
                | ':'
                | '%'
                | '.'
                | ','
                | ';'
                | '('
                | ')'
                | '{'
                | '}'
                | '['
                | ']'
        ) {
            tokens.push(CodeToken {
                text: chars[index].to_string(),
                kind: CodeTokenKind::Operator,
            });
            index += 1;
            continue;
        }

        tokens.push(CodeToken {
            text: chars[index].to_string(),
            kind: CodeTokenKind::Plain,
        });
        index += 1;
    }

    tokens
}

#[cfg(test)]
pub(super) fn classify_shell_line(line: &str, session_state: ShellSessionState) -> ShellLineKind {
    if line.trim().is_empty() {
        return ShellLineKind::Blank;
    }
    if let Some(prompt) = shell_prompt_prefix(line) {
        return match prompt.kind {
            ShellPromptKind::Primary => ShellLineKind::Prompt,
            ShellPromptKind::Continuation
                if matches!(session_state, ShellSessionState::AfterCommand) =>
            {
                ShellLineKind::Continuation
            }
            ShellPromptKind::Continuation => ShellLineKind::Command,
        };
    }
    if matches!(session_state, ShellSessionState::Idle) && line.trim_start().starts_with('#') {
        return ShellLineKind::Comment;
    }
    if let Some(style) = shell_output_style(line) {
        return match style {
            ShellOutputStyle::Info => ShellLineKind::Info,
            ShellOutputStyle::Success => ShellLineKind::Success,
            ShellOutputStyle::Warning => ShellLineKind::Warning,
            ShellOutputStyle::Error => ShellLineKind::Error,
        };
    }
    if matches!(
        session_state,
        ShellSessionState::AfterCommand | ShellSessionState::InOutput
    ) && looks_like_new_shell_command(line)
    {
        return ShellLineKind::Command;
    }
    if matches!(
        session_state,
        ShellSessionState::AfterCommand | ShellSessionState::InOutput
    ) {
        return ShellLineKind::Output;
    }
    ShellLineKind::Command
}

#[cfg(test)]
pub(super) fn tokenize_shell_output_line(line: &str, kind: ShellLineKind) -> Vec<CodeToken> {
    let mut tokens = Vec::new();
    let mut content = line;
    let default_kind = match kind {
        ShellLineKind::Output => CodeTokenKind::Plain,
        _ => CodeTokenKind::Plain,
    };

    if let Some(prefix_len) = shell_output_prefix_len(line, kind) {
        let prefix_kind = match kind {
            ShellLineKind::Info => CodeTokenKind::ShellInfo,
            ShellLineKind::Success => CodeTokenKind::ShellSuccess,
            ShellLineKind::Warning => CodeTokenKind::ShellWarning,
            ShellLineKind::Error => CodeTokenKind::ShellError,
            ShellLineKind::Output => CodeTokenKind::Plain,
            _ => CodeTokenKind::Plain,
        };
        tokens.push(CodeToken {
            text: line[..prefix_len].to_string(),
            kind: prefix_kind,
        });
        content = &line[prefix_len..];
    }

    let chars: Vec<char> = content.chars().collect();
    let len = chars.len();
    let mut index = 0;

    while index < len {
        if chars[index].is_whitespace() {
            let start = index;
            while index < len && chars[index].is_whitespace() {
                index += 1;
            }
            tokens.push(CodeToken {
                text: chars[start..index].iter().collect(),
                kind: CodeTokenKind::Plain,
            });
            continue;
        }

        if matches!(chars[index], '"' | '\'' | '`') {
            let start = index;
            index = consume_quoted_string(&chars, index);
            tokens.push(CodeToken {
                text: chars[start..index.min(len)].iter().collect(),
                kind: CodeTokenKind::String,
            });
            continue;
        }

        if chars[index].is_ascii_digit() {
            let start = index;
            index += 1;
            while index < len
                && (chars[index].is_ascii_digit()
                    || matches!(chars[index], '.' | '_' | 'x' | 'X' | 'a'..='f' | 'A'..='F' | 's'))
            {
                index += 1;
            }
            tokens.push(CodeToken {
                text: chars[start..index].iter().collect(),
                kind: CodeTokenKind::Number,
            });
            continue;
        }

        if is_shell_output_atom_start(chars[index]) {
            let start = index;
            index = consume_shell_output_atom(&chars, index);
            let atom: String = chars[start..index].iter().collect();
            let lower = atom.to_ascii_lowercase();
            let kind = if looks_like_shell_output_path(&atom) {
                CodeTokenKind::ShellPath
            } else if matches!(lower.as_str(), "ok" | "success" | "succeeded" | "passed") {
                CodeTokenKind::ShellSuccess
            } else if matches!(lower.as_str(), "warning" | "warn" | "deprecated") {
                CodeTokenKind::ShellWarning
            } else if matches!(lower.as_str(), "error" | "failed" | "fatal" | "panic") {
                CodeTokenKind::ShellError
            } else if matches!(lower.as_str(), "info" | "note") {
                CodeTokenKind::ShellInfo
            } else {
                default_kind
            };
            tokens.push(CodeToken { text: atom, kind });
            continue;
        }

        if matches!(
            chars[index],
            '=' | '+'
                | '-'
                | '*'
                | '!'
                | '<'
                | '>'
                | '|'
                | '&'
                | '%'
                | ','
                | ';'
                | '('
                | ')'
                | '{'
                | '}'
                | '['
                | ']'
        ) {
            tokens.push(CodeToken {
                text: chars[index].to_string(),
                kind: if matches!(kind, ShellLineKind::Output) {
                    CodeTokenKind::Plain
                } else {
                    CodeTokenKind::Operator
                },
            });
            index += 1;
            continue;
        }

        tokens.push(CodeToken {
            text: chars[index].to_string(),
            kind: default_kind,
        });
        index += 1;
    }

    tokens
}

pub(super) fn shell_prompt_prefix(line: &str) -> Option<ShellPromptMatch> {
    let trimmed = line.trim_start();
    let leading_whitespace = line.len().saturating_sub(trimmed.len());

    if trimmed.is_empty() {
        return None;
    }

    for marker in ["$ ", "% ", "❯ ", "λ ", "➜ "] {
        if trimmed.starts_with(marker) && trimmed.len() > marker.len() {
            return Some(ShellPromptMatch {
                prefix_len: leading_whitespace + marker.len(),
                kind: ShellPromptKind::Primary,
            });
        }
    }

    if let Some(prefix_len) = prefixed_shell_prompt_prefix_len(trimmed) {
        return Some(ShellPromptMatch {
            prefix_len: leading_whitespace + prefix_len,
            kind: ShellPromptKind::Primary,
        });
    }

    if let Some(prefix_len) = powershell_prompt_prefix_len(trimmed) {
        return Some(ShellPromptMatch {
            prefix_len: leading_whitespace + prefix_len,
            kind: ShellPromptKind::Primary,
        });
    }

    if let Some(prefix_len) = rooted_shell_prompt_prefix_len(trimmed) {
        return Some(ShellPromptMatch {
            prefix_len: leading_whitespace + prefix_len,
            kind: ShellPromptKind::Primary,
        });
    }

    for marker in [">> ", "> ", "... "] {
        if trimmed.starts_with(marker) && trimmed.len() > marker.len() {
            return Some(ShellPromptMatch {
                prefix_len: leading_whitespace + marker.len(),
                kind: ShellPromptKind::Continuation,
            });
        }
    }

    None
}

fn prefixed_shell_prompt_prefix_len(line: &str) -> Option<usize> {
    let mut best: Option<usize> = None;

    for marker in ["$ ", "% ", "❯ ", "λ ", "➜ "] {
        for (index, _) in line.match_indices(marker) {
            let end = index + marker.len();
            if index == 0 || end >= line.len() {
                continue;
            }

            if looks_like_shell_prompt_prefix(&line[..index]) {
                best = Some(best.map_or(end, |current| current.max(end)));
            }
        }
    }

    best
}

fn powershell_prompt_prefix_len(line: &str) -> Option<usize> {
    for prefix in ["PS> ", "pwsh> ", "powershell> "] {
        if line.starts_with(prefix) && line.len() > prefix.len() {
            return Some(prefix.len());
        }
    }

    if let Some(rest) = line.strip_prefix("PS ") {
        let index = rest.find("> ")?;
        let end = 3 + index + 2;
        if end < line.len() {
            return Some(end);
        }
    }

    None
}

fn rooted_shell_prompt_prefix_len(line: &str) -> Option<usize> {
    let index = line.find("# ")?;
    let end = index + 2;
    if index == 0 || end >= line.len() {
        return None;
    }

    let prefix = &line[..index];
    if prefix.chars().any(char::is_whitespace) {
        return None;
    }

    if prefix.contains('@')
        || prefix.contains(':')
        || prefix.contains('/')
        || prefix.contains('~')
        || prefix.contains('\\')
        || prefix.starts_with('(')
        || prefix.starts_with('[')
    {
        return Some(end);
    }

    None
}

fn looks_like_shell_prompt_prefix(prefix: &str) -> bool {
    let trimmed = prefix.trim_end();
    if trimmed.is_empty() {
        return false;
    }

    trimmed.contains('@')
        || trimmed.contains(':')
        || trimmed.contains('/')
        || trimmed.contains('~')
        || trimmed.contains('\\')
        || trimmed.contains('(')
        || trimmed.contains('[')
        || trimmed.contains(" on ")
        || trimmed.contains(" via ")
        || trimmed.contains("git:(")
        || trimmed.split_whitespace().count() > 1
}

#[cfg(test)]
fn is_likely_shell_error_output(line: &str) -> bool {
    let trimmed = line.trim_start();
    let lowered = trimmed.to_ascii_lowercase();

    lowered.starts_with("error")
        || lowered.starts_with("fatal")
        || lowered.starts_with("panic")
        || lowered.starts_with("traceback")
        || lowered.starts_with("npm err!")
        || lowered.starts_with("yarn error")
        || lowered.starts_with("pnpm error")
        || lowered.starts_with("zsh:")
        || lowered.starts_with("bash:")
        || lowered.starts_with("sh:")
        || lowered.starts_with("fish:")
        || lowered.starts_with("pwsh:")
        || lowered.starts_with("powershell:")
        || lowered.starts_with("stderr:")
        || trimmed.starts_with("thread '")
        || trimmed.starts_with("× ")
        || lowered.contains("command not found")
        || lowered.contains("permission denied")
        || lowered.contains("no such file or directory")
}

#[cfg(test)]
fn is_likely_shell_warning_output(line: &str) -> bool {
    let trimmed = line.trim_start();
    let lowered = trimmed.to_ascii_lowercase();

    lowered.starts_with("warning:")
        || lowered.starts_with("warning ")
        || lowered.starts_with("warn:")
        || lowered.starts_with("warn ")
        || lowered.starts_with("npm warn")
        || lowered.starts_with("pnpm warn")
        || lowered.starts_with("yarn warn")
        || lowered.contains("deprecated")
}

#[cfg(test)]
fn is_likely_shell_success_output(line: &str) -> bool {
    let trimmed = line.trim_start();
    let lowered = trimmed.to_ascii_lowercase();

    lowered.starts_with("finished ")
        || lowered.starts_with("done")
        || lowered.starts_with("completed")
        || lowered.starts_with("succeeded")
        || lowered.starts_with("success")
        || lowered.starts_with("created ")
        || lowered.starts_with("built ")
        || lowered == "ok"
        || lowered.starts_with("ok ")
        || trimmed.starts_with("✓ ")
        || trimmed.starts_with("✔ ")
}

#[cfg(test)]
fn is_likely_shell_info_output(line: &str) -> bool {
    let first = line.split_whitespace().next().unwrap_or("");
    matches!(
        first,
        "Compiling"
            | "Checking"
            | "Running"
            | "Downloaded"
            | "Downloading"
            | "Resolving"
            | "Collecting"
            | "Installing"
            | "Unpacking"
            | "Updating"
            | "Creating"
            | "Added"
            | "Removed"
            | "Deleted"
            | "Building"
            | "Bundling"
            | "Linking"
            | "note:"
            | "Note:"
            | "info:"
            | "Info:"
            | "INFO"
            | "Total"
            | "On"
    )
}

#[cfg(test)]
pub(super) fn shell_output_style(line: &str) -> Option<ShellOutputStyle> {
    if is_likely_shell_error_output(line) {
        return Some(ShellOutputStyle::Error);
    }
    if is_likely_shell_warning_output(line) {
        return Some(ShellOutputStyle::Warning);
    }
    if is_likely_shell_success_output(line) {
        return Some(ShellOutputStyle::Success);
    }
    if is_likely_shell_info_output(line) {
        return Some(ShellOutputStyle::Info);
    }
    None
}

#[cfg(test)]
pub(super) fn shell_output_prefix_len(line: &str, kind: ShellLineKind) -> Option<usize> {
    let trimmed = line.trim_start();
    let leading_whitespace = line.len().saturating_sub(trimmed.len());

    let prefixes: &[&str] = match kind {
        ShellLineKind::Info => &[
            "Compiling",
            "Checking",
            "Running",
            "Downloaded",
            "Downloading",
            "Resolving",
            "Collecting",
            "Installing",
            "Unpacking",
            "Updating",
            "Building",
            "Bundling",
            "Linking",
            "note:",
            "Note:",
            "info:",
            "Info:",
            "INFO",
        ],
        ShellLineKind::Success => &[
            "Finished",
            "Done",
            "done",
            "Completed",
            "completed",
            "Success",
            "success",
            "Created",
            "Built",
            "ok",
            "OK",
            "\u{2713}",
            "\u{2714}",
        ],
        ShellLineKind::Warning => &[
            "warning:",
            "warning",
            "warn:",
            "warn",
            "npm WARN",
            "pnpm WARN",
            "yarn warn",
        ],
        ShellLineKind::Error => &[
            "error:",
            "error",
            "fatal:",
            "fatal",
            "panic:",
            "panic",
            "traceback",
            "stderr:",
            "npm ERR!",
            "pnpm error",
            "yarn error",
            "bash:",
            "zsh:",
            "sh:",
            "fish:",
            "pwsh:",
            "powershell:",
        ],
        ShellLineKind::Output => &[],
        _ => &[],
    };

    for prefix in prefixes {
        if trimmed.starts_with(prefix) {
            return Some(leading_whitespace + prefix.len());
        }
    }

    None
}

pub(super) fn consume_shell_output_atom(chars: &[char], mut index: usize) -> usize {
    while index < chars.len()
        && !chars[index].is_whitespace()
        && !matches!(
            chars[index],
            '=' | '+'
                | '-'
                | '*'
                | '!'
                | '<'
                | '>'
                | '|'
                | '&'
                | '%'
                | ','
                | ';'
                | '('
                | ')'
                | '{'
                | '}'
                | '['
                | ']'
                | '"'
                | '\''
                | '`'
        )
    {
        index += 1;
    }
    index
}

pub(super) fn is_shell_output_atom_start(ch: char) -> bool {
    ch.is_alphanumeric() || matches!(ch, '_' | '.' | '/' | '~')
}

pub(super) fn looks_like_shell_output_path(atom: &str) -> bool {
    atom.starts_with("./")
        || atom.starts_with("../")
        || atom.starts_with("~/")
        || atom.starts_with('/')
        || atom.contains('\\')
        || atom.contains('/')
        || atom.ends_with(".rs")
        || atom.ends_with(".toml")
        || atom.ends_with(".json")
        || atom.ends_with(".yaml")
        || atom.ends_with(".yml")
        || atom.ends_with(".py")
        || atom.ends_with(".sh")
        || atom.ends_with(".log")
}

#[cfg(test)]
pub(super) fn looks_like_new_shell_command(line: &str) -> bool {
    let trimmed = line.trim_start();
    if trimmed.is_empty() || shell_output_style(trimmed).is_some() {
        return false;
    }

    let mut parts = trimmed.split_whitespace();
    let Some(first) = parts.next() else {
        return false;
    };

    if looks_like_shell_assignment(first) {
        return true;
    }

    if is_known_shell_command(first) {
        return true;
    }

    if first.starts_with("./")
        || first.starts_with("../")
        || first.starts_with("~/")
        || first.starts_with('/')
        || first.ends_with(".sh")
        || first.ends_with(".bash")
        || first.ends_with(".zsh")
        || first.ends_with(".ps1")
        || first.ends_with(".py")
    {
        return true;
    }

    if trimmed.contains(" --")
        || trimmed.contains(" | ")
        || trimmed.contains(" && ")
        || trimmed.contains(" || ")
        || trimmed.contains(" > ")
        || trimmed.contains(" < ")
        || trimmed.contains(" 2>")
        || trimmed.contains('$')
        || trimmed.ends_with('\\')
    {
        return true;
    }

    first.contains('-')
        && first
            .chars()
            .next()
            .map(|ch| ch.is_ascii_uppercase())
            .unwrap_or(false)
}

pub(super) fn is_shell_flag_start(chars: &[char], index: usize) -> bool {
    chars[index] == '-'
        && index + 1 < chars.len()
        && !chars[index + 1].is_whitespace()
        && chars[index + 1] != '-'
        || (chars[index] == '-'
            && index + 2 < chars.len()
            && chars[index + 1] == '-'
            && !chars[index + 2].is_whitespace())
}

pub(super) fn is_shell_atom_start(ch: char) -> bool {
    ch.is_alphabetic() || matches!(ch, '_' | '.' | '/' | '~')
}

pub(super) fn consume_shell_atom(chars: &[char], mut index: usize) -> usize {
    while index < chars.len()
        && !chars[index].is_whitespace()
        && !matches!(chars[index], '|' | '&' | ';' | '(' | ')' | '<' | '>')
    {
        index += 1;
    }
    index
}

pub(super) fn looks_like_shell_assignment(atom: &str) -> bool {
    let Some((name, _value)) = atom.split_once('=') else {
        return false;
    };
    !name.is_empty()
        && name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        && name
            .chars()
            .next()
            .map(|ch| ch.is_ascii_alphabetic() || ch == '_')
            .unwrap_or(false)
}

#[cfg(test)]
pub(super) fn is_known_shell_command(command: &str) -> bool {
    matches!(
        command,
        "cargo"
            | "git"
            | "npm"
            | "pnpm"
            | "yarn"
            | "bun"
            | "node"
            | "deno"
            | "python"
            | "python3"
            | "pip"
            | "pip3"
            | "uv"
            | "pytest"
            | "go"
            | "rustc"
            | "docker"
            | "docker-compose"
            | "kubectl"
            | "make"
            | "cmake"
            | "just"
            | "ls"
            | "cat"
            | "cd"
            | "cp"
            | "mv"
            | "rm"
            | "echo"
            | "printf"
            | "find"
            | "grep"
            | "sed"
            | "awk"
            | "curl"
            | "wget"
            | "ssh"
            | "scp"
            | "rsync"
            | "chmod"
            | "chown"
            | "mkdir"
            | "touch"
            | "tail"
            | "head"
            | "less"
            | "vim"
            | "nvim"
            | "code"
            | "bash"
            | "zsh"
            | "sh"
            | "fish"
            | "pwsh"
            | "powershell"
            | "cmd"
            | "sudo"
            | "Get-Item"
            | "Set-Location"
            | "Test-Path"
            | "Get-ChildItem"
            | "New-Item"
            | "Remove-Item"
            | "Copy-Item"
            | "Move-Item"
            | "Write-Host"
    )
}
