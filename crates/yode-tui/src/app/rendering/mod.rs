mod code;

pub(crate) use code::{
    detect_code_language_from_path, parse_code_language, tokenize_code_line_with_language,
    CodeLanguage, CodeTokenKind,
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
