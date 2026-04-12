use ratatui::style::Color;

use super::responsive::{density_from_width, Density};

pub(crate) const HEADER_LOGO: [&str; 6] = [
    "██╗   ██╗ ██████╗ ██████╗ ███████╗",
    "╚██╗ ██╔╝██╔═══██╗██╔══██╗██╔════╝",
    " ╚████╔╝ ██║   ██║██║  ██║█████╗  ",
    "  ╚██╔╝  ██║   ██║██║  ██║██╔══╝  ",
    "   ██║   ╚██████╔╝██████╔╝███████╗",
    "   ╚═╝    ╚═════╝ ╚═════╝ ╚══════╝",
];

pub(crate) fn header_gradient() -> [Color; 8] {
    [
        Color::Indexed(37),
        Color::Indexed(37),
        Color::Indexed(44),
        Color::Indexed(45),
        Color::Indexed(81),
        Color::Indexed(115),
        Color::Indexed(120),
        Color::Indexed(120),
    ]
}

pub(crate) fn should_show_logo(width: usize, logo_width: usize) -> bool {
    let inner_width = width.saturating_sub(4);
    matches!(density_from_width(width as u16, 72, 110), Density::Wide)
        && inner_width > logo_width + 25
}
