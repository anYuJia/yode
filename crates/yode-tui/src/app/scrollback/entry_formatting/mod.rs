mod roles;

pub(super) use roles::format_entry_as_strings;

pub(super) fn md_line_color(line: &str) -> (crossterm::style::Color, bool) {
    if line.starts_with("━━ ") || line.starts_with("━━━") {
        (crossterm::style::Color::Yellow, true)
    } else if line.starts_with("▸ ") {
        (crossterm::style::Color::Blue, true)
    } else if line.starts_with("  ▹ ") {
        (crossterm::style::Color::Cyan, false)
    } else if line.starts_with("    ") && !line.trim().is_empty() {
        (crossterm::style::Color::Green, false)
    } else if line.starts_with("▎ ") {
        (crossterm::style::Color::DarkYellow, false)
    } else if line.starts_with("────") {
        (crossterm::style::Color::DarkGrey, false)
    } else if line.starts_with("── ") || line.starts_with("───") {
        (crossterm::style::Color::Cyan, true)
    } else if line.contains('│') {
        (crossterm::style::Color::White, false)
    } else {
        (crossterm::style::Color::Reset, false)
    }
}
