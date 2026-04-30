use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

pub(crate) fn keyhint_bar_line(hints: &[&str], accent: Color, muted: Color) -> Line<'static> {
    Line::from(vec![
        Span::styled("  [Keys] ", Style::default().fg(accent)),
        Span::styled(hints.join(" · "), Style::default().fg(muted)),
    ])
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

pub(crate) fn search_prompt_label(query: &str) -> String {
    if query.trim().is_empty() {
        "Search: (empty)".to_string()
    } else {
        format!("Search: {}", query)
    }
}

pub(crate) fn preview_empty_state(title: &str) -> String {
    format!("{} preview unavailable", title)
}

pub(crate) fn leading_panel_rect(area: Rect, max_width: u16, max_height: u16) -> Rect {
    let width = area.width.min(max_width.max(1));
    let height = area.height.min(max_height.max(1));
    Rect {
        x: area.x,
        y: area.y,
        width,
        height,
    }
}

pub(crate) fn leading_panel_rect_for_density(
    area: Rect,
    density: crate::ui::responsive::Density,
    wide_max_width: u16,
    max_height: u16,
) -> Rect {
    match density {
        crate::ui::responsive::Density::Narrow => Rect {
            x: area.x,
            y: area.y,
            width: area.width,
            height: area.height.min(max_height.max(1)),
        },
        crate::ui::responsive::Density::Medium | crate::ui::responsive::Density::Wide => {
            leading_panel_rect(area, wide_max_width, max_height)
        }
    }
}

#[cfg(test)]
mod tests {
    use ratatui::layout::Rect;
    use ratatui::style::Color;

    use super::{
        footer_hint_line, keyhint_bar_line, leading_panel_rect, leading_panel_rect_for_density,
        preview_empty_state, search_prompt_label, section_title_line,
    };
    use crate::ui::responsive::Density;

    #[test]
    fn leading_panel_rect_sticks_to_left_edge() {
        let rect = leading_panel_rect(Rect::new(5, 2, 100, 20), 40, 8);
        assert_eq!(rect.x, 5);
        assert_eq!(rect.y, 2);
        assert_eq!(rect.width, 40);
        assert_eq!(rect.height, 8);
    }

    #[test]
    fn section_helper_renders_text() {
        assert!(section_title_line("Section", Color::Yellow)
            .to_string()
            .contains("Section"));
        assert!(footer_hint_line(&["a", "b"], Color::Gray)
            .to_string()
            .contains("a · b"));
    }

    #[test]
    fn keyhint_and_search_helpers_render() {
        assert!(keyhint_bar_line(&["Esc close"], Color::Yellow, Color::Gray)
            .to_string()
            .contains("Esc close"));
        assert_eq!(search_prompt_label(""), "Search: (empty)");
        assert_eq!(search_prompt_label("abc"), "Search: abc");
        assert_eq!(
            preview_empty_state("Artifact"),
            "Artifact preview unavailable"
        );
    }

    #[test]
    fn leading_panel_rect_for_density_keeps_wide_layout_left_aligned() {
        let rect = leading_panel_rect_for_density(Rect::new(3, 1, 120, 10), Density::Wide, 80, 6);
        assert_eq!(rect.x, 3);
        assert_eq!(rect.width, 80);
    }
}
