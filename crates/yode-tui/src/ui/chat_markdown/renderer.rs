use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

use super::types::{MarkdownBlock, InlineNode, ListItem, TableCell, MarkdownRenderOptions};
use super::utils::{
    block_spacing, keeps_following_block_tight, ensure_blank_line, normalize_blank_lines,
    is_blank_line, line_display_width, pad_line_to_width, prepend_prefix, add_modifier_to_line,
    render_code_block_header, render_code_block, number_to_letter, number_to_roman,
    compact_table_cell_text, min_cell_width, inline_nodes_to_plain_text, render_plain_text_lines,
    append_text_with_links, render_inline_code_spans, TABLE_MAX_ROW_LINES
};
use crate::ui::chat::{DIM, WHITE, YELLOW};
use crate::ui::chat_layout::manual_wrap;
use crate::ui::palette::INFO_COLOR;

pub fn render_block_sequence(
    lines: &mut Vec<Line<'static>>,
    blocks: &[MarkdownBlock],
    default_fg: Option<Color>,
    list_depth: usize,
    with_gap: bool,
    options: &MarkdownRenderOptions,
) {
    for (index, block) in blocks.iter().enumerate() {
        let spacing = block_spacing(block);
        let previous = index
            .checked_sub(1)
            .and_then(|previous| blocks.get(previous));
        if with_gap
            && index > 0
            && spacing.before
            && !previous.is_some_and(|previous| keeps_following_block_tight(previous, block))
        {
            ensure_blank_line(lines);
        }
        render_markdown_block(lines, block, default_fg, list_depth, options);
        if with_gap && spacing.after {
            ensure_blank_line(lines);
        }
    }
    normalize_blank_lines(lines);
}

pub fn render_markdown_block(
    lines: &mut Vec<Line<'static>>,
    block: &MarkdownBlock,
    default_fg: Option<Color>,
    list_depth: usize,
    options: &MarkdownRenderOptions,
) {
    match block {
        MarkdownBlock::Heading { level, content } => {
            let style = match level {
                1 => Style::default()
                    .fg(Color::LightYellow)
                    .add_modifier(Modifier::BOLD | Modifier::ITALIC | Modifier::UNDERLINED),
                2 => Style::default()
                    .fg(Color::Indexed(51))
                    .add_modifier(Modifier::BOLD),
                _ => Style::default()
                    .fg(Color::LightBlue)
                    .add_modifier(Modifier::BOLD),
            };
            let rendered = render_inline_nodes_as_lines_with_style(content, style, options);
            for line in rendered {
                lines.push(prepend_prefix(line, String::new(), style));
            }
        }
        MarkdownBlock::Rule => {
            lines.push(Line::from(Span::styled(
                "────────────────────────────────────────",
                Style::default().fg(DIM),
            )));
        }
        MarkdownBlock::Paragraph { content } => {
            lines.extend(render_inline_nodes_as_lines(content, default_fg, options));
        }
        MarkdownBlock::Quote { blocks } => {
            let mut inner = Vec::new();
            render_block_sequence(&mut inner, blocks, default_fg, list_depth, true, options);
            for line in inner {
                if is_blank_line(&line) {
                    lines.push(line);
                } else {
                    lines.push(prepend_prefix(
                        add_modifier_to_line(line, Modifier::ITALIC),
                        "▎ ".to_string(),
                        Style::default().fg(Color::DarkGray),
                    ));
                }
            }
        }
        MarkdownBlock::List {
            ordered_start,
            items,
        } => {
            render_list_block(
                lines,
                *ordered_start,
                items,
                default_fg,
                list_depth,
                options,
            );
        }
        MarkdownBlock::Table { rows } => {
            render_table(lines, rows, options);
        }
        MarkdownBlock::CodeFence {
            label,
            language,
            lines: code_lines,
        } => {
            lines.push(Line::from(render_code_block_header(
                label.as_deref(),
                *language,
            )));
            render_code_block(lines, code_lines, *language);
        }
    }
}

pub fn render_list_block(
    lines: &mut Vec<Line<'static>>,
    ordered_start: Option<u64>,
    items: &[ListItem],
    default_fg: Option<Color>,
    list_depth: usize,
    options: &MarkdownRenderOptions,
) {
    for (index, item) in items.iter().enumerate() {
        let mut rendered_item = Vec::new();
        render_block_sequence(
            &mut rendered_item,
            &item.blocks,
            default_fg,
            list_depth + 1,
            false,
            options,
        );

        let number = ordered_start.map(|start| start + index as u64);
        let (prefix, prefix_style) = list_item_prefix(list_depth, number, item.task_state);
        let rest_prefix = " ".repeat(UnicodeWidthStr::width(prefix.as_str()));

        let mut rendered_any = false;
        for line in rendered_item {
            if is_blank_line(&line) {
                lines.push(line);
                continue;
            }
            let prefixed = if rendered_any {
                prepend_prefix(line, rest_prefix.clone(), Style::default())
            } else {
                prepend_prefix(line, prefix.clone(), prefix_style)
            };
            lines.push(prefixed);
            rendered_any = true;
        }

        if !rendered_any {
            lines.push(Line::from(Span::styled(prefix, prefix_style)));
        }
    }
}

fn list_item_prefix(
    list_depth: usize,
    ordered_number: Option<u64>,
    task_state: Option<bool>,
) -> (String, Style) {
    let indent = "  ".repeat(list_depth);
    if let Some(checked) = task_state {
        return (
            format!("{}{} ", indent, if checked { "☑" } else { "☐" }),
            Style::default().fg(if checked { Color::LightGreen } else { DIM }),
        );
    }

    if let Some(number) = ordered_number {
        return (
            format!("{}{}. ", indent, format_list_number(list_depth, number)),
            Style::default().fg(DIM),
        );
    }

    let bullet = match list_depth {
        0 => "•",
        1 => "◦",
        _ => "▪",
    };
    (format!("{}{} ", indent, bullet), Style::default().fg(DIM))
}

fn format_list_number(list_depth: usize, number: u64) -> String {
    match list_depth {
        0 | 1 => number.to_string(),
        2 => number_to_letter(number),
        3 => number_to_roman(number),
        _ => number.to_string(),
    }
}

pub fn render_inline_nodes_as_lines(
    nodes: &[InlineNode],
    default_fg: Option<Color>,
    options: &MarkdownRenderOptions,
) -> Vec<Line<'static>> {
    let base_style = default_fg
        .map(|fg| Style::default().fg(fg))
        .unwrap_or_default();
    render_inline_nodes_as_lines_with_style(nodes, base_style, options)
}

pub fn render_inline_nodes_as_lines_with_style(
    nodes: &[InlineNode],
    base_style: Style,
    options: &MarkdownRenderOptions,
) -> Vec<Line<'static>> {
    let mut rendered: Vec<Vec<Span<'static>>> = vec![Vec::new()];
    append_inline_nodes(&mut rendered, nodes, base_style, options);

    let lines = rendered
        .into_iter()
        .map(|spans| {
            if spans.is_empty() {
                Line::from("")
            } else {
                Line::from(spans)
            }
        })
        .collect::<Vec<_>>();
    if let Some(max_width) = options.max_width {
        manual_wrap(lines, max_width as u16)
    } else {
        lines
    }
}

pub fn append_inline_nodes(
    lines: &mut Vec<Vec<Span<'static>>>,
    nodes: &[InlineNode],
    style: Style,
    options: &MarkdownRenderOptions,
) {
    for node in nodes {
        match node {
            InlineNode::Text(text) => {
                if !text.is_empty() {
                    if options.enable_hyperlinks {
                        append_text_with_links(lines.last_mut().unwrap(), text, style);
                    } else {
                        lines
                            .last_mut()
                            .unwrap()
                            .push(Span::styled(text.clone(), style));
                    }
                }
            }
            InlineNode::Strong(children) => {
                append_inline_nodes(lines, children, style.add_modifier(Modifier::BOLD), options)
            }
            InlineNode::Emphasis(children) => append_inline_nodes(
                lines,
                children,
                style.add_modifier(Modifier::ITALIC),
                options,
            ),
            InlineNode::Code(text) => {
                lines
                    .last_mut()
                    .unwrap()
                    .extend(render_inline_code_spans(text));
            }
            InlineNode::Link { text, url } => {
                if let Some(email) = url.strip_prefix("mailto:") {
                    lines
                        .last_mut()
                        .unwrap()
                        .push(Span::styled(email.to_string(), style));
                    continue;
                }
                let link_style = style.fg(INFO_COLOR).add_modifier(Modifier::UNDERLINED);
                if options.enable_hyperlinks {
                    lines
                        .last_mut()
                        .unwrap()
                        .push(Span::raw(super::utils::osc8_start_sequence(url)));
                }
                if text.is_empty() {
                    lines
                        .last_mut()
                        .unwrap()
                        .push(Span::styled(url.clone(), link_style));
                } else {
                    append_inline_nodes(lines, text, link_style, options);
                }
                if options.enable_hyperlinks {
                    lines
                        .last_mut()
                        .unwrap()
                        .push(Span::raw(crate::ui::chat_layout::osc8_close_sequence()));
                }
            }
            InlineNode::SoftBreak => {
                lines.push(Vec::new());
            }
            InlineNode::HardBreak => {
                lines.push(Vec::new());
            }
        }
    }
}

pub fn render_table(
    lines: &mut Vec<Line<'static>>,
    rows: &[Vec<TableCell>],
    options: &MarkdownRenderOptions,
) {
    if rows.is_empty() {
        return;
    }

    let col_count = rows.iter().map(|r| r.len()).max().unwrap_or(0);
    let mut widths = vec![0usize; col_count];
    let mut min_widths = vec![3usize; col_count];
    for row in rows {
        for (j, cell) in row.iter().enumerate() {
            if j < col_count {
                let rendered = render_inline_nodes_as_lines_with_style(
                    &cell.content,
                    Style::default().fg(WHITE),
                    options,
                );
                let max_width = rendered.iter().map(line_display_width).max().unwrap_or(0);
                widths[j] = widths[j].max(max_width);
                min_widths[j] = min_widths[j].max(min_cell_width(&cell.content));
            }
        }
    }

    let available_width = options.max_width.unwrap_or(80).max(12);
    let border_overhead = 2 + col_count * 2 + col_count.saturating_sub(1);
    let cell_budget = available_width.saturating_sub(border_overhead);
    let total_ideal: usize = widths.iter().sum();
    let total_min: usize = min_widths.iter().sum();

    if total_ideal > cell_budget {
        if total_min <= cell_budget {
            let extra = cell_budget - total_min;
            let overflow_total = total_ideal - total_min;
            for index in 0..widths.len() {
                let overflow = widths[index].saturating_sub(min_widths[index]);
                let share = overflow
                    .saturating_mul(extra)
                    .checked_div(overflow_total)
                    .unwrap_or(0);
                widths[index] = min_widths[index] + share;
            }
        } else if total_min > 0 {
            let scaled = cell_budget as f32 / total_min as f32;
            for index in 0..widths.len() {
                widths[index] = ((min_widths[index] as f32 * scaled).floor() as usize).max(3);
            }
        }
    }

    for width in &mut widths {
        *width = (*width).max(3);
    }

    if rows.len() > 1 && should_render_table_vertically(rows, &widths, options) {
        render_vertical_table(lines, rows, options.max_width.unwrap_or(80).max(12));
        return;
    }

    let mut rendered_table = Vec::new();

    rendered_table.push(render_table_border_line(&widths, '┌', '┬', '┐'));

    if let Some(header) = rows.first() {
        render_table_row(&mut rendered_table, header, &widths, true, options);
        rendered_table.push(render_table_border_line(&widths, '├', '┼', '┤'));
    }

    for (row_index, row) in rows.iter().skip(1).enumerate() {
        render_table_row(&mut rendered_table, row, &widths, false, options);
        if row_index + 2 < rows.len() {
            rendered_table.push(render_table_border_line(&widths, '├', '┼', '┤'));
        }
    }

    rendered_table.push(render_table_border_line(&widths, '└', '┴', '┘'));

    if rendered_table
        .iter()
        .any(|line| line_display_width(line) > available_width)
    {
        render_vertical_table(lines, rows, available_width);
        return;
    }

    lines.extend(rendered_table);
}

pub fn render_table_row(
    lines: &mut Vec<Line<'static>>,
    row: &[TableCell],
    widths: &[usize],
    is_header: bool,
    options: &MarkdownRenderOptions,
) {
    let base_style = if is_header {
        Style::default().fg(WHITE).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(WHITE)
    };
    let rendered_cells: Vec<Vec<Line<'static>>> = row
        .iter()
        .enumerate()
        .map(|(index, cell)| {
            let cell_lines =
                render_inline_nodes_as_lines_with_style(&cell.content, base_style, options);
            wrap_cell_lines(cell_lines, widths.get(index).copied().unwrap_or(10))
        })
        .collect();
    let row_height = rendered_cells
        .iter()
        .map(|cell| cell.len())
        .max()
        .unwrap_or(1);

    for line_index in 0..row_height {
        let mut spans = vec![Span::styled("│", Style::default().fg(DIM))];
        for (col_index, cell_lines) in rendered_cells.iter().enumerate() {
            let width = widths.get(col_index).copied().unwrap_or(10);
            spans.push(Span::styled(" ", base_style));

            if let Some(cell_line) = cell_lines.get(line_index) {
                let cell_width = line_display_width(cell_line);
                spans.extend(cell_line.spans.clone());
                pad_line_to_width(&mut spans, cell_width, width, base_style);
            } else {
                spans.push(Span::styled(" ".repeat(width), base_style));
            }

            spans.push(Span::styled(" ", base_style));
            spans.push(Span::styled("│", Style::default().fg(DIM)));
        }
        lines.push(Line::from(spans));
    }
}

pub fn render_table_border_line(
    widths: &[usize],
    left: char,
    middle: char,
    right: char,
) -> Line<'static> {
    let mut content = String::new();
    content.push(left);
    for (index, width) in widths.iter().enumerate() {
        content.push_str(&"─".repeat(*width + 2));
        content.push(if index + 1 < widths.len() {
            middle
        } else {
            right
        });
    }
    Line::from(Span::styled(content, Style::default().fg(DIM)))
}

fn wrap_cell_lines(cell_lines: Vec<Line<'static>>, width: usize) -> Vec<Line<'static>> {
    manual_wrap(cell_lines, width as u16)
}

fn should_render_table_vertically(
    rows: &[Vec<TableCell>],
    widths: &[usize],
    options: &MarkdownRenderOptions,
) -> bool {
    rows.iter()
        .flat_map(|row| row.iter().enumerate())
        .map(|(index, cell)| {
            let base_style = Style::default().fg(WHITE);
            let rendered =
                render_inline_nodes_as_lines_with_style(&cell.content, base_style, options);
            wrap_cell_lines(rendered, widths.get(index).copied().unwrap_or(10)).len()
        })
        .max()
        .unwrap_or(1)
        > TABLE_MAX_ROW_LINES
}

pub fn render_vertical_table(
    lines: &mut Vec<Line<'static>>,
    rows: &[Vec<TableCell>],
    max_width: usize,
) {
    if rows.is_empty() {
        return;
    }

    let headers = rows
        .first()
        .unwrap()
        .iter()
        .enumerate()
        .map(|(index, cell)| {
            let label = inline_nodes_to_plain_text(&cell.content).trim().to_string();
            if label.is_empty() {
                format!("Column {}", index + 1)
            } else {
                label
            }
        })
        .collect::<Vec<_>>();
    let separator_width = max_width.saturating_sub(1).clamp(3, 40);
    let separator = Line::from(Span::styled(
        "─".repeat(separator_width),
        Style::default().fg(DIM),
    ));
    let continuation_prefix = "  ";
    let continuation_width = max_width.saturating_sub(continuation_prefix.len()).max(10);

    for (row_index, row) in rows.iter().enumerate().skip(1) {
        if row_index > 1 {
            lines.push(separator.clone());
        }

        for (col_index, cell) in row.iter().enumerate() {
            let label = headers
                .get(col_index)
                .cloned()
                .unwrap_or_else(|| format!("Column {}", col_index + 1));
            let label_width = UnicodeWidthStr::width(label.as_str());
            let first_line_width = max_width.saturating_sub(label_width + 2).max(10);
            let value = compact_table_cell_text(&cell.content);
            let value_lines = wrap_cell_lines(
                render_plain_table_value(&value, first_line_width),
                first_line_width,
            );

            let mut first_spans = vec![Span::styled(
                format!("{}:", label),
                Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
            )];
            if let Some(first_value) = value_lines.first() {
                first_spans.push(Span::raw(" "));
                first_spans.extend(first_value.spans.clone());
            }
            lines.push(Line::from(first_spans));

            for continuation in value_lines.iter().skip(1) {
                let wrapped = wrap_cell_lines(vec![continuation.clone()], continuation_width);
                for line in wrapped {
                    lines.push(prepend_prefix(
                        line,
                        continuation_prefix.to_string(),
                        Style::default(),
                    ));
                }
            }
        }
    }
}

fn render_plain_table_value(value: &str, max_width: usize) -> Vec<Line<'static>> {
    render_plain_text_lines(
        value,
        Some(WHITE),
        MarkdownRenderOptions {
            max_width: Some(max_width),
            enable_hyperlinks: false,
        },
    )
}
