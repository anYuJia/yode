use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use super::chat::{CODE_BG, DIM, INLINE_CODE_BG, WHITE, YELLOW};

pub(super) fn render_markdown_impl(
    text: &str,
    default_fg: Option<Color>,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let mut in_code_block = false;
    let mut code_block_lines: Vec<String> = Vec::new();
    let mut in_table = false;
    let mut table_rows: Vec<Vec<String>> = Vec::new();

    let raw_lines: Vec<&str> = text.lines().collect();
    let mut i = 0;

    while i < raw_lines.len() {
        let raw = raw_lines[i];

        if raw.starts_with("```") {
            if in_code_block {
                for cl in &code_block_lines {
                    lines.push(Line::from(Span::styled(
                        format!("  {}", cl),
                        Style::default().fg(WHITE).bg(CODE_BG),
                    )));
                }
                code_block_lines.clear();
                in_code_block = false;
            } else {
                if in_table {
                    render_table(&mut lines, &table_rows);
                    table_rows.clear();
                    in_table = false;
                }
                let lang = raw.trim_start_matches('`').trim();
                if !lang.is_empty() {
                    lines.push(Line::from(Span::styled(
                        format!("  ┌─ {} ─", lang),
                        Style::default().fg(DIM),
                    )));
                } else {
                    lines.push(Line::from(Span::styled("  ┌──────", Style::default().fg(DIM))));
                }
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
        for cl in &code_block_lines {
            lines.push(Line::from(Span::styled(
                format!("  {}", cl),
                Style::default().fg(WHITE).bg(CODE_BG),
            )));
        }
        lines.push(Line::from(Span::styled("  └──────", Style::default().fg(DIM))));
    }
    if in_table && !table_rows.is_empty() {
        render_table(&mut lines, &table_rows);
    }
    lines
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
                spans.push(Span::styled(
                    remaining[..end].to_string(),
                    Style::default().fg(YELLOW).bg(INLINE_CODE_BG),
                ));
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
