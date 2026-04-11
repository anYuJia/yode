mod attachments;
mod completions;
mod render;

pub use attachments::render_attachments;
pub use completions::render_command_inline;
pub use render::render_input;

const PROMPT_COLOR: ratatui::style::Color = ratatui::style::Color::LightGreen;
const PROMPT_DIM: ratatui::style::Color = ratatui::style::Color::DarkGray;
const TEXT_COLOR: ratatui::style::Color = ratatui::style::Color::White;
const HINT_COLOR: ratatui::style::Color = ratatui::style::Color::DarkGray;
const GHOST_COLOR: ratatui::style::Color = ratatui::style::Color::DarkGray;
