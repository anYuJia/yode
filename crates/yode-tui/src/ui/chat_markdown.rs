use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use super::chat::{CODE_BG, DIM, INLINE_CODE_BG, WHITE, YELLOW};
use super::highlighted_code::render_highlighted_code_block;
use super::palette::{BORDER_MUTED, INFO_COLOR, LIGHT, MUTED, PANEL_ACCENT};
use super::structured_diff::render_structured_diff_block;
use crate::app::rendering::{
    parse_code_language, tokenize_code_line_with_language, CodeLanguage, CodeTokenKind,
};

pub(super) fn render_markdown_impl(text: &str, default_fg: Option<Color>) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let mut in_code_block = false;
    let mut code_block_lines: Vec<String> = Vec::new();
    let mut code_block_language = CodeLanguage::Plain;
    let mut in_table = false;
    let mut table_rows: Vec<Vec<String>> = Vec::new();

    let raw_lines: Vec<&str> = text.lines().collect();
    let mut i = 0;

    while i < raw_lines.len() {
        let raw = raw_lines[i];

        if raw.starts_with("```") {
            if in_code_block {
                render_code_block(&mut lines, &code_block_lines, code_block_language);
                code_block_lines.clear();
                in_code_block = false;
                code_block_language = CodeLanguage::Plain;
            } else {
                if in_table {
                    render_table(&mut lines, &table_rows);
                    table_rows.clear();
                    in_table = false;
                }
                let lang = raw.trim_start_matches('`').trim();
                code_block_language = parse_code_language(lang);
                lines.push(Line::from(render_code_block_header(
                    (!lang.is_empty()).then_some(lang),
                    code_block_language,
                )));
                in_code_block = true;
            }
            i += 1;
            continue;
        }
        if in_code_block {
            code_block_lines.push(raw.to_string());
            i += 1;
            continue;
        }

        if raw.contains('|') && raw.trim().starts_with('|') {
            let trimmed = raw.trim();
            if trimmed
                .chars()
                .all(|c| c == '|' || c == '-' || c == ':' || c == ' ')
            {
                in_table = true;
                i += 1;
                continue;
            }
            let cells: Vec<String> = trimmed
                .split('|')
                .filter(|s| !s.is_empty())
                .map(|s| s.trim().to_string())
                .collect();
            if !cells.is_empty() {
                table_rows.push(cells);
                in_table = true;
            }
            i += 1;
            continue;
        } else if in_table {
            render_table(&mut lines, &table_rows);
            table_rows.clear();
            in_table = false;
        }

        let trimmed = raw.trim();
        if (trimmed.starts_with("---") || trimmed.starts_with("***") || trimmed.starts_with("___"))
            && trimmed
                .chars()
                .all(|c| c == '-' || c == '*' || c == '_' || c == ' ')
            && trimmed.len() >= 3
        {
            lines.push(Line::from(Span::styled(
                "  ────────────────────────────────────────",
                Style::default().fg(DIM),
            )));
            i += 1;
            continue;
        }

        if raw.starts_with("### ") {
            lines.push(Line::from(Span::styled(
                format!("  ### {}", &raw[4..]),
                Style::default()
                    .fg(Color::LightBlue)
                    .add_modifier(Modifier::BOLD),
            )));
            i += 1;
            continue;
        }
        if raw.starts_with("## ") {
            lines.push(Line::from(Span::styled(
                format!("  ## {}", &raw[3..]),
                Style::default()
                    .fg(Color::Indexed(51))
                    .add_modifier(Modifier::BOLD),
            )));
            i += 1;
            continue;
        }
        if raw.starts_with("# ") {
            lines.push(Line::from(Span::styled(
                format!("  # {}", &raw[2..]),
                Style::default()
                    .fg(Color::LightYellow)
                    .add_modifier(Modifier::BOLD),
            )));
            i += 1;
            continue;
        }

        if raw.starts_with("> ") || raw == ">" {
            let content = if raw.len() > 2 { &raw[2..] } else { "" };
            let mut spans = vec![Span::styled("  ▎ ", Style::default().fg(Color::DarkGray))];
            spans.extend(parse_inline(content.to_string(), default_fg));
            lines.push(Line::from(spans));
            i += 1;
            continue;
        }

        if raw.starts_with("- [x] ") || raw.starts_with("- [X] ") {
            let content = &raw[6..];
            let mut spans = vec![Span::styled("  ☑ ", Style::default().fg(Color::LightGreen))];
            spans.extend(parse_inline(content.to_string(), default_fg));
            lines.push(Line::from(spans));
            i += 1;
            continue;
        }
        if raw.starts_with("- [ ] ") {
            let content = &raw[6..];
            let mut spans = vec![Span::styled("  ☐ ", Style::default().fg(DIM))];
            spans.extend(parse_inline(content.to_string(), default_fg));
            lines.push(Line::from(spans));
            i += 1;
            continue;
        }

        if raw.starts_with("- ") || raw.starts_with("* ") {
            let mut spans = vec![Span::styled("  • ", Style::default().fg(DIM))];
            spans.extend(parse_inline(raw[2..].to_string(), default_fg));
            lines.push(Line::from(spans));
            i += 1;
            continue;
        }
        if raw.starts_with("  - ") || raw.starts_with("  * ") {
            let mut spans = vec![Span::styled("    ◦ ", Style::default().fg(DIM))];
            spans.extend(parse_inline(raw.trim_start()[2..].to_string(), default_fg));
            lines.push(Line::from(spans));
            i += 1;
            continue;
        }
        if raw.starts_with("    - ") || raw.starts_with("    * ") {
            let mut spans = vec![Span::styled("      ▪ ", Style::default().fg(DIM))];
            spans.extend(parse_inline(raw.trim_start()[2..].to_string(), default_fg));
            lines.push(Line::from(spans));
            i += 1;
            continue;
        }

        if let Some((num, rest)) = try_numbered_list(raw) {
            let mut spans = vec![Span::styled(
                format!("  {}. ", num),
                Style::default().fg(DIM),
            )];
            spans.extend(parse_inline(rest.to_string(), default_fg));
            lines.push(Line::from(spans));
            i += 1;
            continue;
        }

        lines.push(Line::from(parse_inline(raw.to_string(), default_fg)));
        i += 1;
    }

    if in_code_block {
        render_code_block(&mut lines, &code_block_lines, code_block_language);
    }
    if in_table && !table_rows.is_empty() {
        render_table(&mut lines, &table_rows);
    }
    lines
}

#[derive(Clone, Copy)]
struct CodeBlockTheme {
    badge_bg: Color,
    badge_fg: Color,
    border: Color,
    background: Color,
}

fn code_block_theme(language: CodeLanguage) -> CodeBlockTheme {
    match language {
        CodeLanguage::Diff => CodeBlockTheme {
            badge_bg: Color::Indexed(110),
            badge_fg: Color::Indexed(232),
            border: Color::Indexed(67),
            background: Color::Indexed(235),
        },
        CodeLanguage::Shell => CodeBlockTheme {
            badge_bg: Color::Indexed(72),
            badge_fg: Color::Indexed(232),
            border: Color::Indexed(66),
            background: Color::Indexed(235),
        },
        CodeLanguage::Json => CodeBlockTheme {
            badge_bg: Color::Indexed(179),
            badge_fg: Color::Indexed(232),
            border: Color::Indexed(143),
            background: Color::Indexed(235),
        },
        CodeLanguage::Rust => CodeBlockTheme {
            badge_bg: Color::Indexed(173),
            badge_fg: Color::Indexed(232),
            border: Color::Indexed(137),
            background: CODE_BG,
        },
        CodeLanguage::Python => CodeBlockTheme {
            badge_bg: Color::Indexed(110),
            badge_fg: Color::Indexed(232),
            border: Color::Indexed(74),
            background: Color::Indexed(235),
        },
        CodeLanguage::Plain => CodeBlockTheme {
            badge_bg: PANEL_ACCENT,
            badge_fg: Color::Indexed(232),
            border: BORDER_MUTED,
            background: CODE_BG,
        },
    }
}

fn render_code_block_header(label: Option<&str>, language: CodeLanguage) -> Vec<Span<'static>> {
    let theme = code_block_theme(language);
    let label = label.unwrap_or("code");
    vec![
        Span::styled("  ╭─", Style::default().fg(theme.border)),
        Span::styled(
            format!(" {} ", label),
            Style::default()
                .fg(theme.badge_fg)
                .bg(theme.badge_bg)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "──────────────────────────────",
            Style::default().fg(theme.border),
        ),
    ]
}

fn render_code_block_footer(language: CodeLanguage) -> Vec<Span<'static>> {
    let theme = code_block_theme(language);
    vec![Span::styled(
        "  ╰────────────────────────────────────",
        Style::default().fg(theme.border),
    )]
}

fn render_code_block(
    lines: &mut Vec<Line<'static>>,
    code_block_lines: &[String],
    language: CodeLanguage,
) {
    if language == CodeLanguage::Diff {
        lines.extend(render_structured_diff_block(code_block_lines));
        lines.push(Line::from(render_code_block_footer(language)));
        return;
    }

    let theme = code_block_theme(language);
    lines.extend(render_highlighted_code_block(
        code_block_lines,
        language,
        theme.border,
        theme.background,
    ));
    lines.push(Line::from(render_code_block_footer(language)));
}

fn code_token_color(kind: CodeTokenKind, language: CodeLanguage) -> Color {
    match kind {
        CodeTokenKind::Plain => LIGHT,
        CodeTokenKind::String => Color::Indexed(180),
        CodeTokenKind::Number => Color::Indexed(151),
        CodeTokenKind::Keyword => match language {
            CodeLanguage::Shell => Color::Indexed(79),
            CodeLanguage::Rust => Color::Indexed(111),
            CodeLanguage::Python => Color::Indexed(111),
            _ => Color::Indexed(111),
        },
        CodeTokenKind::Comment => MUTED,
        CodeTokenKind::Decorator => match language {
            CodeLanguage::Rust => Color::Indexed(179),
            _ => Color::Indexed(116),
        },
        CodeTokenKind::Operator => INFO_COLOR,
        CodeTokenKind::Property => Color::Indexed(153),
        CodeTokenKind::Variable => Color::Indexed(215),
        CodeTokenKind::DiffAdded => Color::Indexed(114),
        CodeTokenKind::DiffRemoved => Color::Indexed(174),
        CodeTokenKind::DiffHunk => Color::Indexed(110),
        CodeTokenKind::DiffMeta => Color::Indexed(180),
        CodeTokenKind::DiffFile => Color::Indexed(223),
        CodeTokenKind::DiffLineNumber => Color::Indexed(153),
        CodeTokenKind::ShellPrompt => Color::Indexed(109),
        CodeTokenKind::ShellCommand => Color::Indexed(222),
        CodeTokenKind::ShellFlag => Color::Indexed(111),
        CodeTokenKind::ShellPath => Color::Indexed(153),
        CodeTokenKind::ShellInfo => Color::Indexed(153),
        CodeTokenKind::ShellSuccess => Color::Indexed(114),
        CodeTokenKind::ShellWarning => Color::Indexed(179),
        CodeTokenKind::ShellOutput => Color::Indexed(246),
        CodeTokenKind::ShellError => Color::Indexed(210),
    }
}

#[cfg(test)]
mod tests {
    use super::render_markdown_impl;
    use ratatui::style::Color;

    #[test]
    fn fenced_code_blocks_render_header_and_highlighted_tokens() {
        let lines = render_markdown_impl("```rust\nfn main() {}\n```", None);
        assert!(lines[0].to_string().contains("rust"));
        let code_line = lines
            .iter()
            .find(|line| line.to_string().contains("fn main"))
            .unwrap();
        assert!(code_line
            .spans
            .iter()
            .any(|span| span.content == " 1 " && span.style.fg == Some(Color::Indexed(244))));
        assert!(code_line
            .spans
            .iter()
            .any(|span| span.style.fg == Some(Color::Indexed(111))));
    }

    #[test]
    fn diff_code_blocks_render_added_lines_with_gutter_background_and_syntax() {
        let lines = render_markdown_impl(
            "```diff\ndiff --git a/src/main.rs b/src/main.rs\n@@ -0,0 +1,1 @@\n+fn main() {}\n```",
            None,
        );
        let code_line = lines
            .iter()
            .find(|line| line.to_string().contains("fn main"))
            .unwrap();
        assert!(code_line
            .spans
            .iter()
            .any(|span| span.content == "+ 1 " && span.style.fg == Some(Color::Indexed(114))));
        assert!(code_line
            .spans
            .iter()
            .any(|span| span.content == "fn" && span.style.fg == Some(Color::Indexed(111))));
        assert!(code_line
            .spans
            .iter()
            .any(|span| span.style.bg == Some(Color::Indexed(22))));
    }

    #[test]
    fn json_code_blocks_highlight_property_keys() {
        let lines = render_markdown_impl("```json\n{\"name\": \"yode\"}\n```", None);
        let code_line = lines
            .iter()
            .find(|line| line.to_string().contains("\"name\""))
            .unwrap();
        assert!(code_line
            .spans
            .iter()
            .any(|span| span.style.fg == Some(Color::Indexed(153))));
    }

    #[test]
    fn diff_code_blocks_highlight_file_headers_and_hunk_ranges() {
        let lines = render_markdown_impl(
            "```diff\ndiff --git a/src/main.rs b/src/main.rs\n@@ -10,2 +10,4 @@ fn render()\n```",
            None,
        );
        let file_line = lines
            .iter()
            .find(|line| line.to_string().contains("a/src/main.rs"))
            .unwrap();
        assert!(file_line
            .spans
            .iter()
            .any(|span| span.style.fg == Some(Color::Indexed(223))));

        let hunk_line = lines
            .iter()
            .find(|line| line.to_string().contains("@@ -10,2 +10,4 @@"))
            .unwrap();
        assert!(hunk_line
            .spans
            .iter()
            .any(|span| span.style.fg == Some(Color::Indexed(153))));
    }

    #[test]
    fn inline_code_renders_token_spans() {
        let lines = render_markdown_impl("Use `fn main()` here.", None);
        let line = lines
            .iter()
            .find(|line| line.to_string().contains("fn main"))
            .unwrap();
        assert!(line
            .spans
            .iter()
            .any(|span| span.content == "fn" && span.style.fg == Some(Color::Indexed(111))));
    }

    #[test]
    fn shell_code_blocks_render_as_highlighted_code() {
        let lines = render_markdown_impl(
            "```bash\nuser@yode ~/repo $ cargo test -- --nocapture\necho $HOME\n```",
            None,
        );
        let command_line = lines
            .iter()
            .find(|line| line.to_string().contains("cargo test"))
            .unwrap();
        assert!(command_line
            .spans
            .iter()
            .any(|span| span.content == " 1 " && span.style.fg == Some(Color::Indexed(244))));
        assert!(command_line
            .spans
            .iter()
            .any(|span| span.content == "cargo" && span.style.fg == Some(Color::Indexed(222))));
        assert!(command_line.spans.iter().any(
            |span| span.content == "--nocapture" && span.style.fg == Some(Color::Indexed(111))
        ));

        let second_line = lines
            .iter()
            .find(|line| line.to_string().contains("echo $HOME"))
            .unwrap();
        assert!(second_line
            .spans
            .iter()
            .any(|span| span.content == " 2 " && span.style.fg == Some(Color::Indexed(244))));
        assert!(second_line
            .spans
            .iter()
            .any(|span| span.content == "echo"
                && span.style.fg == Some(Color::Indexed(222))));
        assert!(second_line
            .spans
            .iter()
            .any(|span| span.content == "$HOME" && span.style.fg == Some(Color::Indexed(215))));
    }

    #[test]
    fn shell_code_blocks_render_as_generic_highlighted_code_without_transcript_gutter() {
        let lines = render_markdown_impl(
            "```bash\nPS C:\\repo> cargo test `\n>> -- --nocapture\n./scripts/run.sh --config ./cfg.json\n```",
            None,
        );
        let continuation_line = lines
            .iter()
            .find(|line| line.to_string().contains("-- --nocapture"))
            .unwrap();
        assert!(continuation_line
            .spans
            .iter()
            .any(|span| span.content == " 2 " && span.style.fg == Some(Color::Indexed(244))));

        let path_line = lines
            .iter()
            .find(|line| line.to_string().contains("./scripts/run.sh"))
            .unwrap();
        assert!(path_line
            .spans
            .iter()
            .any(|span| span.content.contains("./scripts/run.sh")
                && span.style.fg == Some(Color::Indexed(222))));
        assert!(path_line
            .spans
            .iter()
            .any(|span| span.content.contains("./cfg.json")
                && span.style.fg == Some(Color::Indexed(153))));
    }

    #[test]
    fn shell_code_blocks_highlight_output_paths_and_numbers() {
        let lines = render_markdown_impl(
            "```bash\ncat /Users/pyu/code/yode/package.json\nprintf 14\n```",
            None,
        );
        let compiling_line = lines
            .iter()
            .find(|line| line.to_string().contains("/Users/pyu/code/yode/package.json"))
            .unwrap();
        assert!(compiling_line
            .spans
            .iter()
            .any(|span| span.content.contains("/Users/pyu/code/yode/package.json")
                && span.style.fg == Some(Color::Indexed(153))));

        let count_line = lines
            .iter()
            .find(|line| line.to_string().contains("printf 14"))
            .unwrap();
        assert!(count_line
            .spans
            .iter()
            .any(|span| span.content == "14" && span.style.fg == Some(Color::Indexed(151))));
    }
}

fn render_table(lines: &mut Vec<Line<'static>>, rows: &[Vec<String>]) {
    if rows.is_empty() {
        return;
    }

    let col_count = rows.iter().map(|r| r.len()).max().unwrap_or(0);
    let mut widths = vec![0usize; col_count];
    for row in rows {
        for (j, cell) in row.iter().enumerate() {
            if j < col_count {
                widths[j] = widths[j].max(cell.len());
            }
        }
    }

    for w in &mut widths {
        *w = (*w).min(30);
    }

    if let Some(header) = rows.first() {
        let mut spans = vec![Span::styled("  ", Style::default())];
        for (j, cell) in header.iter().enumerate() {
            let w = widths.get(j).copied().unwrap_or(10);
            spans.push(Span::styled(
                format!(" {:<w$} ", cell, w = w),
                Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
            ));
            if j < header.len() - 1 {
                spans.push(Span::styled("│", Style::default().fg(DIM)));
            }
        }
        lines.push(Line::from(spans));

        let sep: String = widths
            .iter()
            .map(|w| "─".repeat(w + 2))
            .collect::<Vec<_>>()
            .join("┼");
        lines.push(Line::from(Span::styled(
            format!("  {}", sep),
            Style::default().fg(DIM),
        )));
    }

    for row in rows.iter().skip(1) {
        let mut spans = vec![Span::styled("  ", Style::default())];
        for (j, cell) in row.iter().enumerate() {
            let w = widths.get(j).copied().unwrap_or(10);
            spans.push(Span::styled(
                format!(" {:<w$} ", cell, w = w),
                Style::default().fg(WHITE),
            ));
            if j < row.len() - 1 {
                spans.push(Span::styled("│", Style::default().fg(DIM)));
            }
        }
        lines.push(Line::from(spans));
    }
}

fn try_numbered_list(line: &str) -> Option<(&str, &str)> {
    let trimmed = line.trim_start();
    let dot = trimmed.find(". ")?;
    let num = &trimmed[..dot];
    if num.len() <= 3 && num.chars().all(|c| c.is_ascii_digit()) {
        Some((num, &trimmed[dot + 2..]))
    } else {
        None
    }
}

fn parse_inline(text: String, default_fg: Option<Color>) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut remaining: &str = &text;
    let default_style = default_fg
        .map(|fg| Style::default().fg(fg))
        .unwrap_or_default();

    while !remaining.is_empty() {
        if let Some(pos) = remaining.find("**") {
            if pos > 0 {
                spans.push(Span::styled(remaining[..pos].to_string(), default_style));
            }
            remaining = &remaining[pos + 2..];
            if let Some(end) = remaining.find("**") {
                spans.push(Span::styled(
                    remaining[..end].to_string(),
                    default_style.add_modifier(Modifier::BOLD),
                ));
                remaining = &remaining[end + 2..];
            } else {
                spans.push(Span::styled("**".to_string(), default_style));
            }
        } else if let Some(pos) = remaining.find('`') {
            if pos > 0 {
                spans.push(Span::styled(remaining[..pos].to_string(), default_style));
            }
            remaining = &remaining[pos + 1..];
            if let Some(end) = remaining.find('`') {
                spans.extend(render_inline_code_spans(&remaining[..end]));
                remaining = &remaining[end + 1..];
            } else {
                spans.push(Span::styled("`".to_string(), default_style));
            }
        } else {
            spans.push(Span::styled(remaining.to_string(), default_style));
            break;
        }
    }
    if spans.is_empty() {
        spans.push(Span::styled(String::new(), default_style));
    }
    spans
}

fn render_inline_code_spans(text: &str) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    for token in tokenize_code_line_with_language(text, CodeLanguage::Plain) {
        spans.push(Span::styled(
            token.text,
            Style::default()
                .fg(inline_code_token_color(token.kind))
                .bg(INLINE_CODE_BG),
        ));
    }
    if spans.is_empty() {
        spans.push(Span::styled(
            String::new(),
            Style::default().fg(YELLOW).bg(INLINE_CODE_BG),
        ));
    }
    spans
}

fn inline_code_token_color(kind: CodeTokenKind) -> Color {
    match kind {
        CodeTokenKind::Plain => YELLOW,
        _ => code_token_color(kind, CodeLanguage::Plain),
    }
}
