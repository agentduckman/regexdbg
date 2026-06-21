use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

use crate::app::{App, Focus, VimMode};

pub fn handle_event(app: &mut App, event: Event) {
    match event {
        Event::Paste(text) => handle_paste(app, text),
        Event::Key(KeyEvent { code, modifiers, .. }) => handle_key(app, code, modifiers),
        _ => {}
    }
}

fn handle_paste(app: &mut App, text: String) {
    if app.editable && app.focus == Focus::Content {
        if app.vim_mode == VimMode::Normal || app.vim_mode == VimMode::Visual {
            app.push_undo();
            app.vim_exit_to_normal();
        }
        app.content_insert_str(&text);
        app.scroll_to_cursor();
    } else if app.focus == Focus::Pattern {
        // Strip newlines — regex pattern is single-line.
        let s: String = text.chars().filter(|&c| c != '\n' && c != '\r').collect();
        if !s.is_empty() {
            if app.vim_mode == VimMode::Normal || app.vim_mode == VimMode::Visual {
                app.pattern_push_undo();
                app.vim_exit_to_normal();
            }
            for ch in s.chars() {
                app.pattern_insert(ch);
            }
        }
    }
}

fn handle_key(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
    if app.show_help {
        app.show_help = false;
        return;
    }

    // Global quit — works in every mode and focus state.
    if code == KeyCode::F(12) {
        app.quit = true;
        return;
    }

    match app.focus {
        Focus::Pattern => handle_pattern(app, code, modifiers),
        Focus::Content => handle_content(app, code, modifiers),
    }
}

// ---------------------------------------------------------------------------
// Pattern pane — dispatches by vim mode
// ---------------------------------------------------------------------------

fn handle_pattern(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
    match app.vim_mode {
        VimMode::Normal => handle_pattern_vim_normal(app, code, modifiers),
        VimMode::Insert => handle_pattern_vim_insert(app, code, modifiers),
        VimMode::Visual => handle_pattern_vim_visual(app, code, modifiers),
    }
}

fn switch_to_content(app: &mut App) {
    app.focus = Focus::Content;
    app.vim_exit_to_normal();
}

fn switch_to_pattern(app: &mut App) {
    app.focus = Focus::Pattern;
    app.vim_exit_to_normal();
}

fn handle_pattern_vim_normal(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
    // Two-key sequences: dd, cc, yy.
    if let Some(pending) = app.vim_pending.take() {
        match (pending, code) {
            ('d', KeyCode::Char('d')) => { app.pattern_vim_delete_all();  return; }
            ('c', KeyCode::Char('c')) => { app.pattern_vim_change_all();  return; }
            ('y', KeyCode::Char('y')) => { app.pattern_vim_yank_all();    return; }
            _ => {}
        }
    }

    match (code, modifiers) {
        // Focus switch
        (KeyCode::Tab, _) | (KeyCode::Enter, _) => switch_to_content(app),
        (KeyCode::Char('q'), KeyModifiers::CONTROL)
        | (KeyCode::Char('c'), KeyModifiers::CONTROL) => app.quit = true,
        (KeyCode::F(1), _) => app.show_help = true,
        (KeyCode::F(2), _) => app.copy_pattern_to_clipboard(),

        // Cursor motion
        (KeyCode::Char('h'), _) | (KeyCode::Left,  _) => app.pattern_cursor_left(),
        (KeyCode::Char('l'), _) | (KeyCode::Right, _) => app.pattern_cursor_right(),
        (KeyCode::Char('w'), _) => app.pattern_vim_word_fwd(),
        (KeyCode::Char('b'), _) => app.pattern_vim_word_back(),
        (KeyCode::Char('e'), _) => app.pattern_vim_word_end(),
        (KeyCode::Char('0'), _) | (KeyCode::Char('^'), _) | (KeyCode::Home, _) => app.pattern_cursor_home(),
        (KeyCode::Char('$'), _) | (KeyCode::End, _) => app.pattern_cursor_end(),

        // Enter insert mode
        (KeyCode::Char('i'), _) => app.pattern_vim_enter_insert(),
        (KeyCode::Char('I'), _) => app.pattern_vim_enter_insert_home(),
        (KeyCode::Char('a'), _) => app.pattern_vim_enter_insert_after(),
        (KeyCode::Char('A'), _) => app.pattern_vim_enter_insert_end(),

        // Enter visual mode
        (KeyCode::Char('v'), _) => app.pattern_vim_enter_visual(),

        // Edit
        (KeyCode::Char('x'), _) => app.pattern_vim_delete_char(),
        (KeyCode::Char('D'), _) => app.pattern_vim_delete_to_end(),
        (KeyCode::Char('d'), _) => { app.vim_pending = Some('d'); }
        (KeyCode::Char('c'), _) => { app.vim_pending = Some('c'); }
        (KeyCode::Char('y'), _) => { app.vim_pending = Some('y'); }
        (KeyCode::Char('p'), _) => app.pattern_vim_paste_after(),
        (KeyCode::Char('P'), _) => app.pattern_vim_paste_before(),
        (KeyCode::Char('u'), _) => app.pattern_vim_undo(),

        _ => {}
    }
}

fn handle_pattern_vim_insert(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
    match (code, modifiers) {
        (KeyCode::Esc, _) => app.vim_exit_to_normal(),
        (KeyCode::Tab, _) => switch_to_content(app),
        (KeyCode::Char('q'), KeyModifiers::CONTROL)
        | (KeyCode::Char('c'), KeyModifiers::CONTROL) => app.quit = true,
        (KeyCode::F(1), _) => app.show_help = true,

        // Cursor movement
        (KeyCode::Left,  _) => app.pattern_cursor_left(),
        (KeyCode::Right, _) => app.pattern_cursor_right(),
        (KeyCode::Home,  _) => app.pattern_cursor_home(),
        (KeyCode::End,   _) => app.pattern_cursor_end(),

        // Edit
        (KeyCode::Backspace, _) => app.pattern_backspace(),
        (KeyCode::Delete, _) => app.pattern_vim_delete_char(),

        // Printable chars (unmodified or shift-only)
        (KeyCode::Char(c), mods)
            if mods == KeyModifiers::NONE || mods == KeyModifiers::SHIFT =>
        {
            app.pattern_insert(c);
        }

        _ => {}
    }
}

fn handle_pattern_vim_visual(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
    match (code, modifiers) {
        (KeyCode::Esc, _) => app.vim_exit_to_normal(),
        (KeyCode::Tab, _) => switch_to_content(app),
        (KeyCode::Char('q'), KeyModifiers::CONTROL)
        | (KeyCode::Char('c'), KeyModifiers::CONTROL) => app.quit = true,
        (KeyCode::F(1), _) => app.show_help = true,

        // Extend selection with motion
        (KeyCode::Char('h'), _) | (KeyCode::Left,  _) => app.pattern_cursor_left(),
        (KeyCode::Char('l'), _) | (KeyCode::Right, _) => app.pattern_cursor_right(),
        (KeyCode::Char('w'), _) => app.pattern_vim_word_fwd(),
        (KeyCode::Char('b'), _) => app.pattern_vim_word_back(),
        (KeyCode::Char('e'), _) => app.pattern_vim_word_end(),
        (KeyCode::Char('0'), _) | (KeyCode::Char('^'), _) | (KeyCode::Home, _) => app.pattern_cursor_home(),
        (KeyCode::Char('$'), _) | (KeyCode::End, _) => app.pattern_cursor_end(),

        // Operators
        (KeyCode::Char('d'), _) | (KeyCode::Char('x'), _) => app.pattern_vim_delete_selection(),
        (KeyCode::Char('y'), _) => app.pattern_vim_yank_selection(),
        (KeyCode::Char('c'), _) => app.pattern_vim_change_selection(),

        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Content pane — dispatches by vim mode
// ---------------------------------------------------------------------------

fn handle_content(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
    if app.editable {
        match app.vim_mode {
            VimMode::Normal => handle_vim_normal(app, code, modifiers),
            VimMode::Insert => handle_vim_insert(app, code, modifiers),
            VimMode::Visual => handle_vim_visual(app, code, modifiers),
        }
    } else {
        handle_content_nav(app, code, modifiers);
    }
}

fn handle_vim_normal(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
    if let Some(pending) = app.vim_pending.take() {
        match (pending, code) {
            ('d', KeyCode::Char('d')) => { app.vim_delete_line(); return; }
            ('y', KeyCode::Char('y')) => { app.vim_yank_line();   return; }
            ('g', KeyCode::Char('g')) => { app.vim_goto_top();    return; }
            _ => {}
        }
    }

    match (code, modifiers) {
        (KeyCode::Tab, _) => switch_to_pattern(app),
        (KeyCode::Char('q'), KeyModifiers::CONTROL)
        | (KeyCode::Char('c'), KeyModifiers::CONTROL) => app.quit = true,
        (KeyCode::F(1), _) => app.show_help = true,
        (KeyCode::F(2), _) => app.copy_pattern_to_clipboard(),
        (KeyCode::F(3), _) => app.next_match(),
        (KeyCode::F(4), _) => app.prev_match(),

        (KeyCode::Char('h'), _) | (KeyCode::Left,  _) => { app.content_cursor_left();  app.scroll_to_cursor(); }
        (KeyCode::Char('l'), _) | (KeyCode::Right, _) => { app.content_cursor_right(); app.scroll_to_cursor(); }
        (KeyCode::Char('j'), _) | (KeyCode::Down,  _) => { app.content_cursor_down();  app.scroll_to_cursor(); }
        (KeyCode::Char('k'), _) | (KeyCode::Up,    _) => { app.content_cursor_up();    app.scroll_to_cursor(); }
        (KeyCode::Char('w'), _) => app.vim_word_fwd(),
        (KeyCode::Char('b'), _) => app.vim_word_back(),
        (KeyCode::Char('e'), _) => app.vim_word_end(),
        (KeyCode::Char('0'), _) | (KeyCode::Char('^'), _) | (KeyCode::Home, _) => {
            app.content_cursor_home(); app.scroll_to_cursor();
        }
        (KeyCode::Char('$'), _) | (KeyCode::End, _) => {
            app.content_cursor_end(); app.scroll_to_cursor();
        }
        (KeyCode::Char('G'), _) => app.vim_goto_bottom(),
        (KeyCode::Char('g'), _) => { app.vim_pending = Some('g'); }

        (KeyCode::PageUp,   _) => app.page_up(),
        (KeyCode::PageDown, _) => app.page_down(),

        (KeyCode::Char('i'), _) => app.vim_enter_insert(),
        (KeyCode::Char('I'), _) => app.vim_enter_insert_home(),
        (KeyCode::Char('a'), _) => app.vim_enter_insert_after(),
        (KeyCode::Char('A'), _) => app.vim_enter_insert_end(),
        (KeyCode::Char('o'), _) => app.vim_open_below(),
        (KeyCode::Char('O'), _) => app.vim_open_above(),

        (KeyCode::Char('v'), _) => app.vim_enter_visual(),

        (KeyCode::Char('x'), _) => app.vim_delete_char(),
        (KeyCode::Char('d'), _) => { app.vim_pending = Some('d'); }
        (KeyCode::Char('y'), _) => { app.vim_pending = Some('y'); }
        (KeyCode::Char('p'), _) => app.vim_paste_after(),
        (KeyCode::Char('P'), _) => app.vim_paste_before(),
        (KeyCode::Char('u'), _) => app.vim_undo(),

        _ => {}
    }
}

fn handle_vim_insert(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
    match (code, modifiers) {
        (KeyCode::Esc, _) => app.vim_exit_to_normal(),
        (KeyCode::Tab, _) => switch_to_pattern(app),
        (KeyCode::Char('q'), KeyModifiers::CONTROL)
        | (KeyCode::Char('c'), KeyModifiers::CONTROL) => app.quit = true,
        (KeyCode::F(1), _) => app.show_help = true,

        (KeyCode::Enter, _)     => { app.content_newline();   app.scroll_to_cursor(); }
        (KeyCode::Backspace, _) => { app.content_backspace(); app.scroll_to_cursor(); }
        (KeyCode::Delete, _)    => { app.content_delete();    app.scroll_to_cursor(); }

        (KeyCode::Left,  _) => { app.content_cursor_left();  app.scroll_to_cursor(); }
        (KeyCode::Right, _) => { app.content_cursor_right(); app.scroll_to_cursor(); }
        (KeyCode::Up,    _) => { app.content_cursor_up();    app.scroll_to_cursor(); }
        (KeyCode::Down,  _) => { app.content_cursor_down();  app.scroll_to_cursor(); }
        (KeyCode::Home,  _) => { app.content_cursor_home();  app.scroll_to_cursor(); }
        (KeyCode::End,   _) => { app.content_cursor_end();   app.scroll_to_cursor(); }

        (KeyCode::PageUp,   _) => app.page_up(),
        (KeyCode::PageDown, _) => app.page_down(),

        (KeyCode::Char(c), mods)
            if mods == KeyModifiers::NONE || mods == KeyModifiers::SHIFT =>
        {
            app.content_insert(c);
            app.scroll_to_cursor();
        }

        _ => {}
    }
}

fn handle_vim_visual(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
    if let Some(pending) = app.vim_pending.take() {
        if pending == 'g' && code == KeyCode::Char('g') {
            app.vim_goto_top();
            return;
        }
    }

    match (code, modifiers) {
        (KeyCode::Esc, _) => app.vim_exit_to_normal(),
        (KeyCode::Tab, _) => switch_to_pattern(app),
        (KeyCode::Char('q'), KeyModifiers::CONTROL)
        | (KeyCode::Char('c'), KeyModifiers::CONTROL) => app.quit = true,
        (KeyCode::F(1), _) => app.show_help = true,

        (KeyCode::Char('h'), _) | (KeyCode::Left,  _) => { app.content_cursor_left();  app.scroll_to_cursor(); }
        (KeyCode::Char('l'), _) | (KeyCode::Right, _) => { app.content_cursor_right(); app.scroll_to_cursor(); }
        (KeyCode::Char('j'), _) | (KeyCode::Down,  _) => { app.content_cursor_down();  app.scroll_to_cursor(); }
        (KeyCode::Char('k'), _) | (KeyCode::Up,    _) => { app.content_cursor_up();    app.scroll_to_cursor(); }
        (KeyCode::Char('w'), _) => app.vim_word_fwd(),
        (KeyCode::Char('b'), _) => app.vim_word_back(),
        (KeyCode::Char('e'), _) => app.vim_word_end(),
        (KeyCode::Char('0'), _) | (KeyCode::Char('^'), _) | (KeyCode::Home, _) => {
            app.content_cursor_home(); app.scroll_to_cursor();
        }
        (KeyCode::Char('$'), _) | (KeyCode::End, _) => {
            app.content_cursor_end(); app.scroll_to_cursor();
        }
        (KeyCode::Char('G'), _) => app.vim_goto_bottom(),
        (KeyCode::Char('g'), _) => { app.vim_pending = Some('g'); }

        (KeyCode::Char('d'), _) | (KeyCode::Char('x'), _) => app.vim_delete_selection(),
        (KeyCode::Char('y'), _) => app.vim_yank_selection(),
        (KeyCode::Char('c'), _) => app.vim_change_selection(),

        _ => {}
    }
}

fn handle_content_nav(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
    match (code, modifiers) {
        (KeyCode::Char('q'), _) => app.quit = true,
        (KeyCode::Tab, _) | (KeyCode::Enter, _) => app.focus = Focus::Pattern,
        (KeyCode::Char('/'), _) => app.focus = Focus::Pattern,
        (KeyCode::F(1), _) => app.show_help = true,
        (KeyCode::F(2), _) => app.copy_pattern_to_clipboard(),

        (KeyCode::Up,   _) | (KeyCode::Char('k'), _) => app.scroll_up(1),
        (KeyCode::Down, _) | (KeyCode::Char('j'), _) => app.scroll_down(1),
        (KeyCode::PageUp,   _) | (KeyCode::Char('b'), _) => app.page_up(),
        (KeyCode::PageDown, _) | (KeyCode::Char('f'), _) => app.page_down(),
        (KeyCode::Char('g'), _) | (KeyCode::Home, _) => app.scroll_top(),
        (KeyCode::Char('G'), _) | (KeyCode::End,  _) => app.scroll_bottom(),

        (KeyCode::Char('n'), _) => app.next_match(),
        (KeyCode::Char('N'), _) => app.prev_match(),

        _ => {}
    }
}
