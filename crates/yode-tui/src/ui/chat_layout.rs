use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

use crate::app::App;

use super::chat::{ACCENT, DIM, GREEN, WHITE};

/// Wrap lines at `width` using unicode display widths.
pub(super) fn manual_wrap(lines: Vec<Line<'static>>, width: u16) -> Vec<Line<'static>> {
    let w = width.max(1) as usize;
    let mut result = Vec::with_capacity(lines.len());

    for line in lines {
        let total_w: usize = line
            .spans
            .iter()
            .map(|s| UnicodeWidthStr::width(s.content.as_ref()))
            .sum();

        if total_w <= w {
            result.push(line);
        } else {
            let mut current_spans: Vec<Span<'static>> = Vec::new();
            let mut current_w: usize = 0;

            for span in line.spans {
                let span_w = UnicodeWidthStr::width(span.content.as_ref());
                if current_w + span_w <= w {
                    current_w += span_w;
                    current_spans.push(span);
                } else {
                    let mut buf = String::new();
                    let style = span.style;
                    for ch in span.content.chars() {
                        let ch_w = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
                        if current_w + ch_w > w && !buf.is_empty() {
                            current_spans.push(Span::styled(buf.clone(), style));
                            result.push(Line::from(current_spans));
                            current_spans = Vec::new();
                            current_w = 0;
                            buf.clear();
                        }
                        buf.push(ch);
                        current_w += ch_w;
                    }
                    if !buf.is_empty() {
                        current_spans.push(Span::styled(buf, style));
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

    let logo = [
        "██╗   ██╗ ██████╗ ██████╗ ███████╗",
        "╚██╗ ██╔╝██╔═══██╗██╔══██╗██╔════╝",
        " ╚████╔╝ ██║   ██║██║  ██║█████╗  ",
        "  ╚██╔╝  ██║   ██║██║  ██║██╔══╝  ",
        "   ██║   ╚██████╔╝██████╔╝███████╗",
        "   ╚═╝    ╚═════╝ ╚═════╝ ╚══════╝",
    ];
    let logo_w = 34usize;
    let gradient: [Color; 8] = [
        Color::Indexed(37),
        Color::Indexed(37),
        Color::Indexed(44),
        Color::Indexed(45),
        Color::Indexed(81),
        Color::Indexed(115),
        Color::Indexed(120),
        Color::Indexed(120),
    ];

    let inner_w = width.saturating_sub(4);
    let show_logo = inner_w > logo_w + 25;

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
                    logo[idx].to_string(),
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
