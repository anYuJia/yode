use crossterm::event::{KeyEvent, KeyModifiers};

use super::history::BrowseResult;
use super::App;

pub(super) fn handle_char(app: &mut App, key: KeyEvent, c: char) {
    // Clear suggestion when user starts typing
    app.input.clear_ghost_text();
    app.suggestion_generating = false;

    if key.modifiers.contains(KeyModifiers::CONTROL) {
        match c {
            'a' => app.input.move_home(),
            'e' => app.input.move_end(),
            'u' => app.input.clear(),
            'k' => app.input.kill_to_end(),
            'w' => app.input.delete_word_back(),
            'l' => {
                app.chat_entries.clear();
                app.printed_count = 0;
            }
            'j' => app.input.insert_newline(),
            'r' => app.history.enter_search(),
            'p' => browse_history_prev(app),
            'n' => browse_history_next(app),
            _ => {}
        }
    } else {
        app.input.insert_char(c);
        {
            let ctx = crate::commands::context::CompletionContext {
                provider_models: &app.provider_models,
                all_provider_models: &app.all_provider_models,
                provider_name: &app.provider_name,
                tools: &app.tools,
                working_dir: &app.session.working_dir,
            };
            app.cmd_completion.update(
                &app.input.lines[0],
                !app.input.is_multiline(),
                &app.cmd_registry,
                &ctx,
            );
        }
        if c == '@' || app.file_completion.is_active() {
            app.file_completion.update(&app.input.text());
        }
    }
}

pub(super) fn handle_up(app: &mut App) {
    if app.file_completion.is_active() {
        app.file_completion.cycle_back();
    } else if app.cmd_completion.is_active() && !app.input.is_multiline() {
        app.cmd_completion.cycle();
    } else if app.input.is_multiline() {
        app.input.move_up();
    } else {
        browse_history_prev(app);
    }
}

pub(super) fn handle_down(app: &mut App) {
    if app.file_completion.is_active() {
        app.file_completion.cycle();
    } else if app.cmd_completion.is_active() && !app.input.is_multiline() {
        app.cmd_completion.cycle_back();
    } else if app.input.is_multiline() {
        app.input.move_down();
    } else {
        browse_history_next(app);
    }
}

pub(super) fn browse_history_prev(app: &mut App) {
    if !app.history.is_browsing() {
        app.history.start_browse(app.input.lines.clone());
    }
    if let Some(entry) = app.history.current_browse_entry() {
        app.input.set_text(entry);
    }
    if let Some(text) = app.history.browse_prev() {
        app.input.set_text(text);
    }
}

pub(super) fn browse_history_next(app: &mut App) {
    match app.history.browse_next() {
        BrowseResult::Entry(text) => app.input.set_text(&text),
        BrowseResult::Restore(lines) => {
            app.input.lines = lines;
            app.input.cursor_line = 0;
            app.input.cursor_col = app.input.lines[0].chars().count();
        }
        BrowseResult::None => {}
    }
}

pub(super) fn handle_tab(app: &mut App) {
    if let Some(suggestion) = app.prompt_suggestion.take() {
        app.input.set_text(&suggestion);
        app.input.clear_ghost_text();
        return;
    }

    if app.file_completion.is_active() {
        if app.file_completion.candidates.len() == 1 {
            if let Some(path) = app.file_completion.accept() {
                let text = app.input.text();
                if let Some(at_pos) = text.rfind('@') {
                    let new_text = format!("{}@{}", &text[..at_pos], path);
                    app.input.set_text(&new_text);
                }
            }
        } else {
            app.file_completion.cycle();
        }
    } else if app.cmd_completion.is_active() {
        if let Some(cmd) = app.cmd_completion.accept() {
            app.input.set_text(&cmd);
        }
    } else {
        {
            let ctx = crate::commands::context::CompletionContext {
                provider_models: &app.provider_models,
                all_provider_models: &app.all_provider_models,
                provider_name: &app.provider_name,
                tools: &app.tools,
                working_dir: &app.session.working_dir,
            };
            app.cmd_completion.update(
                &app.input.lines[0],
                !app.input.is_multiline(),
                &app.cmd_registry,
                &ctx,
            );
        }
        if app.cmd_completion.candidates.len() == 1 {
            if let Some(cmd) = app.cmd_completion.accept() {
                app.input.set_text(&cmd);
            }
        }
    }
}
