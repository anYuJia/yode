use ratatui::style::{Color, Style};

pub(super) struct RoleStylePalette {
    pub dim: Style,
    pub accent: Style,
    pub cyan: Style,
    pub white: Style,
    pub red: Style,
}

pub(super) fn role_style_palette() -> RoleStylePalette {
    RoleStylePalette {
        dim: Style::default().fg(Color::Gray),
        accent: Style::default().fg(Color::LightMagenta),
        cyan: Style::default().fg(Color::Indexed(51)),
        white: Style::default().fg(Color::Indexed(231)),
        red: Style::default().fg(Color::LightRed),
    }
}

pub(super) fn scrollback_render_width(subtract: u16, fallback: usize) -> usize {
    crossterm::terminal::size()
        .map(|(width, _)| width.saturating_sub(subtract) as usize)
        .unwrap_or(fallback)
        .max(24)
}

#[cfg(test)]
mod tests {
    use super::scrollback_render_width;

    #[test]
    fn scrollback_render_width_keeps_narrow_width_floor() {
        assert!(scrollback_render_width(u16::MAX, 0) >= 24);
        assert!(scrollback_render_width(4, 18) >= 24);
    }
}
