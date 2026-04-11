pub(super) fn strip_inline_md(text: &str) -> String {
    render_inline_md(text, false)
}

pub(super) fn render_inline_md(text: &str, ansi: bool) -> String {
    const BOLD_ON: &str = "\x1b[1m";
    const BOLD_OFF: &str = "\x1b[22m";
    const CODE_COLOR: &str = "\x1b[38;2;180;220;170m";
    const RESET: &str = "\x1b[0m";
    const LINK_COLOR: &str = "\x1b[38;2;100;180;255m";

    let mut result = String::new();
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    let mut index = 0;

    while index < len {
        if index + 1 < len && chars[index] == '*' && chars[index + 1] == '*' {
            index += 2;
            let start = index;
            while index + 1 < len && !(chars[index] == '*' && chars[index + 1] == '*') {
                index += 1;
            }
            let content: String = chars[start..index].iter().collect();
            if ansi {
                result.push_str(BOLD_ON);
                result.push_str(&content);
                result.push_str(BOLD_OFF);
            } else {
                result.push_str(&content);
            }
            if index + 1 < len {
                index += 2;
            }
        } else if chars[index] == '`' {
            index += 1;
            let start = index;
            while index < len && chars[index] != '`' {
                index += 1;
            }
            let content: String = chars[start..index].iter().collect();
            if ansi {
                result.push_str(CODE_COLOR);
                result.push_str(&content);
                result.push_str(RESET);
            } else {
                result.push_str(&content);
            }
            if index < len {
                index += 1;
            }
        } else if chars[index] == '[' {
            let bracket_start = index + 1;
            let mut end = bracket_start;
            while end < len && chars[end] != ']' {
                end += 1;
            }
            if end + 1 < len && chars[end] == ']' && chars[end + 1] == '(' {
                let link_text: String = chars[bracket_start..end].iter().collect();
                end += 2;
                while end < len && chars[end] != ')' {
                    end += 1;
                }
                if end < len {
                    end += 1;
                }
                if ansi {
                    result.push_str(LINK_COLOR);
                    result.push_str(&link_text);
                    result.push_str(RESET);
                } else {
                    result.push_str(&link_text);
                }
                index = end;
            } else {
                result.push(chars[index]);
                index += 1;
            }
        } else {
            result.push(chars[index]);
            index += 1;
        }
    }
    result
}
