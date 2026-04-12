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
