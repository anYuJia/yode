use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PanelPagerState {
    pub selected: usize,
    pub offset: usize,
    pub viewport: usize,
}

impl PanelPagerState {
    #[allow(dead_code)]
    pub(crate) fn new(viewport: usize) -> Self {
        Self {
            selected: 0,
            offset: 0,
            viewport: viewport.max(1),
        }
    }

    #[allow(dead_code)]
    pub(crate) fn clamp(mut self, total: usize) -> Self {
        if total == 0 {
            self.selected = 0;
            self.offset = 0;
            return self;
        }
        self.selected = self.selected.min(total.saturating_sub(1));
        if self.selected < self.offset {
            self.offset = self.selected;
        }
        let max_offset = total.saturating_sub(self.viewport);
        if self.selected >= self.offset + self.viewport {
            self.offset = self.selected + 1 - self.viewport;
        }
        self.offset = self.offset.min(max_offset);
        self
    }

    #[allow(dead_code)]
    pub(crate) fn visible_range(&self, total: usize) -> std::ops::Range<usize> {
        let clamped = self.clamp(total);
        let end = (clamped.offset + clamped.viewport).min(total);
        clamped.offset..end
    }
}

pub(crate) fn inspector_header_lines(
    title: &str,
    subtitle: Option<&str>,
    accent: Color,
    light: Color,
    _muted: Color,
) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from(vec![Span::styled(
        format!("  {} ", title),
        Style::default().fg(accent).add_modifier(Modifier::BOLD),
    )])];
    if let Some(subtitle) = subtitle {
        lines.push(Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled(subtitle.to_string(), Style::default().fg(light)),
        ]));
    }
    lines
}

pub(crate) fn section_title_line(title: &str, accent: Color) -> Line<'static> {
    Line::from(vec![Span::styled(
        format!("  ── {} ──", title),
        Style::default().fg(accent).add_modifier(Modifier::BOLD),
    )])
}

pub(crate) fn footer_hint_line(hints: &[&str], muted: Color) -> Line<'static> {
    Line::from(vec![Span::styled(
        format!("  {}", hints.join(" · ")),
        Style::default().fg(muted),
    )])
}

#[allow(dead_code)]
pub(crate) fn preview_selection_label(selected: usize, total: usize) -> String {
    if total == 0 {
        "0/0".to_string()
    } else {
        format!("{}/{}", selected.saturating_add(1).min(total), total)
    }
}

pub(crate) fn centered_panel_rect(area: Rect, max_width: u16, max_height: u16) -> Rect {
    let width = area.width.min(max_width.max(1));
    let height = area.height.min(max_height.max(1));
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 2;
    Rect { x, y, width, height }
}

pub(crate) fn button_row_line(
    labels: &[String],
    selected: usize,
    active: Color,
    muted: Color,
) -> Line<'static> {
    let mut spans = vec![Span::styled("  ", Style::default())];
    for (index, label) in labels.iter().enumerate() {
        if index > 0 {
            spans.push(Span::raw(" "));
        }
        let style = if index == selected {
            Style::default().fg(Color::Black).bg(active).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(muted)
        };
        spans.push(Span::styled(format!("[{}. {}]", index + 1, label), style));
    }
    Line::from(spans)
}

#[allow(dead_code)]
pub(crate) fn preview_panel_lines(
    title: &str,
    lines: &[String],
    pager: PanelPagerState,
    accent: Color,
    light: Color,
    muted: Color,
) -> Vec<Line<'static>> {
    let range = pager.visible_range(lines.len());
    let mut rendered = inspector_header_lines(
        title,
        Some(&format!(
            "Preview {}",
            preview_selection_label(pager.selected, lines.len())
        )),
        accent,
        light,
        muted,
    );
    rendered.push(section_title_line("Preview", accent));
    if lines.is_empty() {
        rendered.push(Line::from(Span::styled(
            "  (empty)",
            Style::default().fg(muted),
        )));
    } else {
        for (relative, line) in lines[range.clone()].iter().enumerate() {
            let absolute = range.start + relative;
            let style = if absolute == pager.clamp(lines.len()).selected {
                Style::default().fg(light).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(muted)
            };
            let prefix = if absolute == pager.clamp(lines.len()).selected {
                "  ❯ "
            } else {
                "    "
            };
            rendered.push(Line::from(vec![
                Span::styled(prefix, style),
                Span::styled(line.clone(), style),
            ]));
        }
    }
    rendered
}

#[allow(dead_code)]
pub(crate) fn timeline_panel_lines(
    title: &str,
    lines: &[String],
    pager: PanelPagerState,
    accent: Color,
    light: Color,
    muted: Color,
) -> Vec<Line<'static>> {
    let mut rendered = preview_panel_lines(title, lines, pager, accent, light, muted);
    rendered.push(footer_hint_line(&["↑↓ move", "PgUp/PgDn page", "Esc close"], muted));
    rendered
}

#[cfg(test)]
mod tests {
    use ratatui::layout::Rect;
    use ratatui::style::Color;

    use super::{
        button_row_line, centered_panel_rect, footer_hint_line, preview_panel_lines,
        preview_selection_label, section_title_line, timeline_panel_lines, PanelPagerState,
    };

    #[test]
    fn pager_state_clamps_selection_into_visible_range() {
        let pager = PanelPagerState {
            selected: 7,
            offset: 0,
            viewport: 3,
        }
        .clamp(10);
        assert_eq!(pager.offset, 5);
        assert_eq!(pager.visible_range(10), 5..8);
    }

    #[test]
    fn centered_panel_rect_respects_narrow_widths() {
        let rect = centered_panel_rect(Rect::new(0, 0, 20, 4), 80, 10);
        assert_eq!(rect.width, 20);
        assert_eq!(rect.height, 4);
    }

    #[test]
    fn preview_selection_label_formats_counts() {
        assert_eq!(preview_selection_label(0, 0), "0/0");
        assert_eq!(preview_selection_label(1, 5), "2/5");
    }

    #[test]
    fn button_row_marks_selected_option() {
        let line = button_row_line(
            &["Yes".to_string(), "No".to_string()],
            1,
            Color::Yellow,
            Color::Gray,
        );
        assert!(line.spans.iter().any(|span| span.content.contains("[2. No]")));
    }

    #[test]
    fn preview_and_timeline_panels_render_headers_and_footer() {
        let lines = vec!["first".to_string(), "second".to_string(), "third".to_string()];
        let preview = preview_panel_lines(
            "Artifacts",
            &lines,
            PanelPagerState::new(2),
            Color::Yellow,
            Color::White,
            Color::Gray,
        );
        assert!(preview.iter().any(|line| line.to_string().contains("Artifacts")));
        assert!(preview.iter().any(|line| line.to_string().contains("Preview")));

        let timeline = timeline_panel_lines(
            "Timeline",
            &lines,
            PanelPagerState::new(2),
            Color::Yellow,
            Color::White,
            Color::Gray,
        );
        assert!(timeline.iter().any(|line| line.to_string().contains("Esc close")));
    }

    #[test]
    fn section_and_footer_helpers_render_text() {
        assert!(section_title_line("Section", Color::Yellow)
            .to_string()
            .contains("Section"));
        assert!(footer_hint_line(&["a", "b"], Color::Gray)
            .to_string()
            .contains("a · b"));
    }
}
