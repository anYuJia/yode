#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodeLanguage {
    Plain,
    Diff,
    Shell,
    Json,
    Rust,
    Python,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CodeTokenKind {
    Plain,
    String,
    Number,
    Keyword,
    Comment,
    Decorator,
    Operator,
    Property,
    Variable,
    DiffAdded,
    DiffRemoved,
    DiffHunk,
    DiffMeta,
    DiffFile,
    DiffLineNumber,
    ShellPrompt,
    ShellCommand,
    ShellFlag,
    ShellPath,
    ShellInfo,
    ShellSuccess,
    ShellWarning,
    ShellOutput,
    ShellError,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CodeToken {
    pub text: String,
    pub kind: CodeTokenKind,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ShellLineKind {
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

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ShellSessionState {
    #[default]
    Idle,
    AfterCommand,
    InOutput,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ShellPromptKind {
    Primary,
    Continuation,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ShellPromptMatch {
    prefix_len: usize,
    kind: ShellPromptKind,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ShellOutputStyle {
    Info,
    Success,
    Warning,
    Error,
}

pub(super) fn truncate_line(line: &str, max_chars: usize) -> String {
    let chars: Vec<char> = line.chars().collect();
    if chars.len() <= max_chars {
        return line.to_string();
    }
    if max_chars <= 1 {
        return "…".to_string();
    }
    let kept: String = chars.into_iter().take(max_chars - 1).collect();
    format!("{}…", kept)
}

pub(super) fn strip_ansi(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == 0x1b {
            index += 1;
            if index < bytes.len() && bytes[index] == b'[' {
                index += 1;
                while index < bytes.len() {
                    let byte = bytes[index];
                    index += 1;
                    if (0x40..=0x7e).contains(&byte) {
                        break;
                    }
                }
            }
        } else {
            output.push(bytes[index] as char);
            index += 1;
        }
    }
    output
}

pub(super) fn is_code_block_line(text: &str) -> bool {
    text.starts_with("    ") || text.starts_with("─── ")
}

pub(crate) fn parse_code_language(label: &str) -> CodeLanguage {
    let normalized = label
        .trim()
        .split(|ch: char| ch.is_whitespace() || ch == ',' || ch == '{')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();

    match normalized.as_str() {
        "diff" | "patch" => CodeLanguage::Diff,
        "sh" | "bash" | "zsh" | "fish" | "shell" | "console" | "shellsession" | "pwsh"
        | "powershell" | "ps1" | "ps" => CodeLanguage::Shell,
        "json" | "jsonc" => CodeLanguage::Json,
        "rust" | "rs" => CodeLanguage::Rust,
        "python" | "py" | "python3" => CodeLanguage::Python,
        _ => CodeLanguage::Plain,
    }
}

pub(crate) fn detect_code_language_from_path(file_path: &str) -> CodeLanguage {
    let normalized = file_path
        .trim()
        .trim_matches('"')
        .strip_prefix("a/")
        .or_else(|| file_path.trim().trim_matches('"').strip_prefix("b/"))
        .unwrap_or(file_path.trim().trim_matches('"'));

    if normalized.is_empty() || normalized == "/dev/null" {
        return CodeLanguage::Plain;
    }

    let file_name = normalized
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(normalized);

    match file_name {
        "Dockerfile" | "Containerfile" | "Makefile" | "Justfile" => CodeLanguage::Shell,
        _ => match file_name
            .rsplit_once('.')
            .map(|(_, ext)| ext.to_ascii_lowercase())
            .unwrap_or_default()
            .as_str()
        {
            "diff" | "patch" => CodeLanguage::Diff,
            "sh" | "bash" | "zsh" | "fish" | "ps1" | "psm1" => CodeLanguage::Shell,
            "json" | "jsonc" => CodeLanguage::Json,
            "rs" => CodeLanguage::Rust,
            "py" | "pyw" => CodeLanguage::Python,
            _ => CodeLanguage::Plain,
        },
    }
}

pub(crate) fn code_block_header_label(line: &str) -> Option<&str> {
    line.trim()
        .strip_prefix("─── ")?
        .strip_suffix(" ───")
        .filter(|label| !label.is_empty())
}

pub(crate) fn code_block_header_language(line: &str) -> Option<CodeLanguage> {
    code_block_header_label(line).map(parse_code_language)
}

pub(super) fn highlight_code_line(line: &str, language: Option<CodeLanguage>) -> String {
    const RESET: &str = "\x1b[0m";
    const BASE: &str = "\x1b[38;2;220;220;220m";
    const STRC: &str = "\x1b[38;2;206;145;120m";
    const NUM: &str = "\x1b[38;2;181;206;168m";
    const KW: &str = "\x1b[38;2;86;156;214m";
    const CMT: &str = "\x1b[38;2;106;153;85m";
    const DEC: &str = "\x1b[38;2;78;201;176m";
    const OP: &str = "\x1b[38;2;212;212;212m";
    const PROP: &str = "\x1b[38;2;156;220;254m";
    const VAR: &str = "\x1b[38;2;255;203;107m";
    const DIFF_ADD: &str = "\x1b[38;2;106;171;115m";
    const DIFF_REMOVE: &str = "\x1b[38;2;206;102;102m";
    const DIFF_HUNK: &str = "\x1b[38;2;97;175;239m";
    const DIFF_META: &str = "\x1b[38;2;181;140;96m";
    const DIFF_FILE: &str = "\x1b[38;2;214;214;170m";
    const DIFF_LINE_NO: &str = "\x1b[38;2;156;220;254m";
    const SH_PROMPT: &str = "\x1b[38;2;110;180;160m";
    const SH_CMD: &str = "\x1b[38;2;245;214;150m";
    const SH_FLAG: &str = "\x1b[38;2;137;196;244m";
    const SH_PATH: &str = "\x1b[38;2;156;220;254m";
    const SH_INFO: &str = "\x1b[38;2;120;170;220m";
    const SH_SUCCESS: &str = "\x1b[38;2;120;190;130m";
    const SH_WARN: &str = "\x1b[38;2;224;193;108m";
    const SH_OUT: &str = "\x1b[38;2;170;170;170m";
    const SH_ERR: &str = "\x1b[38;2;214;116;116m";

    if let Some(header_lang) = code_block_header_language(line) {
        let label = code_block_header_label(line).unwrap_or("code");
        let accent = match header_lang {
            CodeLanguage::Diff => DIFF_HUNK,
            CodeLanguage::Shell => DEC,
            CodeLanguage::Json => PROP,
            CodeLanguage::Rust => DIFF_META,
            CodeLanguage::Python => KW,
            CodeLanguage::Plain => KW,
        };
        return format!("{BASE}─── {RESET}{accent}{label}{RESET}{BASE} ───{RESET}");
    }

    let mut result = String::new();
    result.push_str(BASE);
    for token in tokenize_code_line_with_language(line, language.unwrap_or(CodeLanguage::Plain)) {
        append_ansi_token(
            &mut result,
            &token,
            BASE,
            RESET,
            STRC,
            NUM,
            KW,
            CMT,
            DEC,
            OP,
            PROP,
            VAR,
            DIFF_ADD,
            DIFF_REMOVE,
            DIFF_HUNK,
            DIFF_META,
            DIFF_FILE,
            DIFF_LINE_NO,
            SH_PROMPT,
            SH_CMD,
            SH_FLAG,
            SH_PATH,
            SH_INFO,
            SH_SUCCESS,
            SH_WARN,
            SH_OUT,
            SH_ERR,
        );
    }

    result.push_str(RESET);
    result
}

#[allow(dead_code)]
pub(crate) fn highlight_shell_session_line(
    line: &str,
    session_state: ShellSessionState,
) -> (String, ShellLineKind, ShellSessionState) {
    const RESET: &str = "\x1b[0m";
    const BASE: &str = "\x1b[38;2;220;220;220m";
    const STRC: &str = "\x1b[38;2;206;145;120m";
    const NUM: &str = "\x1b[38;2;181;206;168m";
    const KW: &str = "\x1b[38;2;86;156;214m";
    const CMT: &str = "\x1b[38;2;106;153;85m";
    const DEC: &str = "\x1b[38;2;78;201;176m";
    const OP: &str = "\x1b[38;2;212;212;212m";
    const PROP: &str = "\x1b[38;2;156;220;254m";
    const VAR: &str = "\x1b[38;2;255;203;107m";
    const DIFF_ADD: &str = "\x1b[38;2;106;171;115m";
    const DIFF_REMOVE: &str = "\x1b[38;2;206;102;102m";
    const DIFF_HUNK: &str = "\x1b[38;2;97;175;239m";
    const DIFF_META: &str = "\x1b[38;2;181;140;96m";
    const DIFF_FILE: &str = "\x1b[38;2;214;214;170m";
    const DIFF_LINE_NO: &str = "\x1b[38;2;156;220;254m";
    const SH_PROMPT: &str = "\x1b[38;2;110;180;160m";
    const SH_CMD: &str = "\x1b[38;2;245;214;150m";
    const SH_FLAG: &str = "\x1b[38;2;137;196;244m";
    const SH_PATH: &str = "\x1b[38;2;156;220;254m";
    const SH_INFO: &str = "\x1b[38;2;120;170;220m";
    const SH_SUCCESS: &str = "\x1b[38;2;120;190;130m";
    const SH_WARN: &str = "\x1b[38;2;224;193;108m";
    const SH_OUT: &str = "\x1b[38;2;170;170;170m";
    const SH_ERR: &str = "\x1b[38;2;214;116;116m";

    let (kind, tokens, next_session_state) = tokenize_shell_session_line(line, session_state);
    let mut result = String::new();
    result.push_str(BASE);
    for token in tokens {
        append_ansi_token(
            &mut result,
            &token,
            BASE,
            RESET,
            STRC,
            NUM,
            KW,
            CMT,
            DEC,
            OP,
            PROP,
            VAR,
            DIFF_ADD,
            DIFF_REMOVE,
            DIFF_HUNK,
            DIFF_META,
            DIFF_FILE,
            DIFF_LINE_NO,
            SH_PROMPT,
            SH_CMD,
            SH_FLAG,
            SH_PATH,
            SH_INFO,
            SH_SUCCESS,
            SH_WARN,
            SH_OUT,
            SH_ERR,
        );
    }
    result.push_str(RESET);
    (result, kind, next_session_state)
}

pub(crate) fn tokenize_code_line_with_language(
    line: &str,
    language: CodeLanguage,
) -> Vec<CodeToken> {
    match language {
        CodeLanguage::Diff => tokenize_diff_line(line),
        CodeLanguage::Json => tokenize_json_line(line),
        CodeLanguage::Shell => {
            tokenize_shell_command_line(line, shell_prompt_prefix(line).map(|prompt| prompt.prefix_len))
        }
        CodeLanguage::Rust => tokenize_generic_line(
            line,
            SyntaxProfile {
                comment_style: CommentStyle::SlashSlash,
                recognize_at_decorator: false,
                recognize_rust_attribute: true,
                recognize_variables: false,
                keyword_matcher: is_rust_keyword,
            },
        ),
        CodeLanguage::Python => tokenize_generic_line(
            line,
            SyntaxProfile {
                comment_style: CommentStyle::Hash,
                recognize_at_decorator: true,
                recognize_rust_attribute: false,
                recognize_variables: false,
                keyword_matcher: is_python_keyword,
            },
        ),
        CodeLanguage::Plain => tokenize_generic_line(
            line,
            SyntaxProfile {
                comment_style: CommentStyle::HashOrSlashSlash,
                recognize_at_decorator: true,
                recognize_rust_attribute: false,
                recognize_variables: false,
                keyword_matcher: is_plain_keyword,
            },
        ),
    }
}

#[allow(dead_code)]
pub(crate) fn tokenize_shell_session_line(
    line: &str,
    session_state: ShellSessionState,
) -> (ShellLineKind, Vec<CodeToken>, ShellSessionState) {
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

#[derive(Debug, Clone, Copy)]
enum CommentStyle {
    Hash,
    SlashSlash,
    HashOrSlashSlash,
    ShellHash,
}

#[derive(Clone, Copy)]
struct SyntaxProfile {
    comment_style: CommentStyle,
    recognize_at_decorator: bool,
    recognize_rust_attribute: bool,
    recognize_variables: bool,
    keyword_matcher: fn(&str) -> bool,
}

fn tokenize_diff_line(line: &str) -> Vec<CodeToken> {
    let (indent, content) = split_leading_whitespace(line);
    let mut tokens = Vec::new();
    if !indent.is_empty() {
        tokens.push(CodeToken {
            text: indent.to_string(),
            kind: CodeTokenKind::Plain,
        });
    }

    if content.starts_with("@@") {
        tokenize_diff_hunk_line(content, &mut tokens);
    } else if let Some((prefix, path)) = diff_file_header_parts(content) {
        tokens.push(CodeToken {
            text: prefix.to_string(),
            kind: CodeTokenKind::DiffMeta,
        });
        if !path.is_empty() {
            tokens.push(CodeToken {
                text: path.to_string(),
                kind: CodeTokenKind::DiffFile,
            });
        }
    } else {
        let kind = if content.starts_with("diff ") || content.starts_with("index ") {
            CodeTokenKind::DiffMeta
        } else if content.starts_with('+') {
            CodeTokenKind::DiffAdded
        } else if content.starts_with('-') {
            CodeTokenKind::DiffRemoved
        } else {
            CodeTokenKind::Plain
        };

        tokens.push(CodeToken {
            text: content.to_string(),
            kind,
        });
    }
    tokens
}

fn diff_file_header_parts(content: &str) -> Option<(&str, &str)> {
    const PREFIXES: [&str; 7] = [
        "diff --git ",
        "+++ ",
        "--- ",
        "rename from ",
        "rename to ",
        "copy from ",
        "copy to ",
    ];

    PREFIXES
        .into_iter()
        .find_map(|prefix| content.strip_prefix(prefix).map(|rest| (prefix, rest)))
}

fn tokenize_diff_hunk_line(content: &str, tokens: &mut Vec<CodeToken>) {
    if let Some(end) = content[2..].find("@@") {
        let middle_end = 2 + end;
        tokens.push(CodeToken {
            text: "@@".to_string(),
            kind: CodeTokenKind::DiffHunk,
        });
        if middle_end > 2 {
            let middle = &content[2..middle_end];
            push_diff_hunk_ranges(middle, tokens);
        }
        tokens.push(CodeToken {
            text: "@@".to_string(),
            kind: CodeTokenKind::DiffHunk,
        });
        if middle_end + 2 < content.len() {
            tokens.push(CodeToken {
                text: content[middle_end + 2..].to_string(),
                kind: CodeTokenKind::Plain,
            });
        }
    } else {
        tokens.push(CodeToken {
            text: content.to_string(),
            kind: CodeTokenKind::DiffHunk,
        });
    }
}

fn push_diff_hunk_ranges(text: &str, tokens: &mut Vec<CodeToken>) {
    let chars: Vec<char> = text.chars().collect();
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

        if matches!(chars[index], '+' | '-') {
            let start = index;
            index += 1;
            while index < len && (chars[index].is_ascii_digit() || chars[index] == ',') {
                index += 1;
            }
            tokens.push(CodeToken {
                text: chars[start..index].iter().collect(),
                kind: CodeTokenKind::DiffLineNumber,
            });
            continue;
        }

        let start = index;
        while index < len && !chars[index].is_whitespace() {
            index += 1;
        }
        tokens.push(CodeToken {
            text: chars[start..index].iter().collect(),
            kind: CodeTokenKind::Plain,
        });
    }
}

fn tokenize_json_line(line: &str) -> Vec<CodeToken> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = line.chars().collect();
    let len = chars.len();
    let mut index = 0;

    while index < len {
        if chars[index] == '"' {
            let start = index;
            index = consume_quoted_string(&chars, index);
            let kind = if next_non_whitespace(&chars, index) == Some(':') {
                CodeTokenKind::Property
            } else {
                CodeTokenKind::String
            };
            tokens.push(CodeToken {
                text: chars[start..index.min(len)].iter().collect(),
                kind,
            });
            continue;
        }

        if chars[index].is_ascii_digit()
            || (chars[index] == '-' && index + 1 < len && chars[index + 1].is_ascii_digit())
        {
            let start = index;
            index += 1;
            while index < len
                && (chars[index].is_ascii_digit()
                    || matches!(chars[index], '.' | 'e' | 'E' | '+' | '-'))
            {
                index += 1;
            }
            tokens.push(CodeToken {
                text: chars[start..index].iter().collect(),
                kind: CodeTokenKind::Number,
            });
            continue;
        }

        if chars[index].is_alphabetic() {
            let start = index;
            while index < len && chars[index].is_alphabetic() {
                index += 1;
            }
            let word: String = chars[start..index].iter().collect();
            tokens.push(CodeToken {
                text: word.clone(),
                kind: if matches!(word.as_str(), "true" | "false" | "null") {
                    CodeTokenKind::Keyword
                } else {
                    CodeTokenKind::Plain
                },
            });
            continue;
        }

        if matches!(chars[index], '{' | '}' | '[' | ']' | ':' | ',') {
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

#[allow(dead_code)]
fn classify_shell_line(line: &str, session_state: ShellSessionState) -> ShellLineKind {
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

fn tokenize_shell_command_line(line: &str, prompt_prefix_len: Option<usize>) -> Vec<CodeToken> {
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

#[allow(dead_code)]
fn tokenize_shell_output_line(line: &str, kind: ShellLineKind) -> Vec<CodeToken> {
    let mut tokens = Vec::new();
    let mut content = line;
    let default_kind = match kind {
        ShellLineKind::Output => CodeTokenKind::ShellOutput,
        _ => CodeTokenKind::Plain,
    };

    if let Some(prefix_len) = shell_output_prefix_len(line, kind) {
        let prefix_kind = match kind {
            ShellLineKind::Info => CodeTokenKind::ShellInfo,
            ShellLineKind::Success => CodeTokenKind::ShellSuccess,
            ShellLineKind::Warning => CodeTokenKind::ShellWarning,
            ShellLineKind::Error => CodeTokenKind::ShellError,
            ShellLineKind::Output => CodeTokenKind::ShellOutput,
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
                    CodeTokenKind::ShellOutput
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

fn tokenize_generic_line(line: &str, profile: SyntaxProfile) -> Vec<CodeToken> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = line.chars().collect();
    let len = chars.len();
    let mut index = 0;

    while index < len {
        if profile.recognize_rust_attribute
            && chars[index] == '#'
            && index + 1 < len
            && chars[index + 1] == '['
        {
            let start = index;
            index += 2;
            let mut depth = 1;
            while index < len && depth > 0 {
                match chars[index] {
                    '[' => depth += 1,
                    ']' => depth -= 1,
                    _ => {}
                }
                index += 1;
            }
            tokens.push(CodeToken {
                text: chars[start..index.min(len)].iter().collect(),
                kind: CodeTokenKind::Decorator,
            });
            continue;
        }

        if profile.recognize_at_decorator && chars[index] == '@' {
            let start = index;
            index += 1;
            while index < len && is_word_char(chars[index]) {
                index += 1;
            }
            tokens.push(CodeToken {
                text: chars[start..index].iter().collect(),
                kind: CodeTokenKind::Decorator,
            });
            continue;
        }

        if profile.recognize_variables && chars[index] == '$' {
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
                tokens.push(CodeToken {
                    text: chars[start..index.min(len)].iter().collect(),
                    kind: CodeTokenKind::Variable,
                });
                continue;
            }

            if index < len
                && (is_word_start(chars[index])
                    || chars[index].is_ascii_digit()
                    || matches!(chars[index], '?' | '!' | '@' | '*' | '#' | '$'))
            {
                while index < len && (is_word_char(chars[index]) || chars[index].is_ascii_digit()) {
                    index += 1;
                }
                tokens.push(CodeToken {
                    text: chars[start..index].iter().collect(),
                    kind: CodeTokenKind::Variable,
                });
                continue;
            }

            tokens.push(CodeToken {
                text: "$".to_string(),
                kind: CodeTokenKind::Operator,
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

        if starts_comment(&chars, index, profile.comment_style) {
            tokens.push(CodeToken {
                text: chars[index..].iter().collect(),
                kind: CodeTokenKind::Comment,
            });
            break;
        }

        if is_word_start(chars[index]) {
            let start = index;
            while index < len && is_word_char(chars[index]) {
                index += 1;
            }
            let word: String = chars[start..index].iter().collect();
            tokens.push(CodeToken {
                text: word.clone(),
                kind: if (profile.keyword_matcher)(&word) {
                    CodeTokenKind::Keyword
                } else {
                    CodeTokenKind::Plain
                },
            });
            continue;
        }

        if chars[index].is_ascii_digit() {
            let start = index;
            index += 1;
            while index < len
                && (chars[index].is_ascii_digit()
                    || matches!(chars[index], '.' | 'x' | 'X' | '_' | 'a'..='f' | 'A'..='F'))
            {
                index += 1;
            }
            tokens.push(CodeToken {
                text: chars[start..index].iter().collect(),
                kind: CodeTokenKind::Number,
            });
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

fn split_leading_whitespace(line: &str) -> (&str, &str) {
    let split_at = line
        .char_indices()
        .find(|(_, ch)| !ch.is_whitespace())
        .map(|(index, _)| index)
        .unwrap_or(line.len());
    line.split_at(split_at)
}

fn consume_quoted_string(chars: &[char], start: usize) -> usize {
    let len = chars.len();
    let quote = chars[start];
    let mut index = start + 1;
    while index < len {
        if chars[index] == '\\' && index + 1 < len {
            index += 2;
            continue;
        }
        let current = chars[index];
        index += 1;
        if current == quote {
            break;
        }
    }
    index
}

fn next_non_whitespace(chars: &[char], mut index: usize) -> Option<char> {
    while index < chars.len() {
        if !chars[index].is_whitespace() {
            return Some(chars[index]);
        }
        index += 1;
    }
    None
}

fn starts_comment(chars: &[char], index: usize, style: CommentStyle) -> bool {
    match style {
        CommentStyle::Hash => chars[index] == '#',
        CommentStyle::SlashSlash => {
            chars[index] == '/' && index + 1 < chars.len() && chars[index + 1] == '/'
        }
        CommentStyle::HashOrSlashSlash => {
            chars[index] == '#'
                || (chars[index] == '/' && index + 1 < chars.len() && chars[index + 1] == '/')
        }
        CommentStyle::ShellHash => {
            chars[index] == '#' && (index == 0 || chars[index - 1].is_whitespace())
        }
    }
}

#[allow(dead_code)]
fn shell_prompt_prefix(line: &str) -> Option<ShellPromptMatch> {
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

#[allow(dead_code)]
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

#[allow(dead_code)]
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

#[allow(dead_code)]
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

#[allow(dead_code)]
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

#[allow(dead_code)]
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

#[allow(dead_code)]
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

#[allow(dead_code)]
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

#[allow(dead_code)]
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

#[allow(dead_code)]
fn shell_output_style(line: &str) -> Option<ShellOutputStyle> {
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

#[allow(dead_code)]
fn shell_output_prefix_len(line: &str, kind: ShellLineKind) -> Option<usize> {
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

fn consume_shell_output_atom(chars: &[char], mut index: usize) -> usize {
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

fn is_shell_output_atom_start(ch: char) -> bool {
    ch.is_alphanumeric() || matches!(ch, '_' | '.' | '/' | '~')
}

fn looks_like_shell_output_path(atom: &str) -> bool {
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

#[allow(dead_code)]
fn looks_like_new_shell_command(line: &str) -> bool {
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

fn is_shell_flag_start(chars: &[char], index: usize) -> bool {
    chars[index] == '-'
        && index + 1 < chars.len()
        && !chars[index + 1].is_whitespace()
        && chars[index + 1] != '-'
        || (chars[index] == '-'
            && index + 2 < chars.len()
            && chars[index + 1] == '-'
            && !chars[index + 2].is_whitespace())
}

fn is_shell_atom_start(ch: char) -> bool {
    ch.is_alphabetic() || matches!(ch, '_' | '.' | '/' | '~')
}

fn consume_shell_atom(chars: &[char], mut index: usize) -> usize {
    while index < chars.len()
        && !chars[index].is_whitespace()
        && !matches!(chars[index], '|' | '&' | ';' | '(' | ')' | '<' | '>')
    {
        index += 1;
    }
    index
}

fn looks_like_shell_assignment(atom: &str) -> bool {
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

#[allow(dead_code)]
fn is_known_shell_command(command: &str) -> bool {
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

#[allow(clippy::too_many_arguments)]
fn append_ansi_token(
    result: &mut String,
    token: &CodeToken,
    base: &str,
    reset: &str,
    string_color: &str,
    number_color: &str,
    keyword_color: &str,
    comment_color: &str,
    decorator_color: &str,
    operator_color: &str,
    property_color: &str,
    variable_color: &str,
    diff_add_color: &str,
    diff_remove_color: &str,
    diff_hunk_color: &str,
    diff_meta_color: &str,
    diff_file_color: &str,
    diff_line_number_color: &str,
    shell_prompt_color: &str,
    shell_command_color: &str,
    shell_flag_color: &str,
    shell_path_color: &str,
    shell_info_color: &str,
    shell_success_color: &str,
    shell_warning_color: &str,
    shell_output_color: &str,
    shell_error_color: &str,
) {
    let color = match token.kind {
        CodeTokenKind::Plain => {
            result.push_str(&token.text);
            return;
        }
        CodeTokenKind::String => string_color,
        CodeTokenKind::Number => number_color,
        CodeTokenKind::Keyword => keyword_color,
        CodeTokenKind::Comment => comment_color,
        CodeTokenKind::Decorator => decorator_color,
        CodeTokenKind::Operator => operator_color,
        CodeTokenKind::Property => property_color,
        CodeTokenKind::Variable => variable_color,
        CodeTokenKind::DiffAdded => diff_add_color,
        CodeTokenKind::DiffRemoved => diff_remove_color,
        CodeTokenKind::DiffHunk => diff_hunk_color,
        CodeTokenKind::DiffMeta => diff_meta_color,
        CodeTokenKind::DiffFile => diff_file_color,
        CodeTokenKind::DiffLineNumber => diff_line_number_color,
        CodeTokenKind::ShellPrompt => shell_prompt_color,
        CodeTokenKind::ShellCommand => shell_command_color,
        CodeTokenKind::ShellFlag => shell_flag_color,
        CodeTokenKind::ShellPath => shell_path_color,
        CodeTokenKind::ShellInfo => shell_info_color,
        CodeTokenKind::ShellSuccess => shell_success_color,
        CodeTokenKind::ShellWarning => shell_warning_color,
        CodeTokenKind::ShellOutput => shell_output_color,
        CodeTokenKind::ShellError => shell_error_color,
    };

    result.push_str(color);
    result.push_str(&token.text);
    result.push_str(reset);
    result.push_str(base);
}

fn is_word_start(ch: char) -> bool {
    ch.is_alphabetic() || ch == '_'
}

fn is_word_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

fn is_plain_keyword(word: &str) -> bool {
    matches!(
        word,
        "def"
            | "class"
            | "if"
            | "elif"
            | "else"
            | "for"
            | "while"
            | "return"
            | "import"
            | "from"
            | "with"
            | "try"
            | "except"
            | "finally"
            | "raise"
            | "pass"
            | "break"
            | "continue"
            | "and"
            | "or"
            | "not"
            | "None"
            | "True"
            | "False"
            | "self"
            | "async"
            | "await"
            | "yield"
            | "lambda"
            | "in"
            | "is"
            | "as"
            | "const"
            | "let"
            | "var"
            | "function"
            | "new"
            | "this"
            | "typeof"
            | "instanceof"
            | "export"
            | "default"
            | "switch"
            | "case"
            | "null"
            | "undefined"
            | "true"
            | "false"
            | "throw"
            | "catch"
            | "extends"
            | "implements"
            | "interface"
            | "readonly"
            | "abstract"
            | "fn"
            | "mut"
            | "pub"
            | "struct"
            | "enum"
            | "impl"
            | "trait"
            | "use"
            | "mod"
            | "match"
            | "crate"
            | "super"
            | "move"
            | "dyn"
            | "unsafe"
            | "extern"
            | "ref"
            | "where"
            | "type"
            | "func"
            | "package"
            | "defer"
            | "chan"
            | "select"
            | "range"
            | "void"
            | "static"
            | "final"
            | "private"
            | "protected"
            | "public"
            | "override"
            | "do"
            | "int"
            | "string"
            | "bool"
            | "float"
    )
}

fn is_python_keyword(word: &str) -> bool {
    matches!(
        word,
        "def"
            | "class"
            | "if"
            | "elif"
            | "else"
            | "for"
            | "while"
            | "return"
            | "import"
            | "from"
            | "with"
            | "try"
            | "except"
            | "finally"
            | "raise"
            | "pass"
            | "break"
            | "continue"
            | "and"
            | "or"
            | "not"
            | "None"
            | "True"
            | "False"
            | "self"
            | "async"
            | "await"
            | "yield"
            | "lambda"
            | "in"
            | "is"
            | "as"
            | "global"
            | "nonlocal"
            | "assert"
    )
}

fn is_rust_keyword(word: &str) -> bool {
    matches!(
        word,
        "fn" | "let"
            | "mut"
            | "pub"
            | "struct"
            | "enum"
            | "impl"
            | "trait"
            | "use"
            | "mod"
            | "match"
            | "crate"
            | "super"
            | "self"
            | "Self"
            | "move"
            | "dyn"
            | "unsafe"
            | "extern"
            | "ref"
            | "where"
            | "type"
            | "const"
            | "static"
            | "async"
            | "await"
            | "if"
            | "else"
            | "loop"
            | "while"
            | "for"
            | "in"
            | "return"
            | "break"
            | "continue"
            | "true"
            | "false"
            | "None"
            | "Some"
            | "Ok"
            | "Err"
    )
}

fn is_shell_keyword(word: &str) -> bool {
    matches!(
        word,
        "if" | "then"
            | "else"
            | "elif"
            | "fi"
            | "for"
            | "while"
            | "do"
            | "done"
            | "case"
            | "esac"
            | "in"
            | "function"
            | "export"
            | "local"
            | "readonly"
            | "declare"
            | "typeset"
            | "source"
            | "alias"
            | "unset"
    )
}

pub(super) fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().to_string() + chars.as_str(),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        code_block_header_language, detect_code_language_from_path, parse_code_language,
        tokenize_code_line_with_language, tokenize_shell_session_line, CodeLanguage,
        CodeTokenKind, ShellLineKind, ShellSessionState,
    };

    #[test]
    fn tokenize_code_line_marks_keywords_strings_and_comments() {
        let tokens = tokenize_code_line_with_language(
            "fn main() { let x = \"hi\"; // comment }",
            CodeLanguage::Plain,
        );
        assert!(tokens
            .iter()
            .any(|token| token.kind == CodeTokenKind::Keyword));
        assert!(tokens
            .iter()
            .any(|token| token.kind == CodeTokenKind::String));
        assert!(tokens
            .iter()
            .any(|token| token.kind == CodeTokenKind::Comment));
    }

    #[test]
    fn parse_code_language_recognizes_common_aliases() {
        assert_eq!(parse_code_language("bash"), CodeLanguage::Shell);
        assert_eq!(parse_code_language("pwsh"), CodeLanguage::Shell);
        assert_eq!(parse_code_language("rs"), CodeLanguage::Rust);
        assert_eq!(parse_code_language("python3"), CodeLanguage::Python);
    }

    #[test]
    fn detect_code_language_from_path_recognizes_diff_file_paths() {
        assert_eq!(
            detect_code_language_from_path("b/src/main.rs"),
            CodeLanguage::Rust
        );
        assert_eq!(
            detect_code_language_from_path("/Users/pyu/code/yode/test.json"),
            CodeLanguage::Json
        );
        assert_eq!(
            detect_code_language_from_path("PS.ps1"),
            CodeLanguage::Shell
        );
    }

    #[test]
    fn json_tokenizer_marks_properties_and_literals() {
        let tokens = tokenize_code_line_with_language(
            "{\"name\": \"yode\", \"ok\": true, \"count\": 2}",
            CodeLanguage::Json,
        );
        assert!(tokens
            .iter()
            .any(|token| token.kind == CodeTokenKind::Property));
        assert!(tokens
            .iter()
            .any(|token| token.kind == CodeTokenKind::Keyword));
        assert!(tokens
            .iter()
            .any(|token| token.kind == CodeTokenKind::Number));
    }

    #[test]
    fn shell_tokenizer_marks_variables() {
        let tokens = tokenize_code_line_with_language("echo $HOME", CodeLanguage::Shell);
        assert!(tokens
            .iter()
            .any(|token| token.kind == CodeTokenKind::Variable));
    }

    #[test]
    fn shell_session_tokenizer_marks_prompt_command_and_flags() {
        let (kind, tokens, next_session_state) = tokenize_shell_session_line(
            "user@yode ~/repo $ cargo test -- --nocapture",
            ShellSessionState::Idle,
        );

        assert_eq!(kind, ShellLineKind::Prompt);
        assert!(tokens
            .iter()
            .any(|token| token.kind == CodeTokenKind::ShellPrompt));
        assert!(tokens
            .iter()
            .any(|token| token.kind == CodeTokenKind::ShellCommand));
        assert!(tokens
            .iter()
            .any(|token| token.kind == CodeTokenKind::ShellFlag));
        assert_eq!(next_session_state, ShellSessionState::AfterCommand);
    }

    #[test]
    fn shell_session_tokenizer_marks_output_after_prompt() {
        let (kind, tokens, next_session_state) = tokenize_shell_session_line(
            "running 14 tests",
            ShellSessionState::AfterCommand,
        );

        assert_eq!(kind, ShellLineKind::Output);
        assert!(tokens
            .iter()
            .any(|token| token.kind == CodeTokenKind::ShellOutput));
        assert_eq!(next_session_state, ShellSessionState::InOutput);
    }

    #[test]
    fn shell_session_tokenizer_marks_info_success_warning_and_error_outputs() {
        let (info_kind, info_tokens, _) =
            tokenize_shell_session_line("Compiling yode v0.0.11", ShellSessionState::AfterCommand);
        assert_eq!(info_kind, ShellLineKind::Info);
        assert!(info_tokens
            .iter()
            .any(|token| token.kind == CodeTokenKind::ShellInfo));

        let (success_kind, success_tokens, _) = tokenize_shell_session_line(
            "Finished dev [unoptimized + debuginfo]",
            ShellSessionState::AfterCommand,
        );
        assert_eq!(success_kind, ShellLineKind::Success);
        assert!(success_tokens
            .iter()
            .any(|token| token.kind == CodeTokenKind::ShellSuccess));

        let (warning_kind, warning_tokens, _) = tokenize_shell_session_line(
            "warning: unused import: `PathBuf`",
            ShellSessionState::AfterCommand,
        );
        assert_eq!(warning_kind, ShellLineKind::Warning);
        assert!(warning_tokens
            .iter()
            .any(|token| token.kind == CodeTokenKind::ShellWarning));

        let (error_kind, error_tokens, _) = tokenize_shell_session_line(
            "bash: no such file or directory",
            ShellSessionState::AfterCommand,
        );
        assert_eq!(error_kind, ShellLineKind::Error);
        assert!(error_tokens
            .iter()
            .any(|token| token.kind == CodeTokenKind::ShellError));
    }

    #[test]
    fn shell_session_tokenizer_highlights_output_paths_and_counts() {
        let (_kind, info_tokens, _) = tokenize_shell_session_line(
            "Compiling yode v0.0.11 (/Users/pyu/code/yode)",
            ShellSessionState::AfterCommand,
        );
        assert!(info_tokens
            .iter()
            .any(|token| token.kind == CodeTokenKind::ShellPath));

        let (_kind, output_tokens, _) =
            tokenize_shell_session_line("running 14 tests", ShellSessionState::AfterCommand);
        assert!(output_tokens
            .iter()
            .any(|token| token.text == "14" && token.kind == CodeTokenKind::Number));
    }

    #[test]
    fn shell_session_tokenizer_detects_new_command_after_output() {
        let (kind, tokens, next_session_state) =
            tokenize_shell_session_line("cargo fmt --all", ShellSessionState::InOutput);

        assert_eq!(kind, ShellLineKind::Command);
        assert!(tokens
            .iter()
            .any(|token| token.kind == CodeTokenKind::ShellCommand));
        assert_eq!(next_session_state, ShellSessionState::AfterCommand);
    }

    #[test]
    fn shell_comments_do_not_start_transcript_mode() {
        let (kind, _tokens, next_session_state) =
            tokenize_shell_session_line("# explain this script", ShellSessionState::Idle);
        assert_eq!(kind, ShellLineKind::Comment);
        assert_eq!(next_session_state, ShellSessionState::Idle);

        let (next_kind, _tokens, _) = tokenize_shell_session_line("echo ok", next_session_state);
        assert_eq!(next_kind, ShellLineKind::Command);
    }

    #[test]
    fn shell_session_tokenizer_marks_continuation_lines() {
        let (continuation_kind, continuation_tokens, continuation_state) =
            tokenize_shell_session_line(">> --format json", ShellSessionState::AfterCommand);
        assert_eq!(continuation_kind, ShellLineKind::Continuation);
        assert!(continuation_tokens
            .iter()
            .any(|token| token.kind == CodeTokenKind::ShellPrompt));
        assert_eq!(continuation_state, ShellSessionState::AfterCommand);
    }

    #[test]
    fn shell_session_tokenizer_recognizes_powershell_and_fish_prompts() {
        let (powershell_kind, powershell_tokens, _) =
            tokenize_shell_session_line("PS C:\\repo> cargo check", ShellSessionState::Idle);
        assert_eq!(powershell_kind, ShellLineKind::Prompt);
        assert!(powershell_tokens
            .iter()
            .any(|token| token.kind == CodeTokenKind::ShellPrompt));

        let (fish_kind, fish_tokens, _) =
            tokenize_shell_session_line("devbox on main ➜ cargo test", ShellSessionState::Idle);
        assert_eq!(fish_kind, ShellLineKind::Prompt);
        assert!(fish_tokens
            .iter()
            .any(|token| token.kind == CodeTokenKind::ShellCommand));
    }

    #[test]
    fn rust_attributes_are_not_treated_as_comments() {
        let tokens = tokenize_code_line_with_language("#[derive(Debug)]", CodeLanguage::Rust);
        assert_eq!(tokens[0].kind, CodeTokenKind::Decorator);
    }

    #[test]
    fn diff_header_language_is_parsed() {
        assert_eq!(
            code_block_header_language("─── bash ───"),
            Some(CodeLanguage::Shell)
        );
    }

    #[test]
    fn diff_tokenizer_marks_added_lines() {
        let tokens = tokenize_code_line_with_language("+ let answer = 42;", CodeLanguage::Diff);
        assert!(tokens
            .iter()
            .any(|token| token.kind == CodeTokenKind::DiffAdded));
    }

    #[test]
    fn diff_tokenizer_marks_file_headers_and_hunk_ranges() {
        let file_tokens = tokenize_code_line_with_language(
            "diff --git a/src/main.rs b/src/main.rs",
            CodeLanguage::Diff,
        );
        assert!(file_tokens
            .iter()
            .any(|token| token.kind == CodeTokenKind::DiffFile));

        let hunk_tokens =
            tokenize_code_line_with_language("@@ -10,2 +10,4 @@ fn render()", CodeLanguage::Diff);
        assert!(hunk_tokens
            .iter()
            .any(|token| token.kind == CodeTokenKind::DiffLineNumber));
    }
}
