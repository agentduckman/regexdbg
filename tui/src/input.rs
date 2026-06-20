use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

use crate::app::{App, Focus};

pub fn handle_event(app: &mut App, event: Event) {
    let Event::Key(KeyEvent { code, modifiers, .. }) = event else { return };

    if app.show_help {
        app.show_help = false;
        return;
    }

    match app.focus {
        Focus::Pattern => handle_pattern(app, code, modifiers),
        Focus::Content => handle_content(app, code, modifiers),
    }
}

fn handle_pattern(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
    match (code, modifiers) {
        (KeyCode::Esc, _) | (KeyCode::Tab, _) => app.focus = Focus::Content,
        (KeyCode::Char('q'), KeyModifiers::CONTROL) => app.quit = true,
        (KeyCode::F(1), _) => app.show_help = true,
        (KeyCode::F(2), _) => app.copy_pattern_to_clipboard(),

        (KeyCode::Left,  _) => app.pattern_cursor_left(),
        (KeyCode::Right, _) => app.pattern_cursor_right(),
        (KeyCode::Home,  _) => app.pattern_cursor_home(),
        (KeyCode::End,   _) => app.pattern_cursor_end(),

        (KeyCode::Backspace, _) => app.pattern_backspace(),

        (KeyCode::Char(c), _) => app.pattern_insert(c),
        _ => {}
    }
}

fn handle_content(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
    match (code, modifiers) {
        (KeyCode::Char('q'), _) => app.quit = true,
        (KeyCode::Tab, _) | (KeyCode::Enter, _) => app.focus = Focus::Pattern,
        (KeyCode::Char('/'), _) => app.focus = Focus::Pattern,
        (KeyCode::F(1), _) => app.show_help = true,
        (KeyCode::F(2), _) => app.copy_pattern_to_clipboard(),

        // Scrolling
        (KeyCode::Up,   _) | (KeyCode::Char('k'), _) => app.scroll_up(1),
        (KeyCode::Down, _) | (KeyCode::Char('j'), _) => app.scroll_down(1),
        (KeyCode::PageUp,   _) | (KeyCode::Char('b'), _) => app.page_up(),
        (KeyCode::PageDown, _) | (KeyCode::Char('f'), _) => app.page_down(),
        (KeyCode::Char('g'), _) | (KeyCode::Home, _) => app.scroll_top(),
        (KeyCode::Char('G'), _) | (KeyCode::End,  _) => app.scroll_bottom(),

        // Match navigation
        (KeyCode::Char('n'), _) => app.next_match(),
        (KeyCode::Char('N'), _) => app.prev_match(),

        _ => {}
    }
}
