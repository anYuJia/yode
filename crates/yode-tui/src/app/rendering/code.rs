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

pub(super) fn highlight_code_line(line: &str) -> String {
    const RESET: &str = "\x1b[0m";
    const BASE: &str = "\x1b[38;2;220;220;220m";
    const STRC: &str = "\x1b[38;2;206;145;120m";
    const NUM: &str = "\x1b[38;2;181;206;168m";
    const KW: &str = "\x1b[38;2;86;156;214m";
    const CMT: &str = "\x1b[38;2;106;153;85m";
    const DEC: &str = "\x1b[38;2;78;201;176m";
    const OP: &str = "\x1b[38;2;212;212;212m";

    let mut result = String::new();
    result.push_str(BASE);

    let chars: Vec<char> = line.chars().collect();
    let len = chars.len();
    let mut index = 0;
    while index < len {
        if chars[index] == '@' {
            result.push_str(DEC);
            result.push(chars[index]);
            index += 1;
            while index < len && (chars[index].is_alphanumeric() || chars[index] == '_') {
                result.push(chars[index]);
                index += 1;
            }
            result.push_str(RESET);
            result.push_str(BASE);
            continue;
        }

        if chars[index] == '"' || chars[index] == '\'' || chars[index] == '`' {
            let quote = chars[index];
            result.push_str(STRC);
            result.push(quote);
            index += 1;
            while index < len {
                if chars[index] == '\\' && index + 1 < len {
                    result.push(chars[index]);
                    result.push(chars[index + 1]);
                    index += 2;
                } else if chars[index] == quote {
                    break;
                } else {
                    result.push(chars[index]);
                    index += 1;
                }
            }
            if index < len {
                result.push(quote);
                index += 1;
            }
            result.push_str(RESET);
            result.push_str(BASE);
            continue;
        }

        if chars[index] == '#' || (chars[index] == '/' && index + 1 < len && chars[index + 1] == '/')
        {
            result.push_str(CMT);
            while index < len {
                result.push(chars[index]);
                index += 1;
            }
            break;
        }

        if chars[index].is_alphabetic() || chars[index] == '_' || chars[index] == '@' {
            let start = index;
            while index < len && (chars[index].is_alphanumeric() || chars[index] == '_') {
                index += 1;
            }
            let word: String = chars[start..index].iter().collect();
            if is_code_keyword(&word) {
                result.push_str(KW);
                result.push_str(&word);
                result.push_str(RESET);
                result.push_str(BASE);
            } else {
                result.push_str(&word);
            }
            continue;
        }

        if chars[index].is_ascii_digit() {
            result.push_str(NUM);
            while index < len
                && (chars[index].is_ascii_digit() || chars[index] == '.' || chars[index] == 'x')
            {
                result.push(chars[index]);
                index += 1;
            }
            result.push_str(RESET);
            result.push_str(BASE);
            continue;
        }

        if matches!(
            chars[index],
            '=' | '+' | '-' | '*' | '/' | '!' | '<' | '>' | '|'
        ) {
            result.push_str(OP);
            result.push(chars[index]);
            index += 1;
            result.push_str(RESET);
            result.push_str(BASE);
            continue;
        }

        result.push(chars[index]);
        index += 1;
    }

    result.push_str(RESET);
    result
}

fn is_code_keyword(word: &str) -> bool {
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

pub(super) fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().to_string() + chars.as_str(),
    }
}
