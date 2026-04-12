use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::wizard::{Wizard, WizardStep};
use super::palette::{ERROR_COLOR, INPUT_BG, LIGHT, MUTED, PANEL_ACCENT, SELECT_ACCENT};

/// Render the wizard in the viewport.
pub fn render_wizard(frame: &mut Frame, area: Rect, wizard: &Wizard) {
    if area.height == 0 {
        return;
    }

    let step = match wizard.current_step() {
        Some(s) => s,
        None => return,
    };

    let mut lines: Vec<Line> = Vec::new();

    // Title line
    lines.push(Line::from(vec![Span::styled(
        format!("  {} ", wizard.title),
        Style::default()
            .fg(PANEL_ACCENT)
            .add_modifier(Modifier::BOLD),
    )]));

    // Step prompt
    lines.push(Line::from(vec![
        Span::styled(
            format!("  {} ", wizard.step_label()),
            Style::default().fg(MUTED),
        ),
        Span::styled(step.prompt().to_string(), Style::default().fg(LIGHT)),
    ]));

    // Step-specific content
    match step {
        WizardStep::Select { options, .. } => {
            for (i, option) in options.iter().enumerate() {
                if i == wizard.select_index {
                    lines.push(Line::from(vec![
                        Span::styled(
                            "  ❯ ",
                            Style::default()
                                .fg(SELECT_ACCENT)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            option.clone(),
                            Style::default()
                                .fg(Color::White)
                                .add_modifier(Modifier::BOLD),
                        ),
                    ]));
                } else {
                    lines.push(Line::from(vec![
                        Span::styled("    ", Style::default()),
                        Span::styled(option.clone(), Style::default().fg(MUTED)),
                    ]));
                }
            }
        }
        WizardStep::Input { default, .. } => {
            let (text, is_placeholder) = if wizard.input_buf.is_empty() {
                let placeholder = if let Some(d) = default {
                    if d.is_empty() {
                        "(empty, press Enter to skip)".to_string()
                    } else {
                        format!("{} (Enter for default)", d)
                    }
                } else {
                    String::new()
                };
                (placeholder, true)
            } else {
                (wizard.input_buf.clone(), false)
            };

            let mut spans = vec![
                Span::styled("  > ", Style::default().fg(SELECT_ACCENT)),
                Span::styled(
                    text,
                    Style::default()
                        .fg(if is_placeholder { MUTED } else { LIGHT })
                        .bg(INPUT_BG),
                ),
            ];
            if !is_placeholder {
                spans.push(Span::styled(
                    "█",
                    Style::default().fg(LIGHT).bg(INPUT_BG),
                ));
            }
            lines.push(Line::from(spans));
        }
    }

    // Error message
    if let Some(ref err) = wizard.error {
        lines.push(Line::from(vec![Span::styled(
            format!("  ✘ {}", err),
            Style::default().fg(ERROR_COLOR),
        )]));
    }

    // Hint line
    let hint = match step {
        WizardStep::Select { .. } => "↑↓ select · Enter confirm · Esc cancel",
        WizardStep::Input { .. } => "Enter confirm · Esc cancel",
    };
    lines.push(Line::from(vec![Span::styled(
        format!("  {}", hint),
        Style::default().fg(Color::DarkGray),
    )]));

    frame.render_widget(Paragraph::new(lines), area);
}
