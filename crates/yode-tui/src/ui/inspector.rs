use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::layout::Rect;
use ratatui::Frame;

use super::panels::{footer_hint_line, section_title_line};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectorTab {
    pub id: String,
    pub label: String,
    pub item_count: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectorState {
    pub title: String,
    pub selected_tab: usize,
    pub tabs: Vec<InspectorTab>,
    pub selected_line: usize,
    pub scroll_offset: usize,
}

pub(crate) fn inspector_experiment_enabled() -> bool {
    std::env::var("YODE_EXPERIMENT_INSPECTOR")
        .ok()
        .map(|value| matches!(value.as_str(), "1" | "true" | "yes"))
        .unwrap_or(false)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectorPanel {
    pub tab: InspectorTab,
    pub lines: Vec<String>,
    pub badges: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectorDocument {
    pub state: InspectorState,
    pub panels: Vec<InspectorPanel>,
    pub footer: Option<String>,
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

impl InspectorDocument {
    pub(crate) fn single(title: impl Into<String>, lines: Vec<String>) -> Self {
        let title = title.into();
        let tab = InspectorTab {
            id: "main".to_string(),
            label: "Main".to_string(),
            item_count: Some(lines.len()),
        };
        Self {
            state: InspectorState::new(title, vec![tab.clone()]),
            panels: vec![InspectorPanel {
                tab,
                lines,
                badges: Vec::new(),
            }],
            footer: None,
        }
    }

    pub(crate) fn active_panel(&self) -> Option<&InspectorPanel> {
        self.panels.get(self.state.selected_tab)
    }

    pub(crate) fn move_up(&mut self) {
        self.state.selected_line = self.state.selected_line.saturating_sub(1);
        self.sync_scroll();
    }

    pub(crate) fn move_down(&mut self) {
        let total = self.active_panel().map(|panel| panel.lines.len()).unwrap_or(0);
        if total > 0 {
            self.state.selected_line = (self.state.selected_line + 1).min(total - 1);
        }
        self.sync_scroll();
    }

    pub(crate) fn page_up(&mut self, page_size: usize) {
        self.state.selected_line = self.state.selected_line.saturating_sub(page_size);
        self.sync_scroll();
    }

    pub(crate) fn page_down(&mut self, page_size: usize) {
        let total = self.active_panel().map(|panel| panel.lines.len()).unwrap_or(0);
        if total > 0 {
            self.state.selected_line = (self.state.selected_line + page_size).min(total - 1);
        }
        self.sync_scroll();
    }

    pub(crate) fn cycle_tab(&mut self) {
        if self.panels.len() > 1 {
            self.state.selected_tab = (self.state.selected_tab + 1) % self.panels.len();
            self.state.selected_line = 0;
            self.state.scroll_offset = 0;
        }
    }

    fn sync_scroll(&mut self) {
        let viewport = 12usize;
        if self.state.selected_line < self.state.scroll_offset {
            self.state.scroll_offset = self.state.selected_line;
        } else if self.state.selected_line >= self.state.scroll_offset + viewport {
            self.state.scroll_offset = self.state.selected_line + 1 - viewport;
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

pub(crate) fn render_inspector(
    frame: &mut Frame,
    area: Rect,
    document: &InspectorDocument,
) {
    let Some(panel) = document.active_panel() else {
        return;
    };

    let mut lines = vec![Line::from(vec![Span::styled(
        format!("  {} ", document.state.title),
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
    )])];
    lines.push(multi_pane_title_strip(
        &document.state.tabs,
        document.state.selected_tab,
        Color::Yellow,
        Color::Gray,
    ));
    if !panel.badges.is_empty() {
        let badges = panel
            .badges
            .iter()
            .map(|(label, value)| (label.as_str(), value.as_str()))
            .collect::<Vec<_>>();
        lines.push(inspector_status_badge_row(&badges, Color::LightCyan));
    }
    lines.push(section_title_line(&panel.tab.label, Color::Yellow));

    let total = panel.lines.len();
    let start = document.state.scroll_offset.min(total);
    let end = (start + 12).min(total);
    if start == end {
        lines.push(Line::from("  (empty)"));
    } else {
        for (index, line) in panel.lines[start..end].iter().enumerate() {
            let absolute = start + index;
            let selected = absolute == document.state.selected_line.min(total.saturating_sub(1));
            lines.push(Line::from(vec![
                Span::styled(
                    if selected { "  ❯ " } else { "    " },
                    Style::default().fg(if selected { Color::LightCyan } else { Color::Gray }),
                ),
                Span::styled(
                    line.clone(),
                    Style::default().fg(if selected { Color::White } else { Color::Gray }),
                ),
            ]));
        }
    }

    let footer = document
        .footer
        .clone()
        .unwrap_or_else(|| inspector_pagination_footer(document.state.selected_line, total));
    lines.push(footer_hint_line(&[&footer], Color::DarkGray));
    frame.render_widget(Paragraph::new(lines), area);
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
        inspector_empty_state_actions, inspector_experiment_enabled,
        inspector_pagination_footer, inspector_status_badge_row,
        multi_pane_title_strip, InspectorBodySource, InspectorDocument,
        InspectorState, InspectorTab, PanelStackCoordinator,
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

    #[test]
    fn inspector_experiment_defaults_off() {
        assert!(!inspector_experiment_enabled());
    }

    #[test]
    fn inspector_document_navigation_moves_selection() {
        let mut doc = InspectorDocument::single(
            "demo",
            vec!["a".to_string(), "b".to_string(), "c".to_string()],
        );
        doc.move_down();
        assert_eq!(doc.state.selected_line, 1);
        doc.page_down(10);
        assert_eq!(doc.state.selected_line, 2);
        doc.page_up(10);
        assert_eq!(doc.state.selected_line, 0);
    }
}
