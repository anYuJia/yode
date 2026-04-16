use ratatui::style::Color;

pub const SEP: Color = Color::DarkGray;
pub const MUTED: Color = Color::Indexed(245);
pub const LIGHT: Color = Color::Indexed(255);

pub const SURFACE_BG: Color = Color::Indexed(235);
pub const SURFACE_BG_ALT: Color = Color::Indexed(237);
pub const BORDER_MUTED: Color = Color::Indexed(240);

pub const PROMPT_COLOR: Color = Color::Indexed(114);
pub const PROMPT_DIM: Color = Color::Indexed(240);
pub const TEXT_COLOR: Color = LIGHT;
pub const HINT_COLOR: Color = BORDER_MUTED;
pub const GHOST_COLOR: Color = Color::Indexed(242);

pub const SELECT_ACCENT: Color = Color::Indexed(31);
pub const SELECT_BG: Color = Color::Indexed(24);
pub const PANEL_ACCENT: Color = Color::Indexed(180);
pub const INPUT_BG: Color = SURFACE_BG_ALT;
pub const INFO_COLOR: Color = Color::Indexed(110);
pub const USER_COLOR: Color = Color::Indexed(153);
pub const USER_PREFIX: Color = Color::Indexed(117);
pub const TOOL_ACCENT: Color = Color::Indexed(116);
pub const WARNING_COLOR: Color = Color::Indexed(179);
pub const SUCCESS_COLOR: Color = Color::Indexed(114);
pub const ERROR_COLOR: Color = Color::Indexed(174);
