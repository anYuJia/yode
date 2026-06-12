pub mod languages;
pub mod shell;
pub mod themes;

use languages::{
    is_plain_keyword, is_python_keyword, is_rust_keyword, tokenize_diff_line,
    tokenize_generic_line, tokenize_json_line, CommentStyle, SyntaxProfile,
};
use shell::{shell_prompt_prefix, tokenize_shell_command_line};
#[cfg(test)]
pub use shell::{tokenize_shell_session_line, ShellLineKind, ShellSessionState};
use themes::ANSI_THEME;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodeLanguage {
    Plain,
    Diff,
    Shell,
    Json,
    Rust,
    Python,
}

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
    ShellError,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CodeToken {
    pub text: String,
    pub kind: CodeTokenKind,
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
    let mut index = 0usize;
    let mut last_plain = 0usize;

    while index < bytes.len() {
        if bytes[index] != 0x1b {
            index += 1;
            continue;
        }

        if last_plain < index {
            output.push_str(&text[last_plain..index]);
        }

        index += 1;
        if index >= bytes.len() {
            break;
        }

        match bytes[index] {
            b'[' => {
                index += 1;
                while index < bytes.len() {
                    let byte = bytes[index];
                    index += 1;
                    if (0x40..=0x7e).contains(&byte) {
                        break;
                    }
                }
            }
            b']' => {
                index += 1;
                while index < bytes.len() {
                    if bytes[index] == 0x07 {
                        index += 1;
                        break;
                    }
                    if bytes[index] == 0x1b && index + 1 < bytes.len() && bytes[index + 1] == b'\\'
                    {
                        index += 2;
                        break;
                    }
                    index += 1;
                }
            }
            _ => {
                while index < bytes.len() && (0x20..=0x2f).contains(&bytes[index]) {
                    index += 1;
                }
                if index < bytes.len() {
                    index += 1;
                }
            }
        }

        last_plain = index;
    }

    if last_plain < text.len() {
        output.push_str(&text[last_plain..]);
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

    let file_name = normalized.rsplit(['/', '\\']).next().unwrap_or(normalized);

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
    if let Some(header_lang) = code_block_header_language(line) {
        let label = code_block_header_label(line).unwrap_or("code");
        let accent = match header_lang {
            CodeLanguage::Diff => ANSI_THEME.diff_hunk,
            CodeLanguage::Shell => ANSI_THEME.decorator,
            CodeLanguage::Json => ANSI_THEME.property,
            CodeLanguage::Rust => ANSI_THEME.diff_meta,
            CodeLanguage::Python => ANSI_THEME.keyword,
            CodeLanguage::Plain => ANSI_THEME.keyword,
        };
        return format!(
            "{}─── {}{}{}{} ───{}",
            ANSI_THEME.base, ANSI_THEME.reset, accent, label, ANSI_THEME.base, ANSI_THEME.reset
        );
    }

    let mut result = String::new();
    result.push_str(ANSI_THEME.base);
    for token in tokenize_code_line_with_language(line, language.unwrap_or(CodeLanguage::Plain)) {
        append_ansi_token(&mut result, &token, &ANSI_THEME);
    }

    result.push_str(ANSI_THEME.reset);
    result
}

pub(crate) fn tokenize_code_line_with_language(
    line: &str,
    language: CodeLanguage,
) -> Vec<CodeToken> {
    match language {
        CodeLanguage::Diff => tokenize_diff_line(line),
        CodeLanguage::Json => tokenize_json_line(line),
        CodeLanguage::Shell => tokenize_shell_command_line(
            line,
            shell_prompt_prefix(line).map(|prompt| prompt.prefix_len),
        ),
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

fn append_ansi_token(result: &mut String, token: &CodeToken, theme: &themes::AnsiTheme) {
    let Some(color) = theme.color_for(token.kind) else {
        result.push_str(&token.text);
        return;
    };

    result.push_str(color);
    result.push_str(&token.text);
    result.push_str(theme.reset);
    result.push_str(theme.base);
}

#[cfg(test)]
mod tests {
    use super::{
        code_block_header_language, detect_code_language_from_path, parse_code_language,
        strip_ansi, tokenize_code_line_with_language, tokenize_shell_session_line, CodeLanguage,
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
    fn strip_ansi_preserves_unicode_text() {
        let text = "\x1b[31m∴ Thinking…\x1b[0m";
        assert_eq!(strip_ansi(text), "∴ Thinking…");
    }

    #[test]
    fn strip_ansi_removes_osc8_hyperlinks() {
        let text = "\x1b]8;;https://example.com\x07example\x1b]8;;\x07";
        assert_eq!(strip_ansi(text), "example");
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
        let (kind, tokens, next_session_state) =
            tokenize_shell_session_line("running 14 tests", ShellSessionState::AfterCommand);

        assert_eq!(kind, ShellLineKind::Output);
        assert!(tokens
            .iter()
            .any(|token| token.text.contains("running") && token.kind == CodeTokenKind::Plain));
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
