use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};

/// Terminal events.
#[derive(Debug)]
pub enum AppEvent {
    /// Key press
    Key(KeyEvent),
    /// Bracketed paste (text may contain newlines)
    Paste(String),
    /// Terminal resized
    Resize(u16, u16),
    /// Terminal tick (for animations / polling)
    Tick,
}

/// Poll for terminal events with a timeout.
pub fn poll_event(timeout: Duration) -> Result<Option<AppEvent>> {
    if event::poll(timeout)? {
        match event::read()? {
            Event::Key(key) => Ok(Some(AppEvent::Key(key))),
            Event::Paste(text) => Ok(Some(AppEvent::Paste(text))),
            Event::Resize(w, h) => Ok(Some(AppEvent::Resize(w, h))),
            Event::FocusGained | Event::FocusLost => Ok(None),
            Event::Mouse(_) => Ok(None),
        }
    } else {
        Ok(Some(AppEvent::Tick))
    }
}

/// Check if this is a quit key combination (Ctrl+C).
pub fn is_quit(key: &KeyEvent) -> bool {
    matches!(
        key,
        KeyEvent {
            code: KeyCode::Char('c'),
            modifiers: KeyModifiers::CONTROL,
            ..
        }
    )
}
