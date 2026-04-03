use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::wizard::{Wizard, WizardStep};

const TITLE_COLOR: Color = Color::Yellow;
const PROMPT_COLOR: Color = Color::White;
const SEL_COLOR: Color = Color::LightGreen;
const DIM: Color = Color::Gray;
const INPUT_BG: Color = Color::Indexed(236);
const ERROR_COLOR: Color = Color::LightRed;

/// Render the wizard in the viewport.
pub fn render_wizard(frame: &mut Frame, area: Rect, wizard: &Wizard) {
    if area.height == 0 { return; }

    let step = match wizard.current_step() {
        Some(s) => s,
        None => return,
    };

    let mut lines: Vec<Line> = Vec::new();

    // Title line
    lines.push(Line::from(vec![
        Span::styled(
            format!("  {} ", wizard.title),
            Style::default().fg(TITLE_COLOR).add_modifier(Modifier::BOLD),
        ),
    ]));

    // Step prompt
    lines.push(Line::from(vec![
        Span::styled(
            format!("  {} ", wizard.step_label()),
            Style::default().fg(DIM),
        ),
        Span::styled(
            step.prompt().to_string(),
            Style::default().fg(PROMPT_COLOR),
        ),
    ]));

    // Step-specific content
    match step {
        WizardStep::Select { options, .. } => {
            for (i, option) in options.iter().enumerate() {
                if i == wizard.select_index {
                    lines.push(Line::from(vec![
                        Span::styled("  ❯ ", Style::default().fg(SEL_COLOR).add_modifier(Modifier::BOLD)),
                        Span::styled(
                            option.clone(),
                            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
                        ),
                    ]));
                } else {
                    lines.push(Line::from(vec![
                        Span::styled("    ", Style::default()),
                        Span::styled(option.clone(), Style::default().fg(DIM)),
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
                Span::styled("  > ", Style::default().fg(SEL_COLOR)),
                Span::styled(
                    text,
                    Style::default()
                        .fg(if is_placeholder { DIM } else { Color::White })
                        .bg(INPUT_BG),
                ),
            ];
            if !is_placeholder {
                spans.push(Span::styled("█", Style::default().fg(Color::White).bg(INPUT_BG)));
            }
            lines.push(Line::from(spans));
        }
    }

    // Error message
    if let Some(ref err) = wizard.error {
        lines.push(Line::from(vec![
            Span::styled(format!("  ✘ {}", err), Style::default().fg(ERROR_COLOR)),
        ]));
    }

    // Hint line
    let hint = match step {
        WizardStep::Select { .. } => "↑↓ select · Enter confirm · Esc cancel",
        WizardStep::Input { .. } => "Enter confirm · Esc cancel",
    };
    lines.push(Line::from(vec![
        Span::styled(
            format!("  {}", hint),
            Style::default().fg(Color::DarkGray),
        ),
    ]));

    frame.render_widget(
        Paragraph::new(lines),
        area,
    );
}
