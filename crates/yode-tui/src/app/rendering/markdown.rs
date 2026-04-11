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
                    .map(|cell| strip_inline_md(cell.trim()))
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

    let num_cols = rows.iter().map(|row| row.len()).max().unwrap_or(0);
    let mut widths = vec![0usize; num_cols];
    for row in rows {
        for (index, cell) in row.iter().enumerate() {
            if index < num_cols {
                widths[index] = widths[index].max(UnicodeWidthStr::width(cell.as_str()));
            }
        }
    }

    let mut result = String::new();
    for (row_index, row) in rows.iter().enumerate() {
        result.push_str("  ");
        for (index, cell) in row.iter().enumerate() {
            if index >= num_cols {
                break;
            }
            let width = widths[index];
            let cell_width = UnicodeWidthStr::width(cell.as_str());
            let pad = width.saturating_sub(cell_width);
            if index > 0 {
                result.push_str(" │ ");
            }
            result.push_str(cell);
            result.push_str(&" ".repeat(pad));
        }
        result.push('\n');

        if row_index == 0 && rows.len() > 1 {
            result.push_str("  ");
            for (index, width) in widths.iter().enumerate() {
                if index > 0 {
                    result.push_str("─┼─");
                }
                result.push_str(&"─".repeat(*width));
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
            .map(|cell| render_inline_md(cell.trim(), true))
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
