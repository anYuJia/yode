mod code;
mod markdown;

pub(super) fn truncate_line(line: &str, max_chars: usize) -> String {
    code::truncate_line(line, max_chars)
}

pub(super) fn strip_ansi(text: &str) -> String {
    code::strip_ansi(text)
}

pub(super) fn is_code_block_line(text: &str) -> bool {
    code::is_code_block_line(text)
}

pub(super) fn highlight_code_line(line: &str) -> String {
    code::highlight_code_line(line)
}

pub(super) fn capitalize(text: &str) -> String {
    code::capitalize(text)
}

pub(super) fn markdown_to_plain(text: &str) -> String {
    markdown::markdown_to_plain(text)
}

pub(super) fn process_md_line(line: &str, in_code_block: &mut bool) -> String {
    markdown::process_md_line(line, in_code_block)
}
