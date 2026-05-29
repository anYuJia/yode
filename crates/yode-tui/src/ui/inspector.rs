use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use super::palette::{BORDER_MUTED, LIGHT, MUTED, PANEL_ACCENT, SELECT_ACCENT, SELECT_BG};
use super::panels::{footer_hint_line, section_title_line};
use crate::inspector_targets::read_only_inspector_target_from_command;

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
    pub selected_action: usize,
    pub scroll_offset: usize,
    pub focus: InspectorFocus,
    pub search_active: bool,
    pub search_query: String,
    pub last_action_label: Option<String>,
    pub last_action_at: Option<String>,
    pub last_action_result: Option<InspectorActionResult>,
    pub last_action_error: Option<String>,
    pub last_action_detail: Option<String>,
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
    pub actions: Vec<InspectorAction>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectorDocument {
    pub state: InspectorState,
    pub panels: Vec<InspectorPanel>,
    pub footer: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectorAction {
    pub label: String,
    pub command: String,
    pub typed: Option<InspectorTypedAction>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectorTypedAction {
    pub kind: InspectorActionKind,
    pub target: InspectorActionTarget,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InspectorActionKind {
    LoadCommand,
    RunCommand,
    InternalConfirmAllow,
    InternalConfirmAlways,
    InternalConfirmDeny,
    OpenArtifact,
    OpenInspectorTarget,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InspectorActionTarget {
    Command(String),
    Artifact(String),
    InspectorTarget(String),
    Internal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InspectorActionEffect {
    LoadCommand(String),
    RunCommand(String),
    InternalConfirmAllow,
    InternalConfirmAlways,
    InternalConfirmDeny,
    OpenArtifact { target: String, command: String },
    OpenInspectorTarget { target: String, command: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InspectorActionResult {
    Success,
    Failure,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InspectorFocus {
    Tabs,
    Body,
    Actions,
}

impl InspectorState {
    pub(crate) fn new(title: impl Into<String>, tabs: Vec<InspectorTab>) -> Self {
        Self {
            title: title.into(),
            selected_tab: 0,
            tabs,
            selected_line: 0,
            selected_action: 0,
            scroll_offset: 0,
            focus: InspectorFocus::Body,
            search_active: false,
            search_query: String::new(),
            last_action_label: None,
            last_action_at: None,
            last_action_result: None,
            last_action_error: None,
            last_action_detail: None,
        }
    }
}

impl InspectorAction {
    pub(crate) fn command(label: impl Into<String>, command: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            command: command.into(),
            typed: None,
        }
    }

    pub(crate) fn from_command(label: impl Into<String>, command: impl Into<String>) -> Self {
        let label = label.into();
        let command = command.into();
        let label = if label.trim() == command.trim() {
            inspector_action_label_for_command(&command)
        } else {
            label
        };
        if let Some(target) = inspect_artifact_target(&command) {
            return Self::open_artifact(label, command, target);
        }
        if let Some(target) = inspect_inspector_target(&command) {
            return Self::open_inspector_target(label, command, target);
        }
        Self::command(label, command)
    }

    #[allow(dead_code)]
    pub(crate) fn load_command(label: impl Into<String>, command: impl Into<String>) -> Self {
        let command = command.into();
        Self {
            label: label.into(),
            command: command.clone(),
            typed: Some(InspectorTypedAction {
                kind: InspectorActionKind::LoadCommand,
                target: InspectorActionTarget::Command(command),
            }),
        }
    }

    #[allow(dead_code)]
    pub(crate) fn run_command(label: impl Into<String>, command: impl Into<String>) -> Self {
        let command = command.into();
        Self {
            label: label.into(),
            command: command.clone(),
            typed: Some(InspectorTypedAction {
                kind: InspectorActionKind::RunCommand,
                target: InspectorActionTarget::Command(command),
            }),
        }
    }

    pub(crate) fn internal_confirm_allow() -> Self {
        Self {
            label: "allow once".to_string(),
            command: "__yode_confirm_allow__".to_string(),
            typed: Some(InspectorTypedAction {
                kind: InspectorActionKind::InternalConfirmAllow,
                target: InspectorActionTarget::Internal,
            }),
        }
    }

    pub(crate) fn internal_confirm_always() -> Self {
        Self {
            label: "always allow".to_string(),
            command: "__yode_confirm_always__".to_string(),
            typed: Some(InspectorTypedAction {
                kind: InspectorActionKind::InternalConfirmAlways,
                target: InspectorActionTarget::Internal,
            }),
        }
    }

    pub(crate) fn internal_confirm_deny() -> Self {
        Self {
            label: "deny".to_string(),
            command: "__yode_confirm_deny__".to_string(),
            typed: Some(InspectorTypedAction {
                kind: InspectorActionKind::InternalConfirmDeny,
                target: InspectorActionTarget::Internal,
            }),
        }
    }

    pub(crate) fn open_artifact(
        label: impl Into<String>,
        command: impl Into<String>,
        artifact: impl Into<String>,
    ) -> Self {
        Self {
            label: label.into(),
            command: command.into(),
            typed: Some(InspectorTypedAction {
                kind: InspectorActionKind::OpenArtifact,
                target: InspectorActionTarget::Artifact(artifact.into()),
            }),
        }
    }

    pub(crate) fn open_inspector_target(
        label: impl Into<String>,
        command: impl Into<String>,
        target: impl Into<String>,
    ) -> Self {
        Self {
            label: label.into(),
            command: command.into(),
            typed: Some(InspectorTypedAction {
                kind: InspectorActionKind::OpenInspectorTarget,
                target: InspectorActionTarget::InspectorTarget(target.into()),
            }),
        }
    }

    pub(crate) fn effect(&self, execute_now: bool) -> InspectorActionEffect {
        let Some(typed) = &self.typed else {
            return if execute_now {
                InspectorActionEffect::RunCommand(self.command.clone())
            } else {
                InspectorActionEffect::LoadCommand(self.command.clone())
            };
        };
        match typed.kind {
            InspectorActionKind::LoadCommand => {
                if execute_now {
                    InspectorActionEffect::RunCommand(self.command.clone())
                } else {
                    InspectorActionEffect::LoadCommand(self.command.clone())
                }
            }
            InspectorActionKind::RunCommand => {
                InspectorActionEffect::RunCommand(self.command.clone())
            }
            InspectorActionKind::InternalConfirmAllow => {
                InspectorActionEffect::InternalConfirmAllow
            }
            InspectorActionKind::InternalConfirmAlways => {
                InspectorActionEffect::InternalConfirmAlways
            }
            InspectorActionKind::InternalConfirmDeny => InspectorActionEffect::InternalConfirmDeny,
            InspectorActionKind::OpenArtifact => match &typed.target {
                InspectorActionTarget::Artifact(_) if execute_now => {
                    InspectorActionEffect::OpenArtifact {
                        target: typed.target.name().unwrap_or_default().to_string(),
                        command: self.command.clone(),
                    }
                }
                _ => InspectorActionEffect::LoadCommand(self.command.clone()),
            },
            InspectorActionKind::OpenInspectorTarget => match &typed.target {
                InspectorActionTarget::InspectorTarget(_) if execute_now => {
                    InspectorActionEffect::OpenInspectorTarget {
                        target: typed.target.name().unwrap_or_default().to_string(),
                        command: self.command.clone(),
                    }
                }
                _ => InspectorActionEffect::LoadCommand(self.command.clone()),
            },
        }
    }
}

impl InspectorActionTarget {
    fn name(&self) -> Option<&str> {
        match self {
            InspectorActionTarget::Artifact(target)
            | InspectorActionTarget::InspectorTarget(target)
            | InspectorActionTarget::Command(target) => Some(target),
            InspectorActionTarget::Internal => None,
        }
    }
}

impl InspectorDocument {
    #[cfg(test)]
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
                actions: Vec::new(),
            }],
            footer: None,
        }
    }

    pub(crate) fn active_panel(&self) -> Option<&InspectorPanel> {
        self.panels.get(self.state.selected_tab)
    }

    fn filtered_indices(&self) -> Vec<usize> {
        let Some(panel) = self.active_panel() else {
            return Vec::new();
        };
        if self.state.search_query.trim().is_empty() {
            return (0..panel.lines.line_count()).collect();
        }
        let needle = self.state.search_query.to_lowercase();
        (0..panel.lines.line_count())
            .filter(|index| {
                panel
                    .lines
                    .line_at(*index)
                    .map(|line| line.to_lowercase().contains(&needle))
                    .unwrap_or(false)
            })
            .collect()
    }

    pub(crate) fn move_up(&mut self) {
        let indices = self.filtered_indices();
        if indices.is_empty() {
            self.state.selected_line = 0;
            self.sync_scroll();
            return;
        }
        let current = indices
            .iter()
            .position(|index| *index == self.state.selected_line)
            .unwrap_or(0);
        self.state.selected_line = indices[current.saturating_sub(1)];
        self.sync_scroll();
    }

    pub(crate) fn move_down(&mut self) {
        let indices = self.filtered_indices();
        if indices.is_empty() {
            self.state.selected_line = 0;
            self.sync_scroll();
            return;
        }
        let current = indices
            .iter()
            .position(|index| *index == self.state.selected_line)
            .unwrap_or(0);
        self.state.selected_line = indices[(current + 1).min(indices.len() - 1)];
        self.sync_scroll();
    }

    pub(crate) fn page_up(&mut self, page_size: usize) {
        let indices = self.filtered_indices();
        if indices.is_empty() {
            self.state.selected_line = 0;
            return;
        }
        let current = indices
            .iter()
            .position(|index| *index == self.state.selected_line)
            .unwrap_or(0);
        let next = current.saturating_sub(page_size);
        self.state.selected_line = indices[next];
        self.sync_scroll();
    }

    pub(crate) fn page_down(&mut self, page_size: usize) {
        let indices = self.filtered_indices();
        if indices.is_empty() {
            self.state.selected_line = 0;
            return;
        }
        let current = indices
            .iter()
            .position(|index| *index == self.state.selected_line)
            .unwrap_or(0);
        self.state.selected_line = indices[(current + page_size).min(indices.len() - 1)];
        self.sync_scroll();
    }

    pub(crate) fn cycle_tab(&mut self) {
        if self.panels.len() > 1 {
            self.state.selected_tab = (self.state.selected_tab + 1) % self.panels.len();
            self.state.selected_line = 0;
            self.state.scroll_offset = 0;
        }
    }

    pub(crate) fn toggle_focus(&mut self) {
        self.state.focus = match self.state.focus {
            InspectorFocus::Tabs => InspectorFocus::Body,
            InspectorFocus::Body => {
                if self
                    .active_panel()
                    .is_some_and(|panel| !panel.actions.is_empty())
                {
                    InspectorFocus::Actions
                } else {
                    InspectorFocus::Tabs
                }
            }
            InspectorFocus::Actions => InspectorFocus::Tabs,
        };
    }

    pub(crate) fn jump_to_line(&mut self, line_number: usize) {
        let total = self
            .active_panel()
            .map(|panel| panel.lines.line_count())
            .unwrap_or(0);
        if total == 0 {
            self.state.selected_line = 0;
        } else {
            self.state.selected_line = line_number.saturating_sub(1).min(total - 1);
        }
        self.sync_scroll();
    }

    pub(crate) fn begin_search(&mut self) {
        self.state.search_active = true;
        self.state.search_query.clear();
    }

    pub(crate) fn append_search_char(&mut self, c: char) {
        self.state.search_query.push(c);
        if let Some(first) = self.filtered_indices().into_iter().next() {
            self.state.selected_line = first;
        }
        self.sync_scroll();
    }

    pub(crate) fn pop_search_char(&mut self) {
        self.state.search_query.pop();
        if let Some(first) = self.filtered_indices().into_iter().next() {
            self.state.selected_line = first;
        }
        self.sync_scroll();
    }

    pub(crate) fn finish_search(&mut self, keep_query: bool) {
        self.state.search_active = false;
        if !keep_query {
            self.state.search_query.clear();
        }
    }

    pub(crate) fn tab_cycle_summary(&self) -> String {
        format!(
            "panel {}/{}",
            self.state.selected_tab + 1,
            self.state.tabs.len().max(1)
        )
    }

    pub(crate) fn handoff_command(&self) -> Option<String> {
        let panel = self.active_panel()?;
        if matches!(self.state.focus, InspectorFocus::Actions) && !panel.actions.is_empty() {
            return panel
                .actions
                .get(
                    self.state
                        .selected_action
                        .min(panel.actions.len().saturating_sub(1)),
                )
                .map(|action| action.command.clone());
        }
        let line = panel.lines.get(self.state.selected_line)?;
        extract_command_target(line)
            .or_else(|| panel.actions.first().map(|action| action.command.clone()))
            .or_else(|| self.footer.as_deref().and_then(extract_command_target))
    }

    pub(crate) fn handoff_action(&self) -> Option<InspectorAction> {
        let panel = self.active_panel()?;
        if matches!(self.state.focus, InspectorFocus::Actions) && !panel.actions.is_empty() {
            return panel
                .actions
                .get(
                    self.state
                        .selected_action
                        .min(panel.actions.len().saturating_sub(1)),
                )
                .cloned();
        }
        let line = panel.lines.get(self.state.selected_line)?;
        if let Some(command) = extract_command_target(line) {
            Some(InspectorAction::from_command(
                inspector_action_label_for_command(&command),
                command,
            ))
        } else {
            panel.actions.first().cloned()
        }
    }

    pub(crate) fn cycle_action_next(&mut self) {
        let Some(panel) = self.active_panel() else {
            return;
        };
        if panel.actions.is_empty() {
            self.state.selected_action = 0;
        } else {
            self.state.selected_action = (self.state.selected_action + 1) % panel.actions.len();
        }
    }

    pub(crate) fn cycle_action_prev(&mut self) {
        let Some(panel) = self.active_panel() else {
            return;
        };
        if panel.actions.is_empty() {
            self.state.selected_action = 0;
        } else {
            self.state.selected_action = if self.state.selected_action == 0 {
                panel.actions.len() - 1
            } else {
                self.state.selected_action - 1
            };
        }
    }

    pub(crate) fn note_action_dispatched(&mut self, label: impl Into<String>) {
        self.state.last_action_label = Some(label.into());
        self.state.last_action_at =
            Some(chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string());
        self.state.last_action_result = None;
        self.state.last_action_error = None;
        self.state.last_action_detail = None;
    }

    pub(crate) fn note_action_succeeded(&mut self, label: impl Into<String>) {
        self.state.last_action_label = Some(label.into());
        self.state.last_action_at =
            Some(chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string());
        self.state.last_action_result = Some(InspectorActionResult::Success);
        self.state.last_action_error = None;
        self.state.last_action_detail = None;
    }

    pub(crate) fn note_action_succeeded_with_detail(
        &mut self,
        label: impl Into<String>,
        detail: impl Into<String>,
    ) {
        self.state.last_action_label = Some(label.into());
        self.state.last_action_at =
            Some(chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string());
        self.state.last_action_result = Some(InspectorActionResult::Success);
        self.state.last_action_error = None;
        self.state.last_action_detail = Some(detail.into());
    }

    pub(crate) fn note_action_failed(
        &mut self,
        label: impl Into<String>,
        reason: impl Into<String>,
    ) {
        self.state.last_action_label = Some(label.into());
        self.state.last_action_at =
            Some(chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string());
        self.state.last_action_result = Some(InspectorActionResult::Failure);
        self.state.last_action_error = Some(reason.into());
        self.state.last_action_detail = None;
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
    _accent: Color,
    muted: Color,
) -> Line<'static> {
    let mut spans = vec![Span::styled("  ", Style::default())];
    for (index, tab) in tabs.iter().enumerate() {
        if index > 0 {
            spans.push(Span::raw(" "));
        }
        let label = tab
            .item_count
            .map(|count| format!("{} ·{}", tab.label, compact_tab_count(count)))
            .unwrap_or_else(|| tab.label.clone());
        let style = if index == selected {
            Style::default()
                .fg(LIGHT)
                .bg(SELECT_BG)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(muted)
        };
        spans.push(Span::styled(format!("[{}]", label), style));
    }
    Line::from(spans)
}

pub(crate) fn inspector_status_badge_row(badges: &[(&str, &str)], accent: Color) -> Line<'static> {
    let mut spans = vec![Span::styled("  ", Style::default())];
    let mut ordered = badges.to_vec();
    ordered.sort_by_key(|(label, _)| badge_priority(label));
    for (index, (label, value)) in ordered.iter().enumerate() {
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

pub(crate) fn inspector_action_row(
    actions: &[InspectorAction],
    selected: usize,
    _accent: Color,
    focused: bool,
) -> Line<'static> {
    let mut spans = vec![Span::styled(
        "  actions: ",
        Style::default().fg(BORDER_MUTED),
    )];
    for (index, action) in actions.iter().enumerate() {
        if index > 0 {
            spans.push(Span::raw(" "));
        }
        spans.push(Span::styled(
            format!("[{}]", action.label),
            if index == selected {
                Style::default()
                    .fg(LIGHT)
                    .bg(SELECT_BG)
                    .add_modifier(if focused {
                        Modifier::BOLD | Modifier::UNDERLINED
                    } else {
                        Modifier::BOLD
                    })
            } else {
                Style::default().fg(MUTED)
            },
        ));
    }
    Line::from(spans)
}

pub(crate) fn inspector_action_safety_summary(actions: &[InspectorAction]) -> Option<String> {
    if actions.is_empty() {
        return None;
    }
    let has_write = actions
        .iter()
        .any(|action| action.command.contains("run-write") || action.command.contains(" restore "));
    Some(if has_write {
        "safety: Enter loads; Ctrl+Enter runs after preview/diff".to_string()
    } else {
        "safety: Enter loads the selected action; Ctrl+Enter runs it".to_string()
    })
}

pub(crate) fn inspector_empty_state_actions(actions: &[&str]) -> Vec<String> {
    if actions.is_empty() {
        return vec!["No visible lines; try /status or /inspect status".to_string()];
    }
    actions.iter().map(|action| action.to_string()).collect()
}

pub(crate) fn inspector_pagination_footer(selected: usize, total: usize) -> String {
    inspector_footer_text(selected, total, None)
}

fn inspector_footer_text(selected: usize, total: usize, note: Option<&str>) -> String {
    let mut parts = vec![if total == 0 {
        "0/0".to_string()
    } else {
        format!("{}/{}", selected.min(total.saturating_sub(1)) + 1, total)
    }];
    if let Some(note) = note.and_then(compact_inspector_footer_note) {
        parts.push(note);
    }
    parts.push("Tab panel".to_string());
    parts.push("S-Tab focus".to_string());
    parts.push("/ search".to_string());
    parts.push("Enter load".to_string());
    parts.push("Ctrl+Enter run".to_string());
    if total > 1 {
        parts.push("PgUp/PgDn".to_string());
    }
    parts.push("Esc close".to_string());
    parts.join(" · ")
}

fn compact_inspector_footer_note(note: &str) -> Option<String> {
    let compact = note
        .replace(
            "Esc close inspector · return to confirmation with y / a / n",
            "y allow · a always · n deny",
        )
        .replace("Esc close inspector", "")
        .replace("Esc close", "")
        .trim()
        .trim_matches('·')
        .trim()
        .to_string();

    if compact.is_empty() {
        None
    } else {
        Some(compact)
    }
}

fn merge_inspector_footer_note(base: Option<&str>, extra: Option<&str>) -> Option<String> {
    let mut parts = Vec::new();
    if let Some(base) = base.and_then(compact_inspector_footer_note) {
        parts.push(base);
    }
    if let Some(extra) = extra.and_then(compact_inspector_footer_note) {
        parts.push(extra);
    }
    (!parts.is_empty()).then(|| parts.join(" · "))
}

fn compact_tab_count(count: usize) -> String {
    if count >= 100 {
        "99+".to_string()
    } else {
        count.to_string()
    }
}

fn badge_priority(label: &str) -> usize {
    match label {
        "state" => 0,
        "severity" => 1,
        "kind" => 2,
        "content" => 3,
        "reasoning" => 4,
        "summary" => 5,
        "access" => 6,
        "mode" => 7,
        "warning" => 8,
        "hint" => 9,
        "diff" => 10,
        "output" => 11,
        _ => 20,
    }
}

pub(crate) fn render_inspector(frame: &mut Frame, area: Rect, document: &InspectorDocument) {
    let Some(panel) = document.active_panel() else {
        return;
    };
    let filtered = document.filtered_indices();
    let total = filtered.len();

    let mut lines = vec![Line::from(vec![Span::styled(
        format!("  {} ", document.state.title),
        Style::default()
            .fg(PANEL_ACCENT)
            .add_modifier(Modifier::BOLD),
    )])];
    lines.push(Line::from(vec![Span::styled(
        format!(
            "  {} · {}{}",
            match document.state.focus {
                InspectorFocus::Tabs => "tabs",
                InspectorFocus::Body => "body",
                InspectorFocus::Actions => "actions",
            },
            document.tab_cycle_summary(),
            {
                let mut details = Vec::new();
                if matches!(document.state.focus, InspectorFocus::Body) && total > 0 {
                    details.push(format!(
                        " · line {}/{}",
                        document.state.selected_line.min(total.saturating_sub(1)) + 1,
                        total
                    ));
                }
                if document.state.search_active || !document.state.search_query.is_empty() {
                    details.push(format!(" · / {}", document.state.search_query));
                }
                if let Some(last_action) = &document.state.last_action_label {
                    let result = match document.state.last_action_result {
                        Some(InspectorActionResult::Success) => " ok",
                        Some(InspectorActionResult::Failure) => " failed",
                        None => "",
                    };
                    let detail = document
                        .state
                        .last_action_error
                        .as_deref()
                        .or(document.state.last_action_detail.as_deref())
                        .map(|detail| format!(": {}", detail))
                        .unwrap_or_default();
                    details.push(format!(
                        " · last={}{}{} @ {}",
                        last_action,
                        result,
                        detail,
                        document
                            .state
                            .last_action_at
                            .as_deref()
                            .unwrap_or("unknown")
                    ));
                }
                details.join("")
            }
        ),
        Style::default().fg(BORDER_MUTED),
    )]));
    lines.push(multi_pane_title_strip(
        &document.state.tabs,
        document.state.selected_tab,
        match document.state.focus {
            InspectorFocus::Tabs => SELECT_ACCENT,
            InspectorFocus::Body => PANEL_ACCENT,
            InspectorFocus::Actions => SELECT_ACCENT,
        },
        MUTED,
    ));
    if !panel.badges.is_empty() {
        let badges = panel
            .badges
            .iter()
            .map(|(label, value)| (label.as_str(), value.as_str()))
            .collect::<Vec<_>>();
        lines.push(inspector_status_badge_row(&badges, SELECT_ACCENT));
    }
    if !panel.actions.is_empty() {
        lines.push(inspector_action_row(
            &panel.actions,
            document.state.selected_action,
            SELECT_ACCENT,
            matches!(document.state.focus, InspectorFocus::Actions),
        ));
        if let Some(summary) = inspector_action_safety_summary(&panel.actions) {
            lines.push(Line::from(vec![Span::styled(
                format!("  {}", summary),
                Style::default().fg(BORDER_MUTED),
            )]));
        }
    }
    lines.push(section_title_line(&panel.tab.label, PANEL_ACCENT));

    let start = document.state.scroll_offset.min(total);
    let end = (start + 12).min(total);
    if start == end {
        for action in inspector_empty_state_actions(&["/status", "/inspect status", "Esc close"]) {
            lines.push(Line::from(format!("  {}", action)));
        }
    } else {
        for actual_index in filtered[start..end].iter() {
            let line = panel.lines.get(*actual_index).cloned().unwrap_or_default();
            let selected = *actual_index == document.state.selected_line;
            lines.push(Line::from(vec![
                Span::styled(
                    if selected { "  ❯ " } else { "    " },
                    if selected {
                        Style::default().fg(SELECT_ACCENT).bg(SELECT_BG)
                    } else {
                        Style::default().fg(MUTED)
                    },
                ),
                Span::styled(
                    line,
                    if selected {
                        Style::default().fg(LIGHT).bg(SELECT_BG)
                    } else {
                        Style::default().fg(MUTED)
                    },
                ),
            ]));
        }
    }

    let footer_note = merge_inspector_footer_note(
        document.footer.as_deref(),
        document
            .state
            .last_action_label
            .as_deref()
            .map(|_| "last action may be stale"),
    );
    let footer = footer_note
        .as_deref()
        .map(|note| inspector_footer_text(document.state.selected_line, total, Some(note)))
        .unwrap_or_else(|| inspector_pagination_footer(document.state.selected_line, total));
    lines.push(footer_hint_line(&[&footer], BORDER_MUTED));
    frame.render_widget(Paragraph::new(lines), area);
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PanelStackCoordinator {
    pub layers: Vec<String>,
}

impl PanelStackCoordinator {
    pub(crate) fn push(&mut self, id: impl Into<String>) {
        self.layers.push(id.into());
    }

    pub(crate) fn pop(&mut self) -> Option<String> {
        self.layers.pop()
    }
}

fn extract_command_target(text: &str) -> Option<String> {
    if let Some(start) = text.find("/inspect artifact ") {
        return extract_command_segment(&text[start..]);
    }
    let start = text.find('/')?;
    extract_command_segment(&text[start..])
}

fn extract_command_segment(rest: &str) -> Option<String> {
    let end = [" · ", " | ", "|"]
        .iter()
        .filter_map(|separator| rest.find(separator))
        .min()
        .unwrap_or(rest.len());
    let command = rest[..end].trim().trim_matches('`').to_string();
    (!command.is_empty()).then_some(command)
}

fn inspect_artifact_target(command: &str) -> Option<String> {
    command
        .trim()
        .strip_prefix("/inspect artifact ")
        .map(str::trim)
        .filter(|target| !target.is_empty())
        .map(ToString::to_string)
}

fn inspect_inspector_target(command: &str) -> Option<String> {
    read_only_inspector_target_from_command(command)
}

fn inspector_action_label_for_command(command: &str) -> String {
    if inspect_artifact_target(command).is_some() {
        "Open artifact".to_string()
    } else if let Some(target) = inspect_inspector_target(command) {
        inspector_target_action_label(&target)
    } else if command.trim().starts_with('/') {
        "Load command".to_string()
    } else {
        "Load prompt".to_string()
    }
}

fn inspector_target_action_label(target: &str) -> String {
    let parts = target.split_whitespace().collect::<Vec<_>>();
    match parts.as_slice() {
        ["plugin", "list"] => "Inspect plugins".to_string(),
        ["plugin", "inspect", name] => format!("Inspect plugin {}", name),
        ["skills", "list"] => "Inspect skills".to_string(),
        ["skills", "show", name] => format!("Inspect skill {}", name),
        ["keys"] => "Inspect keybindings".to_string(),
        ["help"] => "Open help".to_string(),
        ["history"] => "Inspect history".to_string(),
        ["history", "pick"] => "Inspect history picker".to_string(),
        ["history", "search", ..] => "Inspect history search".to_string(),
        ["tasks", "read", target] => format!("Inspect task output {}", target),
        ["teams"] | ["teams", "list"] => "Inspect teams".to_string(),
        ["teams", "latest"] => "Inspect latest team".to_string(),
        ["teams", "monitor"] => "Inspect team monitor".to_string(),
        ["teams", "monitor", selector] => format!("Inspect team monitor {}", selector),
        ["teams", "messages"] => "Inspect team messages".to_string(),
        ["teams", "messages", selector] => format!("Inspect team messages {}", selector),
        ["teams", selector] => format!("Inspect team {}", selector),
        ["remote-control", "retry-summary"] => "Inspect remote retries".to_string(),
        _ => format!("Inspect {}", target),
    }
}

#[cfg(test)]
mod tests {
    use ratatui::backend::TestBackend;
    use ratatui::layout::Rect;
    use ratatui::style::Color;
    use ratatui::Terminal;

    use super::{
        inspector_action_row, inspector_action_safety_summary, inspector_empty_state_actions,
        inspector_experiment_enabled, inspector_footer_text, inspector_pagination_footer,
        inspector_status_badge_row, merge_inspector_footer_note, multi_pane_title_strip,
        render_inspector, InspectorAction, InspectorActionEffect, InspectorActionKind,
        InspectorActionResult, InspectorActionTarget, InspectorBodySource, InspectorDocument,
        InspectorFocus, InspectorState, InspectorTab, PanelStackCoordinator,
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
        assert!(line.to_string().contains("Timeline ·2"));
        let badges = inspector_status_badge_row(&[("status", "running")], Color::Yellow);
        assert!(badges.to_string().contains("status=running"));
        let ordered = inspector_status_badge_row(
            &[
                ("hint", "rewrite"),
                ("state", "running"),
                ("severity", "warn"),
            ],
            Color::Yellow,
        )
        .to_string();
        assert!(ordered.find("state=running").unwrap() < ordered.find("hint=rewrite").unwrap());
        let actions = inspector_action_row(
            &[InspectorAction::command("rerun", "/workflows run latest")],
            0,
            Color::Green,
            true,
        );
        assert!(actions.to_string().contains("[rerun]"));
        let safety = inspector_action_safety_summary(&[InspectorAction::command(
            "restore",
            "/checkpoint restore latest",
        )])
        .unwrap();
        assert!(safety.contains("Ctrl+Enter runs"));
    }

    #[test]
    fn empty_actions_and_pagination_render_fallbacks() {
        assert_eq!(
            inspector_empty_state_actions(&[]),
            vec!["No visible lines; try /status or /inspect status".to_string()]
        );
        assert!(inspector_pagination_footer(0, 0).contains("0/0"));
        assert!(inspector_pagination_footer(0, 0).contains("Tab panel"));
        assert!(inspector_pagination_footer(0, 0).contains("/ search"));
        assert!(inspector_pagination_footer(1, 5).contains("2/5"));
        assert!(inspector_pagination_footer(1, 5).contains("PgUp/PgDn"));
        assert_eq!(
            inspector_footer_text(
                0,
                0,
                Some("Esc close inspector · return to confirmation with y / a / n"),
            ),
            "0/0 · y allow · a always · n deny · Tab panel · S-Tab focus · / search · Enter load · Ctrl+Enter run · Esc close"
        );
        assert_eq!(
            merge_inspector_footer_note(
                Some("Esc close inspector"),
                Some("last action may be stale"),
            )
            .as_deref(),
            Some("last action may be stale")
        );
    }

    #[test]
    fn inspector_action_labels_use_open_show_run_verbs() {
        let actions = [
            InspectorAction::load_command("Inspect status", "/status"),
            InspectorAction::open_inspector_target("Open help", "/help", "help"),
            InspectorAction::command("Run compact", "/compact"),
        ];
        assert!(actions.iter().all(|action| {
            action.label.starts_with("Inspect ")
                || action.label.starts_with("Open ")
                || action.label.starts_with("Load ")
                || action.label.starts_with("Run ")
        }));

        assert_eq!(
            InspectorAction::from_command("/plugin inspect demo", "/plugin inspect demo").label,
            "Inspect plugin demo"
        );
        assert_eq!(
            InspectorAction::from_command("/skills show rust", "/skills show rust").label,
            "Inspect skill rust"
        );
        assert_eq!(
            InspectorAction::from_command("/teams monitor team-demo", "/teams monitor team-demo")
                .label,
            "Inspect team monitor team-demo"
        );
        assert_eq!(
            InspectorAction::from_command(
                "/remote-control retry-summary",
                "/remote-control retry-summary",
            )
            .label,
            "Inspect remote retries"
        );
        assert_eq!(
            InspectorAction::from_command("/help", "/help").label,
            "Open help"
        );
        assert_eq!(
            InspectorAction::from_command("/history search build", "/history search build").label,
            "Inspect history search"
        );
    }

    #[test]
    fn panel_stack_tracks_active_layer() {
        let mut stack = PanelStackCoordinator::default();
        stack.push("task");
        stack.push("transcript");
        assert_eq!(stack.layers.last().map(String::as_str), Some("transcript"));
        assert_eq!(stack.pop().as_deref(), Some("transcript"));
        assert_eq!(stack.layers.last().map(String::as_str), Some("task"));
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

    #[test]
    fn inspector_search_filters_lines() {
        let mut doc = InspectorDocument::single(
            "demo",
            vec![
                "alpha".to_string(),
                "beta match".to_string(),
                "gamma".to_string(),
            ],
        );
        doc.begin_search();
        doc.append_search_char('m');
        doc.append_search_char('a');
        assert_eq!(doc.state.selected_line, 1);
        doc.finish_search(true);
        assert_eq!(doc.state.search_query, "ma");
        assert!(!doc.state.search_active);
    }

    #[test]
    fn inspector_action_focus_cycles_when_actions_exist() {
        let mut doc = InspectorDocument::single("demo", vec!["a".to_string()]);
        doc.panels[0]
            .actions
            .push(InspectorAction::load_command("run", "/status"));
        doc.toggle_focus();
        assert!(matches!(doc.state.focus, InspectorFocus::Actions));
        doc.note_action_dispatched("run");
        assert_eq!(doc.state.last_action_label.as_deref(), Some("run"));
    }

    #[test]
    fn tab_and_backtab_focus_behaviors_stay_distinct() {
        let mut doc = InspectorDocument::single("demo", vec!["a".to_string()]);
        let second_tab = InspectorTab {
            id: "second".to_string(),
            label: "Second".to_string(),
            item_count: Some(1),
        };
        doc.state.tabs.push(second_tab.clone());
        doc.panels.push(super::InspectorPanel {
            tab: second_tab,
            lines: vec!["b".to_string()],
            badges: Vec::new(),
            actions: vec![InspectorAction::load_command("Inspect status", "/status")],
        });

        doc.cycle_tab();
        assert_eq!(doc.state.selected_tab, 1);
        assert!(matches!(doc.state.focus, InspectorFocus::Body));

        doc.toggle_focus();
        assert_eq!(doc.state.selected_tab, 1);
        assert!(matches!(doc.state.focus, InspectorFocus::Actions));
    }

    #[test]
    fn typed_action_keeps_legacy_command_handoff() {
        let mut doc = InspectorDocument::single("demo", vec!["a".to_string()]);
        doc.panels[0]
            .actions
            .push(InspectorAction::load_command("Inspect status", "/status"));
        doc.toggle_focus();

        let action = doc.handoff_action().unwrap();
        assert_eq!(action.command, "/status");
        assert_eq!(doc.handoff_command().as_deref(), Some("/status"));
        assert_eq!(
            action.effect(false),
            InspectorActionEffect::LoadCommand("/status".to_string())
        );
        assert_eq!(
            action.effect(true),
            InspectorActionEffect::RunCommand("/status".to_string())
        );
        assert!(action
            .typed
            .as_ref()
            .is_some_and(|typed| typed.kind == InspectorActionKind::LoadCommand));
    }

    #[test]
    fn open_artifact_action_preserves_command_and_uses_execute_fallback() {
        let action =
            InspectorAction::from_command("Open artifact", "/inspect artifact history runtime");

        assert_eq!(action.command, "/inspect artifact history runtime");
        assert_eq!(
            action.effect(false),
            InspectorActionEffect::LoadCommand("/inspect artifact history runtime".to_string())
        );
        assert_eq!(
            action.effect(true),
            InspectorActionEffect::OpenArtifact {
                target: "history runtime".to_string(),
                command: "/inspect artifact history runtime".to_string(),
            }
        );
        assert!(action.typed.as_ref().is_some_and(|typed| {
            typed.kind == InspectorActionKind::OpenArtifact
                && typed.target == InspectorActionTarget::Artifact("history runtime".to_string())
        }));
    }

    #[test]
    fn open_inspector_target_loads_or_runs_command_handoff() {
        let action = InspectorAction::open_inspector_target(
            "Inspect diagnostics",
            "/diagnostics",
            "diagnostics",
        );

        assert_eq!(
            action.effect(false),
            InspectorActionEffect::LoadCommand("/diagnostics".to_string())
        );
        assert_eq!(
            action.effect(true),
            InspectorActionEffect::OpenInspectorTarget {
                target: "diagnostics".to_string(),
                command: "/diagnostics".to_string(),
            }
        );
        assert!(action.typed.as_ref().is_some_and(|typed| {
            typed.kind == InspectorActionKind::OpenInspectorTarget
                && typed.target == InspectorActionTarget::InspectorTarget("diagnostics".to_string())
        }));
    }

    #[test]
    fn inspect_target_command_is_typed_without_breaking_fallback_command() {
        let action = InspectorAction::from_command("Inspect status", "/inspect status");

        assert_eq!(action.command, "/inspect status");
        assert_eq!(
            action.effect(false),
            InspectorActionEffect::LoadCommand("/inspect status".to_string())
        );
        assert_eq!(
            action.effect(true),
            InspectorActionEffect::OpenInspectorTarget {
                target: "status".to_string(),
                command: "/inspect status".to_string(),
            }
        );
        assert!(action.typed.as_ref().is_some_and(|typed| {
            typed.kind == InspectorActionKind::OpenInspectorTarget
                && typed.target == InspectorActionTarget::InspectorTarget("status".to_string())
        }));
    }

    #[test]
    fn inspect_proxy_command_uses_same_read_only_typed_boundary() {
        let allowed =
            InspectorAction::from_command("Inspect checkpoint", "/inspect checkpoint latest");
        assert_eq!(
            allowed.effect(true),
            InspectorActionEffect::OpenInspectorTarget {
                target: "checkpoint latest".to_string(),
                command: "/inspect checkpoint latest".to_string(),
            }
        );

        for command in [
            "/inspect workflows run latest",
            "/inspect permissions governance",
            "/inspect doctor bundle",
            "/inspect checkpoint restore latest",
            "/inspect remote-control doctor",
            "/inspect hooks",
        ] {
            let action = InspectorAction::from_command(command, command);
            assert_eq!(action.label, "Load command");
            assert!(action.typed.is_none(), "{} should remain fallback", command);
            assert_eq!(
                action.effect(false),
                InspectorActionEffect::LoadCommand(command.to_string())
            );
            assert_eq!(
                action.effect(true),
                InspectorActionEffect::RunCommand(command.to_string())
            );
        }
    }

    #[test]
    fn selected_line_inspect_commands_use_typed_action_with_command_fallback() {
        let doc = InspectorDocument::single(
            "diagnostics",
            vec![
                "Recovery: ok -> /status".to_string(),
                "Artifact: inspect /inspect artifact latest-runtime-tasks".to_string(),
            ],
        );
        assert_eq!(doc.handoff_command().as_deref(), Some("/status"));
        let status_action = doc.handoff_action().unwrap();
        assert_eq!(status_action.label, "Inspect status");
        assert_eq!(
            status_action.effect(true),
            InspectorActionEffect::OpenInspectorTarget {
                target: "status".to_string(),
                command: "/status".to_string(),
            }
        );

        let mut doc = doc;
        doc.move_down();
        assert_eq!(
            doc.handoff_command().as_deref(),
            Some("/inspect artifact latest-runtime-tasks")
        );
        let artifact_action = doc.handoff_action().unwrap();
        assert_eq!(artifact_action.label, "Open artifact");
        assert_eq!(
            artifact_action.effect(true),
            InspectorActionEffect::OpenArtifact {
                target: "latest-runtime-tasks".to_string(),
                command: "/inspect artifact latest-runtime-tasks".to_string(),
            }
        );
    }

    #[test]
    fn compound_diagnostics_lines_prefer_artifact_inspect_target() {
        let doc = InspectorDocument::single(
            "diagnostics",
            vec![
                "Tasks: 1 running -> /tasks summary · artifact /inspect artifact latest-runtime-tasks"
                    .to_string(),
                "Status: ok -> /status | /diagnostics".to_string(),
            ],
        );

        assert_eq!(
            doc.handoff_command().as_deref(),
            Some("/inspect artifact latest-runtime-tasks")
        );
        let action = doc.handoff_action().unwrap();
        assert_eq!(action.label, "Open artifact");
        assert_eq!(
            action.effect(true),
            InspectorActionEffect::OpenArtifact {
                target: "latest-runtime-tasks".to_string(),
                command: "/inspect artifact latest-runtime-tasks".to_string(),
            }
        );

        let mut doc = doc;
        doc.move_down();
        assert_eq!(doc.handoff_command().as_deref(), Some("/status"));
    }

    #[test]
    fn read_only_tasks_and_reviews_commands_are_typed_inspector_targets() {
        let tasks = InspectorAction::from_command("/tasks monitor", "/tasks monitor");
        assert_eq!(tasks.label, "Inspect tasks monitor");
        assert_eq!(
            tasks.effect(true),
            InspectorActionEffect::OpenInspectorTarget {
                target: "tasks monitor".to_string(),
                command: "/tasks monitor".to_string(),
            }
        );

        let filtered_tasks =
            InspectorAction::from_command("/tasks latest failed", "/tasks latest failed");
        assert_eq!(filtered_tasks.label, "Inspect tasks latest failed");
        assert_eq!(
            filtered_tasks.effect(true),
            InspectorActionEffect::OpenInspectorTarget {
                target: "tasks latest failed".to_string(),
                command: "/tasks latest failed".to_string(),
            }
        );

        let reviews = InspectorAction::from_command("/reviews latest", "/reviews latest");
        assert_eq!(reviews.label, "Inspect reviews latest");
        assert_eq!(
            reviews.effect(true),
            InspectorActionEffect::OpenInspectorTarget {
                target: "reviews latest".to_string(),
                command: "/reviews latest".to_string(),
            }
        );

        let team_monitor =
            InspectorAction::from_command("/teams monitor team-demo", "/teams monitor team-demo");
        assert_eq!(team_monitor.label, "Inspect team monitor team-demo");
        assert_eq!(
            team_monitor.effect(true),
            InspectorActionEffect::OpenInspectorTarget {
                target: "teams monitor team-demo".to_string(),
                command: "/teams monitor team-demo".to_string(),
            }
        );

        let task_read = InspectorAction::from_command("/tasks read latest", "/tasks read latest");
        assert_eq!(task_read.label, "Inspect task output latest");
        assert_eq!(
            task_read.effect(true),
            InspectorActionEffect::OpenInspectorTarget {
                target: "tasks read latest".to_string(),
                command: "/tasks read latest".to_string(),
            }
        );
    }

    #[test]
    fn mutating_tasks_commands_stay_on_command_string_handoff() {
        for command in [
            "/tasks stop latest",
            "/tasks bundle latest",
            "/tasks issue latest",
            "/tasks follow latest",
        ] {
            let action = InspectorAction::from_command(command, command);
            assert_eq!(action.label, "Load command");
            assert!(action.typed.is_none());
            assert_eq!(
                action.effect(false),
                InspectorActionEffect::LoadCommand(command.to_string())
            );
            assert_eq!(
                action.effect(true),
                InspectorActionEffect::RunCommand(command.to_string())
            );
        }
    }

    #[test]
    fn broader_read_only_navigation_commands_are_typed_inspector_targets() {
        for (command, target, label) in [
            ("/memory latest", "memory latest", "Inspect memory latest"),
            (
                "/memory compare latest latest-1",
                "memory compare latest latest-1",
                "Inspect memory compare latest latest-1",
            ),
            (
                "/doctor remote-artifacts",
                "doctor remote-artifacts",
                "Inspect doctor remote-artifacts",
            ),
            (
                "/permissions sources",
                "permissions sources",
                "Inspect permissions sources",
            ),
            (
                "/permissions denials",
                "permissions denials",
                "Inspect permissions denials",
            ),
            (
                "/permissions mode guide",
                "permissions mode guide",
                "Inspect permissions mode guide",
            ),
            ("/context", "context", "Inspect context"),
            ("/brief", "brief", "Inspect brief"),
            ("/files", "files", "Inspect files"),
            ("/cost", "cost", "Inspect cost"),
            ("/config", "config", "Inspect config"),
            ("/version", "version", "Inspect version"),
            ("/keys", "keys", "Inspect keybindings"),
            ("/keybindings", "keys", "Inspect keybindings"),
            ("/time", "time", "Inspect time"),
            ("/help", "help", "Open help"),
            ("/history", "history", "Inspect history"),
            ("/history 20", "history 20", "Inspect history 20"),
            ("/history pick", "history pick", "Inspect history picker"),
            (
                "/history search build-failure",
                "history search build-failure",
                "Inspect history search",
            ),
            ("/update status", "update status", "Inspect update status"),
            (
                "/workflows latest",
                "workflows latest",
                "Inspect workflows latest",
            ),
            (
                "/workflows preview latest",
                "workflows preview latest",
                "Inspect workflows preview latest",
            ),
            (
                "/coordinate latest",
                "coordinate latest",
                "Inspect coordinate latest",
            ),
            (
                "/coordinate history",
                "coordinate history",
                "Inspect coordinate history",
            ),
            (
                "/remote-control latest",
                "remote-control latest",
                "Inspect remote-control latest",
            ),
            (
                "/remote-control queue",
                "remote-control queue",
                "Inspect remote-control queue",
            ),
            (
                "/remote-control tasks",
                "remote-control tasks",
                "Inspect remote-control tasks",
            ),
            (
                "/remote-control replay",
                "remote-control replay",
                "Inspect remote-control replay",
            ),
            (
                "/remote-control replay latest",
                "remote-control replay latest",
                "Inspect remote-control replay latest",
            ),
            (
                "/remote-control retry-summary",
                "remote-control retry-summary",
                "Inspect remote retries",
            ),
            ("/tools", "tools", "Inspect tools"),
            ("/tools diag", "tools diag", "Inspect tools diag"),
            ("/tools list", "tools list", "Inspect tools list"),
            ("/tools verbose", "tools verbose", "Inspect tools verbose"),
            ("/plugin list", "plugin list", "Inspect plugins"),
            (
                "/plugin inspect demo",
                "plugin inspect demo",
                "Inspect plugin demo",
            ),
            ("/skills list", "skills list", "Inspect skills"),
            (
                "/skills show rust",
                "skills show rust",
                "Inspect skill rust",
            ),
            ("/teams", "teams", "Inspect teams"),
            ("/teams list", "teams list", "Inspect teams"),
            ("/teams latest", "teams latest", "Inspect latest team"),
            ("/teams monitor", "teams monitor", "Inspect team monitor"),
            (
                "/teams monitor team-demo",
                "teams monitor team-demo",
                "Inspect team monitor team-demo",
            ),
            (
                "/teams messages team-demo",
                "teams messages team-demo",
                "Inspect team messages team-demo",
            ),
            (
                "/teams team-demo",
                "teams team-demo",
                "Inspect team team-demo",
            ),
            (
                "/checkpoint list",
                "checkpoint list",
                "Inspect checkpoint list",
            ),
            (
                "/checkpoint latest",
                "checkpoint latest",
                "Inspect checkpoint latest",
            ),
            (
                "/checkpoint diff latest latest-1",
                "checkpoint diff latest latest-1",
                "Inspect checkpoint diff latest latest-1",
            ),
            (
                "/checkpoint branch list",
                "checkpoint branch list",
                "Inspect checkpoint branch list",
            ),
            (
                "/checkpoint branch latest",
                "checkpoint branch latest",
                "Inspect checkpoint branch latest",
            ),
            (
                "/checkpoint branch diff latest latest-1",
                "checkpoint branch diff latest latest-1",
                "Inspect checkpoint branch diff latest latest-1",
            ),
            (
                "/checkpoint rollback list",
                "checkpoint rollback list",
                "Inspect checkpoint rollback list",
            ),
            (
                "/checkpoint rollback latest",
                "checkpoint rollback latest",
                "Inspect checkpoint rollback latest",
            ),
            (
                "/checkpoint rollback-dry-run latest",
                "checkpoint rollback-dry-run latest",
                "Inspect checkpoint rollback-dry-run latest",
            ),
            (
                "/checkpoint rewind-anchor",
                "checkpoint rewind-anchor",
                "Inspect checkpoint rewind-anchor",
            ),
            (
                "/checkpoint rewind-anchor latest",
                "checkpoint rewind-anchor latest",
                "Inspect checkpoint rewind-anchor latest",
            ),
        ] {
            let action = InspectorAction::from_command(command, command);
            assert_eq!(action.label, label);
            assert_eq!(
                action.effect(true),
                InspectorActionEffect::OpenInspectorTarget {
                    target: target.to_string(),
                    command: command.to_string(),
                }
            );
        }
    }

    #[test]
    fn write_or_state_changing_navigation_commands_stay_command_strings() {
        for command in [
            "/doctor bundle",
            "/permissions",
            "/permissions governance",
            "/permissions explain bash",
            "/permissions mode auto",
            "/permissions add project allow bash echo write=true",
            "/workflows run latest",
            "/workflows run-write latest",
            "/workflows init rust",
            "/workflows timeline",
            "/coordinate",
            "/coordinate timeline",
            "/hooks",
            "/mcp",
            "/mcp reload",
            "/mcp resources cleanup",
            "/plugin",
            "/plugin enable demo",
            "/plugin disable demo",
            "/skills",
            "/skills active",
            "/skills search rust tui",
            "/remote-control",
            "/remote-control plan",
            "/remote-control session",
            "/remote-control session status",
            "/remote-control session sync",
            "/remote-control transport",
            "/remote-control transport status",
            "/remote-control transport connect",
            "/remote-control transport disconnect",
            "/remote-control transport reconnect",
            "/remote-control monitor",
            "/remote-control doctor",
            "/remote-control follow latest",
            "/remote-control dispatch latest",
            "/remote-control run latest",
            "/remote-control complete latest remote completion confirmed",
            "/remote-control fail latest remote failure recorded",
            "/remote-control retry latest",
            "/remote-control ack latest",
            "/remote-control handoff latest",
            "/remote-control bundle",
            "/checkpoint",
            "/checkpoint save handoff",
            "/checkpoint restore latest",
            "/checkpoint restore-dry-run latest",
            "/checkpoint rewind latest",
            "/checkpoint branch save workstream-a",
            "/checkpoint branch merge latest",
            "/checkpoint branch merge-dry-run latest",
            "/checkpoint rewind-anchor save latest",
            "/history use 1",
            "/update",
            "/update check",
            "/update set auto_check true",
        ] {
            let action = InspectorAction::from_command(command, command);
            assert_eq!(action.label, "Load command");
            assert!(action.typed.is_none(), "{} should remain fallback", command);
        }
    }

    #[test]
    fn plain_command_string_fallback_still_has_no_typed_action() {
        let action = InspectorAction::from_command("Open model", "/model");

        assert_eq!(action.command, "/model");
        assert!(action.typed.is_none());
        assert_eq!(
            action.effect(true),
            InspectorActionEffect::RunCommand("/model".to_string())
        );
    }

    #[test]
    fn action_feedback_renders_success_and_failure() {
        let mut doc = InspectorDocument::single("demo", vec!["visible".to_string()]);
        doc.note_action_succeeded("Inspect status");
        assert_eq!(
            doc.state.last_action_result,
            Some(InspectorActionResult::Success)
        );

        let area = Rect::new(0, 0, 120, 12);
        let backend = TestBackend::new(area.width, area.height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render_inspector(frame, area, &doc))
            .unwrap();
        let rendered = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(rendered.contains("last=Inspect status ok"));

        doc.note_action_failed("allow once", "no pending confirmation");
        terminal
            .draw(|frame| render_inspector(frame, area, &doc))
            .unwrap();
        let rendered = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(rendered.contains("last=allow once failed: no pending confirmation"));

        doc.note_action_succeeded_with_detail(
            "Open artifact",
            "fallback ran command: open artifact",
        );
        terminal
            .draw(|frame| render_inspector(frame, area, &doc))
            .unwrap();
        let rendered = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(rendered.contains("last=Open artifact ok: fallback ran command: open artifact"));
    }
}
