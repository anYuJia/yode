use ratatui::style::Color;

pub const SEP: Color = Color::DarkGray;
pub const MUTED: Color = Color::Indexed(243);
pub const LIGHT: Color = Color::Indexed(253);

pub const SURFACE_BG: Color = Color::Indexed(236);
pub const SURFACE_BG_ALT: Color = Color::Indexed(237);
pub const BORDER_MUTED: Color = Color::Indexed(239);

pub const PROMPT_COLOR: Color = Color::Indexed(109);
pub const PROMPT_DIM: Color = Color::Indexed(239);
pub const TEXT_COLOR: Color = LIGHT;
pub const HINT_COLOR: Color = BORDER_MUTED;
pub const GHOST_COLOR: Color = Color::Indexed(242);

pub const SELECT_ACCENT: Color = Color::Indexed(110);
pub const SELECT_BG: Color = Color::Indexed(238);
pub const PANEL_ACCENT: Color = Color::Indexed(109);
pub const INPUT_BG: Color = SURFACE_BG_ALT;
pub const INFO_COLOR: Color = Color::Indexed(109);
pub const USER_COLOR: Color = Color::Indexed(145);
pub const USER_PREFIX: Color = Color::Indexed(110);
pub const TOOL_ACCENT: Color = Color::Indexed(110);
pub const WARNING_COLOR: Color = Color::Indexed(180);
pub const SUCCESS_COLOR: Color = Color::Indexed(108);
pub const ERROR_COLOR: Color = Color::Indexed(167);
