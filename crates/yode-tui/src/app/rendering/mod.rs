mod code;
mod markdown;

pub(crate) use code::{
    code_block_header_language, detect_code_language_from_path, parse_code_language,
    tokenize_code_line_with_language, CodeLanguage, CodeTokenKind, ShellSessionState,
};

pub(crate) fn truncate_line(line: &str, max_chars: usize) -> String {
    code::truncate_line(line, max_chars)
}

pub(crate) fn strip_ansi(text: &str) -> String {
    code::strip_ansi(text)
}

pub(crate) fn is_code_block_line(text: &str) -> bool {
    code::is_code_block_line(text)
}

pub(crate) fn highlight_code_line(line: &str) -> String {
    code::highlight_code_line(line, None)
}

pub(crate) fn highlight_code_line_with_language(
    line: &str,
    language: Option<CodeLanguage>,
) -> String {
    code::highlight_code_line(line, language)
}

pub(crate) fn highlight_code_line_in_block(
    line: &str,
    language: Option<CodeLanguage>,
    _shell_session_state: &mut ShellSessionState,
) -> String {
    if code::code_block_header_language(line).is_some() {
        return code::highlight_code_line(line, language);
    }
    code::highlight_code_line(line, language)
}

pub(crate) fn capitalize(text: &str) -> String {
    code::capitalize(text)
}

pub(crate) fn markdown_to_plain(text: &str) -> String {
    markdown::markdown_to_plain(text)
}

pub(crate) fn process_md_line(
    line: &str,
    in_code_block: &mut bool,
    code_language: &mut Option<CodeLanguage>,
) -> String {
    markdown::process_md_line(line, in_code_block, code_language)
}
