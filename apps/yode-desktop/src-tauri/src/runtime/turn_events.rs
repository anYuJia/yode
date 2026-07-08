use tauri::{AppHandle, Emitter};

use crate::protocol::DesktopEvent;

pub(super) fn emit_desktop_event(app: &AppHandle, desktop_event: DesktopEvent) {
    let session_id = desktop_event.session_id.clone();
    let turn_id = desktop_event.turn_id.clone();
    let kind = desktop_event.kind.clone();
    if let Err(err) = app.emit("desktop-event", desktop_event) {
        tracing::warn!(
            session_id = %session_id,
            turn_id = %turn_id,
            kind = %kind,
            error = %err,
            "Failed to emit desktop runtime event"
        );
    }
}
