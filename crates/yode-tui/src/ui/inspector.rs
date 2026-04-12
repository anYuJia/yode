use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InspectorTab {
    pub id: String,
    pub label: String,
    pub item_count: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InspectorState {
    pub title: String,
    pub selected_tab: usize,
    pub tabs: Vec<InspectorTab>,
    pub selected_line: usize,
    pub scroll_offset: usize,
}

impl InspectorState {
    pub(crate) fn new(title: impl Into<String>, tabs: Vec<InspectorTab>) -> Self {
        Self {
            title: title.into(),
            selected_tab: 0,
            tabs,
            selected_line: 0,
            scroll_offset: 0,
        }
    }
}

pub(crate) trait InspectorBodySource {
    fn line_count(&self) -> usize;
    fn line_at(&self, index: usize) -> Option<String>;
}

impl InspectorBodySource for Vec<String> {
    fn line_count(&self) -> usize {
        self.len()
    }

    fn line_at(&self, index: usize) -> Option<String> {
        self.get(index).cloned()
    }
}

pub(crate) fn multi_pane_title_strip(
    tabs: &[InspectorTab],
    selected: usize,
    accent: Color,
    muted: Color,
) -> Line<'static> {
    let mut spans = vec![Span::styled("  ", Style::default())];
    for (index, tab) in tabs.iter().enumerate() {
        if index > 0 {
            spans.push(Span::raw(" "));
        }
        let label = tab
            .item_count
            .map(|count| format!("{} ({})", tab.label, count))
            .unwrap_or_else(|| tab.label.clone());
        let style = if index == selected {
            Style::default().fg(accent).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(muted)
        };
        spans.push(Span::styled(format!("[{}]", label), style));
    }
    Line::from(spans)
}

pub(crate) fn inspector_status_badge_row(badges: &[(&str, &str)], accent: Color) -> Line<'static> {
    let mut spans = vec![Span::styled("  ", Style::default())];
    for (index, (label, value)) in badges.iter().enumerate() {
        if index > 0 {
            spans.push(Span::raw(" "));
        }
        spans.push(Span::styled(
            format!("{}={}", label, value),
            Style::default().fg(accent),
        ));
    }
    Line::from(spans)
}

pub(crate) fn inspector_empty_state_actions(actions: &[&str]) -> Vec<String> {
    if actions.is_empty() {
        return vec!["no actions available".to_string()];
    }
    actions.iter().map(|action| format!("try {}", action)).collect()
}

pub(crate) fn inspector_pagination_footer(selected: usize, total: usize) -> String {
    if total == 0 {
        "0/0 · PgUp/PgDn page · Esc close".to_string()
    } else {
        format!(
            "{}/{} · PgUp/PgDn page · Esc close",
            selected.min(total.saturating_sub(1)) + 1,
            total
        )
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct PanelStackCoordinator {
    pub layers: Vec<String>,
}

impl PanelStackCoordinator {
    pub(crate) fn push(&mut self, id: impl Into<String>) {
        self.layers.push(id.into());
    }

    pub(crate) fn pop(&mut self) -> Option<String> {
        self.layers.pop()
    }

    pub(crate) fn active(&self) -> Option<&str> {
        self.layers.last().map(String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use ratatui::style::Color;

    use super::{
        inspector_empty_state_actions, inspector_pagination_footer, inspector_status_badge_row,
        multi_pane_title_strip, InspectorBodySource, InspectorState, InspectorTab,
        PanelStackCoordinator,
    };

    #[test]
    fn title_strip_and_badges_render_selected_tabs() {
        let line = multi_pane_title_strip(
            &[
                InspectorTab {
                    id: "a".to_string(),
                    label: "Timeline".to_string(),
                    item_count: Some(2),
                },
                InspectorTab {
                    id: "b".to_string(),
                    label: "Artifacts".to_string(),
                    item_count: None,
                },
            ],
            0,
            Color::Yellow,
            Color::Gray,
        );
        assert!(line.to_string().contains("Timeline (2)"));
        let badges = inspector_status_badge_row(&[("status", "running")], Color::Yellow);
        assert!(badges.to_string().contains("status=running"));
    }

    #[test]
    fn empty_actions_and_pagination_render_fallbacks() {
        assert_eq!(
            inspector_empty_state_actions(&[]),
            vec!["no actions available".to_string()]
        );
        assert!(inspector_pagination_footer(0, 0).contains("0/0"));
        assert!(inspector_pagination_footer(1, 5).contains("2/5"));
    }

    #[test]
    fn panel_stack_tracks_active_layer() {
        let mut stack = PanelStackCoordinator::default();
        stack.push("task");
        stack.push("transcript");
        assert_eq!(stack.active(), Some("transcript"));
        assert_eq!(stack.pop().as_deref(), Some("transcript"));
        assert_eq!(stack.active(), Some("task"));
    }

    #[test]
    fn vec_line_source_implements_body_source() {
        let source = vec!["a".to_string(), "b".to_string()];
        assert_eq!(source.line_count(), 2);
        assert_eq!(source.line_at(1).as_deref(), Some("b"));
    }

    #[test]
    fn inspector_state_initializes_with_tabs() {
        let state = InspectorState::new(
            "demo",
            vec![InspectorTab {
                id: "a".to_string(),
                label: "One".to_string(),
                item_count: Some(1),
            }],
        );
        assert_eq!(state.title, "demo");
        assert_eq!(state.selected_tab, 0);
    }
}
