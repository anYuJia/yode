use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

use crate::app::App;

use super::chat::{ACCENT, DIM, GREEN, WHITE};
use super::chat_header::{header_gradient, should_show_logo, HEADER_LOGO};

/// Wrap lines at `width` using unicode display widths.
pub(super) fn manual_wrap(lines: Vec<Line<'static>>, width: u16) -> Vec<Line<'static>> {
    let w = width.max(1) as usize;
    let mut result = Vec::with_capacity(lines.len());

    for line in lines {
        let total_w: usize = line
            .spans
            .iter()
            .map(|s| visible_text_width(&s.content))
            .sum();

        if total_w <= w {
            result.push(line);
        } else {
            let mut current_spans: Vec<Span<'static>> = Vec::new();
            let mut current_w: usize = 0;
            let mut active_hyperlink: Option<String> = None;

            for span in line.spans {
                let span_w = visible_text_width(&span.content);
                if span_w == 0 {
                    if let Some(start) = osc8_start_sequence(&span.content) {
                        active_hyperlink = Some(start);
                    } else if is_osc8_close_sequence(&span.content) {
                        active_hyperlink = None;
                    }
                    current_spans.push(span);
                    continue;
                }
                if current_w + span_w <= w {
                    current_w += span_w;
                    current_spans.push(span);
                } else {
                    let style = span.style;
                    let fragments = split_terminal_text_fragments(&span.content);
                    let mut buffer = String::new();
                    let mut buffer_w = 0;

                    for fragment in fragments {
                        if fragment.visible_width == 0 {
                            buffer.push_str(&fragment.raw);
                            continue;
                        }

                        if current_w + buffer_w + fragment.visible_width > w
                            && (!buffer.is_empty() || !current_spans.is_empty())
                        {
                            if !buffer.is_empty() {
                                current_spans.push(Span::styled(buffer.clone(), style));
                                buffer.clear();
                                buffer_w = 0;
                            }
                            finalize_wrapped_line(
                                &mut result,
                                &mut current_spans,
                                &active_hyperlink,
                            );
                            current_w = 0;
                            if let Some(start) = active_hyperlink.as_ref() {
                                current_spans.push(Span::raw(start.clone()));
                            }
                        }

                        buffer.push_str(&fragment.raw);
                        buffer_w += fragment.visible_width;
                    }

                    if !buffer.is_empty() {
                        current_w += buffer_w;
                        current_spans.push(Span::styled(buffer, style));
                    }
                }
            }
            if !current_spans.is_empty() {
                result.push(Line::from(current_spans));
            }
        }
    }

    result
}

pub(super) fn visible_text_width(text: &str) -> usize {
    split_terminal_text_fragments(text)
        .iter()
        .map(|fragment| fragment.visible_width)
        .sum()
}

#[derive(Debug, Clone)]
struct TerminalTextFragment {
    raw: String,
    visible_width: usize,
}

fn finalize_wrapped_line(
    result: &mut Vec<Line<'static>>,
    current_spans: &mut Vec<Span<'static>>,
    active_hyperlink: &Option<String>,
) {
    if active_hyperlink.is_some() {
        current_spans.push(Span::raw(osc8_close_sequence()));
    }
    result.push(Line::from(std::mem::take(current_spans)));
}

fn split_terminal_text_fragments(text: &str) -> Vec<TerminalTextFragment> {
    let mut fragments = Vec::new();
    let bytes = text.as_bytes();
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index] == 0x1b {
            if let Some((sequence, next_index)) = consume_terminal_sequence(text, index) {
                fragments.push(TerminalTextFragment {
                    raw: sequence,
                    visible_width: 0,
                });
                index = next_index;
                continue;
            }
        }

        let rest = &text[index..];
        let mut chars = rest.chars();
        let Some(ch) = chars.next() else {
            break;
        };
        let ch_len = ch.len_utf8();
        fragments.push(TerminalTextFragment {
            raw: ch.to_string(),
            visible_width: unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0),
        });
        index += ch_len;
    }

    fragments
}

fn consume_terminal_sequence(text: &str, start: usize) -> Option<(String, usize)> {
    let bytes = text.as_bytes();
    if start >= bytes.len() || bytes[start] != 0x1b {
        return None;
    }

    let next = start + 1;
    if next >= bytes.len() {
        return None;
    }

    match bytes[next] {
        b'[' => {
            let mut index = next + 1;
            while index < bytes.len() {
                let byte = bytes[index];
                index += 1;
                if (0x40..=0x7e).contains(&byte) {
                    return Some((text[start..index].to_string(), index));
                }
            }
            Some((text[start..].to_string(), bytes.len()))
        }
        b']' => {
            let mut index = next + 1;
            while index < bytes.len() {
                if bytes[index] == 0x07 {
                    index += 1;
                    return Some((text[start..index].to_string(), index));
                }
                if bytes[index] == 0x1b && index + 1 < bytes.len() && bytes[index + 1] == b'\\' {
                    index += 2;
                    return Some((text[start..index].to_string(), index));
                }
                index += 1;
            }
            Some((text[start..].to_string(), bytes.len()))
        }
        _ => Some((text[start..next + 1].to_string(), next + 1)),
    }
}

fn osc8_start_sequence(text: &str) -> Option<String> {
    if text.starts_with("\x1b]8;;")
        && (text.ends_with('\x07') || text.ends_with("\x1b\\"))
        && text != osc8_close_sequence()
    {
        Some(text.to_string())
    } else {
        None
    }
}

fn is_osc8_close_sequence(text: &str) -> bool {
    text == osc8_close_sequence()
}

pub(super) fn osc8_close_sequence() -> &'static str {
    "\x1b]8;;\x07"
}

pub(crate) fn render_header(app: &App, width: usize) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let title_style = Style::default().fg(ACCENT).add_modifier(Modifier::BOLD);
    let ver_style = Style::default().fg(DIM);
    let model_style = Style::default().fg(WHITE).add_modifier(Modifier::BOLD);
    let path_style = Style::default().fg(GREEN);
    let dim = Style::default().fg(DIM);
    let hint_style = Style::default().fg(DIM);
    let session_short = if app.session.session_id.len() >= 8 {
        app.session.session_id[..8].to_string()
    } else {
        app.session.session_id.clone()
    };
    let model = app.session.model.clone();
    let workdir = app.session.working_dir.clone();

    let logo_w = 34usize;
    let gradient = header_gradient();

    let inner_w = width.saturating_sub(4);
    let show_logo = should_show_logo(width, logo_w);

    let make_row = |left_spans: Vec<Span<'static>>,
                    logo_idx: Option<usize>,
                    row_idx: usize|
     -> Line<'static> {
        let left_w: usize = left_spans
            .iter()
            .map(|s| UnicodeWidthStr::width(s.content.as_ref()))
            .sum();

        let border_color = Style::default().fg(gradient[row_idx]);
        let mut spans = vec![Span::styled("│ ", border_color)];
        spans.extend(left_spans);

        if show_logo {
            if let Some(idx) = logo_idx {
                let gap = inner_w.saturating_sub(left_w + logo_w);
                spans.push(Span::raw(" ".repeat(gap)));
                spans.push(Span::styled(
                    HEADER_LOGO[idx].to_string(),
                    Style::default()
                        .fg(gradient[row_idx])
                        .add_modifier(Modifier::BOLD),
                ));
            }
        }

        Line::from(spans)
    };

    let title_text = " Yode ";
    let ver_text = concat!("v", env!("CARGO_PKG_VERSION"), " ");
    let rule_len = width.saturating_sub(title_text.len() + ver_text.len() + 2);
    let top_color = Style::default().fg(gradient[0]);
    lines.push(Line::from(vec![
        Span::styled("╭", top_color),
        Span::styled(title_text, title_style),
        Span::styled(ver_text, ver_style),
        Span::styled("─".repeat(rule_len), top_color),
        Span::styled("╮", top_color),
    ]));

    lines.push(make_row(vec![], Some(0), 1));
    lines.push(make_row(
        vec![
            Span::styled(" ", Style::default()),
            Span::styled(model, model_style),
        ],
        Some(1),
        2,
    ));
    lines.push(make_row(
        vec![
            Span::styled(" ", Style::default()),
            Span::styled(workdir, path_style),
        ],
        Some(2),
        3,
    ));
    lines.push(make_row(
        vec![
            Span::styled(" ", Style::default()),
            Span::styled("agentic terminal · ", Style::default().fg(ACCENT)),
            Span::styled(format!("session {}", session_short), dim),
        ],
        Some(3),
        4,
    ));
    lines.push(make_row(vec![], Some(4), 5));
    lines.push(make_row(
        vec![
            Span::styled(" ", Style::default()),
            Span::styled("? ", Style::default().fg(ACCENT)),
            Span::styled("/help", hint_style),
            Span::styled(" · ", Style::default().fg(Color::DarkGray)),
            Span::styled("/keys", hint_style),
            Span::styled(" · ", Style::default().fg(Color::DarkGray)),
            Span::styled("Shift+Tab mode", hint_style),
            Span::styled(" · ", Style::default().fg(Color::DarkGray)),
            Span::styled("Ctrl+C×2 quit", hint_style),
        ],
        Some(5),
        6,
    ));

    let bot_color = Style::default().fg(gradient[7]);
    lines.push(Line::from(vec![
        Span::styled("╰", bot_color),
        Span::styled("─".repeat(width.saturating_sub(2)), bot_color),
        Span::styled("╯", bot_color),
    ]));

    lines
}
