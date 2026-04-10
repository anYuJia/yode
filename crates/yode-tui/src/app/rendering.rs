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

pub(super) fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == 0x1b {
            i += 1;
            if i < bytes.len() && bytes[i] == b'[' {
                i += 1;
                while i < bytes.len() {
                    let b = bytes[i];
                    i += 1;
                    if (0x40..=0x7e).contains(&b) {
                        break;
                    }
                }
            }
        } else {
            out.push(bytes[i] as char);
            i += 1;
        }
    }
    out
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
    let mut i = 0;
    while i < len {
        if chars[i] == '@' {
            result.push_str(DEC);
            result.push(chars[i]);
            i += 1;
            while i < len && (chars[i].is_alphanumeric() || chars[i] == '_') {
                result.push(chars[i]);
                i += 1;
            }
            result.push_str(RESET);
            result.push_str(BASE);
            continue;
        }

        if chars[i] == '"' || chars[i] == '\'' || chars[i] == '`' {
            let quote = chars[i];
            result.push_str(STRC);
            result.push(quote);
            i += 1;
            while i < len {
                if chars[i] == '\\' && i + 1 < len {
                    result.push(chars[i]);
                    result.push(chars[i + 1]);
                    i += 2;
                } else if chars[i] == quote {
                    break;
                } else {
                    result.push(chars[i]);
                    i += 1;
                }
            }
            if i < len {
                result.push(quote);
                i += 1;
            }
            result.push_str(RESET);
            result.push_str(BASE);
            continue;
        }

        if chars[i] == '#' || (chars[i] == '/' && i + 1 < len && chars[i + 1] == '/') {
            result.push_str(CMT);
            while i < len {
                result.push(chars[i]);
                i += 1;
            }
            break;
        }

        if chars[i].is_alphabetic() || chars[i] == '_' || chars[i] == '@' {
            let start = i;
            while i < len && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
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

        if chars[i].is_ascii_digit() {
            result.push_str(NUM);
            while i < len && (chars[i].is_ascii_digit() || chars[i] == '.' || chars[i] == 'x') {
                result.push(chars[i]);
                i += 1;
            }
            result.push_str(RESET);
            result.push_str(BASE);
            continue;
        }

        if matches!(chars[i], '=' | '+' | '-' | '*' | '/' | '!' | '<' | '>' | '|') {
            result.push_str(OP);
            result.push(chars[i]);
            i += 1;
            result.push_str(RESET);
            result.push_str(BASE);
            continue;
        }

        result.push(chars[i]);
        i += 1;
    }

    result.push_str(RESET);
    result
}

fn is_code_keyword(word: &str) -> bool {
    matches!(
        word,
        "def" | "class" | "if" | "elif" | "else" | "for" | "while" | "return" |
        "import" | "from" | "with" | "try" | "except" | "finally" |
        "raise" | "pass" | "break" | "continue" | "and" | "or" | "not" |
        "None" | "True" | "False" | "self" | "async" | "await" |
        "yield" | "lambda" | "in" | "is" | "as" |
        "const" | "let" | "var" | "function" | "new" | "this" | "typeof" |
        "instanceof" | "export" | "default" | "switch" | "case" |
        "null" | "undefined" | "true" | "false" | "throw" | "catch" |
        "extends" | "implements" | "interface" | "readonly" | "abstract" |
        "fn" | "mut" | "pub" | "struct" | "enum" | "impl" | "trait" |
        "use" | "mod" | "match" | "crate" | "super" | "move" | "dyn" |
        "unsafe" | "extern" | "ref" | "where" | "type" |
        "func" | "package" | "defer" | "chan" | "select" | "range" |
        "void" | "static" | "final" | "private" | "protected" | "public" |
        "override" | "do" | "int" | "string" | "bool" | "float"
    )
}

pub(super) fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().to_string() + c.as_str(),
    }
}

pub(super) fn markdown_to_plain(text: &str) -> String {
    let mut result = String::new();
    let mut in_code_block = false;
    let mut table_rows: Vec<Vec<String>> = Vec::new();

    for line in text.lines() {
        let trimmed = line.trim();
        let is_table_line = !in_code_block
            && trimmed.starts_with('|')
            && trimmed.ends_with('|')
            && trimmed.len() > 1;
        if !table_rows.is_empty() && !is_table_line {
            result.push_str(&render_table(&table_rows));
            table_rows.clear();
        }

        if line.starts_with("```") {
            in_code_block = !in_code_block;
            if in_code_block {
                let lang = line[3..].trim();
                if !lang.is_empty() {
                    result.push_str(&format!("─── {} ───\n", lang));
                } else {
                    result.push('\n');
                }
            } else {
                result.push('\n');
            }
            continue;
        }
        if in_code_block {
            result.push_str(&format!("    {}\n", line));
            continue;
        }

        if is_table_line {
            let inner = &trimmed[1..trimmed.len() - 1];
            let is_separator = inner
                .chars()
                .all(|c| c == '-' || c == ':' || c == '|' || c == ' ');
            if !is_separator {
                let cells: Vec<String> = inner
                    .split('|')
                    .map(|c| strip_inline_md(c.trim()))
                    .collect();
                table_rows.push(cells);
            }
            continue;
        }

        if (trimmed.starts_with("---") || trimmed.starts_with("***") || trimmed.starts_with("___"))
            && trimmed.len() >= 3
            && trimmed
                .chars()
                .all(|c| c == '-' || c == '*' || c == '_' || c == ' ')
        {
            result.push_str("────────────────────────────────\n\n");
            continue;
        }

        if trimmed.starts_with("#### ") {
            if !result.is_empty() && !result.ends_with("\n\n") {
                result.push('\n');
            }
            result.push_str(&format!("  ▹ {}\n", strip_inline_md(&trimmed[5..])));
            continue;
        }
        if trimmed.starts_with("### ") {
            if !result.is_empty() && !result.ends_with("\n\n") {
                result.push('\n');
            }
            result.push_str(&format!("▸ {}\n", strip_inline_md(&trimmed[4..])));
            continue;
        }
        if trimmed.starts_with("## ") {
            if !result.is_empty() && !result.ends_with("\n\n") {
                result.push('\n');
            }
            result.push_str(&format!("── {}\n\n", strip_inline_md(&trimmed[3..])));
            continue;
        }
        if trimmed.starts_with("# ") {
            if !result.is_empty() && !result.ends_with("\n\n") {
                result.push('\n');
            }
            result.push_str(&format!("━━ {}\n\n", strip_inline_md(&trimmed[2..])));
            continue;
        }

        if trimmed.starts_with("- [x] ") || trimmed.starts_with("- [X] ") {
            result.push_str(&format!("☑ {}\n", strip_inline_md(&trimmed[6..])));
            continue;
        }
        if trimmed.starts_with("- [ ] ") {
            result.push_str(&format!("☐ {}\n", strip_inline_md(&trimmed[6..])));
            continue;
        }

        if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
            let src_indent = line.len() - line.trim_start().len();
            let pad = " ".repeat(2 + src_indent);
            result.push_str(&format!("{}• {}\n", pad, strip_inline_md(&trimmed[2..])));
            continue;
        }

        if let Some(dot_pos) = trimmed.find(". ") {
            if dot_pos <= 3 && dot_pos > 0 && trimmed[..dot_pos].chars().all(|c| c.is_ascii_digit())
            {
                let num = &trimmed[..dot_pos];
                let content = &trimmed[dot_pos + 2..];
                let src_indent = line.len() - line.trim_start().len();
                let pad = " ".repeat(2 + src_indent);
                result.push_str(&format!("{}{}. {}\n", pad, num, strip_inline_md(content)));
                continue;
            }
        }

        if trimmed.starts_with("> ") {
            result.push_str(&format!("▎ {}\n", strip_inline_md(&trimmed[2..])));
            continue;
        }

        result.push_str(&strip_inline_md(line));
        result.push('\n');
    }

    if !table_rows.is_empty() {
        result.push_str(&render_table(&table_rows));
    }

    if result.ends_with('\n') {
        result.pop();
    }
    result
}

fn render_table(rows: &[Vec<String>]) -> String {
    use unicode_width::UnicodeWidthStr;

    if rows.is_empty() {
        return String::new();
    }

    let num_cols = rows.iter().map(|r| r.len()).max().unwrap_or(0);
    let mut widths = vec![0usize; num_cols];
    for row in rows {
        for (i, cell) in row.iter().enumerate() {
            if i < num_cols {
                widths[i] = widths[i].max(UnicodeWidthStr::width(cell.as_str()));
            }
        }
    }

    let mut result = String::new();
    for (row_idx, row) in rows.iter().enumerate() {
        result.push_str("  ");
        for (i, cell) in row.iter().enumerate() {
            if i >= num_cols {
                break;
            }
            let w = widths[i];
            let cell_w = UnicodeWidthStr::width(cell.as_str());
            let pad = w.saturating_sub(cell_w);
            if i > 0 {
                result.push_str(" │ ");
            }
            result.push_str(cell);
            result.push_str(&" ".repeat(pad));
        }
        result.push('\n');

        if row_idx == 0 && rows.len() > 1 {
            result.push_str("  ");
            for (i, w) in widths.iter().enumerate() {
                if i > 0 {
                    result.push_str("─┼─");
                }
                result.push_str(&"─".repeat(*w));
            }
            result.push('\n');
        }
    }

    result
}

pub(super) fn process_md_line(line: &str, in_code_block: &mut bool) -> String {
    let trimmed = line.trim();

    if trimmed.starts_with("```") {
        let was_in_block = *in_code_block;
        *in_code_block = !*in_code_block;
        if !was_in_block {
            let lang = trimmed[3..].trim();
            if !lang.is_empty() {
                return format!("\x1b[38;2;100;100;120m─── {} ───\x1b[0m", lang);
            }
        }
        return String::new();
    }

    if *in_code_block {
        return format!("    {}", line);
    }

    if (trimmed.starts_with("---") || trimmed.starts_with("***"))
        && trimmed.len() >= 3
        && trimmed
            .chars()
            .all(|c| c == '-' || c == '*' || c == '_' || c == ' ')
    {
        return "────────────────────────────────".to_string();
    }
    if trimmed.starts_with('|') && trimmed.ends_with('|') && trimmed.len() > 1 {
        let inner = &trimmed[1..trimmed.len() - 1];
        let is_separator = inner
            .chars()
            .all(|c| c == '-' || c == ':' || c == '|' || c == ' ');
        if is_separator {
            return "  ──────────────────────────".to_string();
        }
        let cells: Vec<String> = inner
            .split('|')
            .map(|c| render_inline_md(c.trim(), true))
            .collect();
        return format!("  {}", cells.join("  │  "));
    }
    if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
        let src_indent = line.len() - line.trim_start().len();
        let pad = " ".repeat(2 + src_indent);
        return format!("{}• {}", pad, render_inline_md(&trimmed[2..], true));
    }
    if let Some(dot_pos) = trimmed.find(". ") {
        if dot_pos <= 3 && dot_pos > 0 && trimmed[..dot_pos].chars().all(|c| c.is_ascii_digit()) {
            let num = &trimmed[..dot_pos];
            let content = &trimmed[dot_pos + 2..];
            let src_indent = line.len() - line.trim_start().len();
            let pad = " ".repeat(2 + src_indent);
            return format!("{}{}. {}", pad, num, render_inline_md(content, true));
        }
    }
    if trimmed.starts_with("- [x] ") || trimmed.starts_with("- [X] ") {
        return format!("☑ {}", render_inline_md(&trimmed[6..], true));
    }
    if trimmed.starts_with("- [ ] ") {
        return format!("☐ {}", render_inline_md(&trimmed[6..], true));
    }
    if trimmed.starts_with("#### ") {
        return format!("  ▹ {}", render_inline_md(&trimmed[5..], true));
    }
    if trimmed.starts_with("### ") {
        return format!("▸ {}", render_inline_md(&trimmed[4..], true));
    }
    if trimmed.starts_with("## ") {
        return format!("── {}", render_inline_md(&trimmed[3..], true));
    }
    if trimmed.starts_with("# ") {
        return format!("━━ {}", render_inline_md(&trimmed[2..], true));
    }
    if trimmed.starts_with("> ") {
        return format!("▎ {}", render_inline_md(&trimmed[2..], true));
    }
    render_inline_md(line, true)
}

fn strip_inline_md(text: &str) -> String {
    render_inline_md(text, false)
}

fn render_inline_md(text: &str, ansi: bool) -> String {
    const BOLD_ON: &str = "\x1b[1m";
    const BOLD_OFF: &str = "\x1b[22m";
    const CODE_COLOR: &str = "\x1b[38;2;180;220;170m";
    const RESET: &str = "\x1b[0m";
    const LINK_COLOR: &str = "\x1b[38;2;100;180;255m";

    let mut result = String::new();
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        if i + 1 < len && chars[i] == '*' && chars[i + 1] == '*' {
            i += 2;
            let start = i;
            while i + 1 < len && !(chars[i] == '*' && chars[i + 1] == '*') {
                i += 1;
            }
            let content: String = chars[start..i].iter().collect();
            if ansi {
                result.push_str(BOLD_ON);
                result.push_str(&content);
                result.push_str(BOLD_OFF);
            } else {
                result.push_str(&content);
            }
            if i + 1 < len {
                i += 2;
            }
        } else if chars[i] == '`' {
            i += 1;
            let start = i;
            while i < len && chars[i] != '`' {
                i += 1;
            }
            let content: String = chars[start..i].iter().collect();
            if ansi {
                result.push_str(CODE_COLOR);
                result.push_str(&content);
                result.push_str(RESET);
            } else {
                result.push_str(&content);
            }
            if i < len {
                i += 1;
            }
        } else if chars[i] == '[' {
            let bracket_start = i + 1;
            let mut j = bracket_start;
            while j < len && chars[j] != ']' {
                j += 1;
            }
            if j + 1 < len && chars[j] == ']' && chars[j + 1] == '(' {
                let link_text: String = chars[bracket_start..j].iter().collect();
                j += 2;
                while j < len && chars[j] != ')' {
                    j += 1;
                }
                if j < len {
                    j += 1;
                }
                if ansi {
                    result.push_str(LINK_COLOR);
                    result.push_str(&link_text);
                    result.push_str(RESET);
                } else {
                    result.push_str(&link_text);
                }
                i = j;
            } else {
                result.push(chars[i]);
                i += 1;
            }
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }
    result
}
