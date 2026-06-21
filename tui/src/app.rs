use std::io::Write;
use std::ops::Range;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use core::{CompileError, CompiledPattern, Flags, Match, compile, run_matches};

use crate::byte_display::{DisplayLine, build_display_lines};

const DEBOUNCE_MS: u128 = 150;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VimMode { Normal, Insert, Visual }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Pattern,
    Content,
}

pub struct App {
    // --- Input ---
    pub buf:      Vec<u8>,
    pub filename: String,

    // --- Pattern editing ---
    pub pattern:       String,
    pub cursor_col:    usize, // byte index into pattern
    pub focus:         Focus,

    // --- Match state ---
    pub compiled:      Option<CompiledPattern>,
    pub compile_error: Option<CompileError>,
    pub matches:       Vec<Match>,
    pub match_index:   usize, // currently selected match (0-based)

    // --- Display ---
    pub display_lines:   Vec<DisplayLine>,
    pub scroll_row:      usize, // first visible line index
    pub viewport_height: u16,   // set by render each frame

    // --- Debounce ---
    pattern_dirty: bool,
    last_key_time: Option<Instant>,

    // --- Content editing ---
    pub editable:       bool,     // true when launched with no file (scratch mode)
    pub content_cursor: usize,    // byte offset into buf for the content cursor
    pub vim_mode:       VimMode,  // current vim mode (only meaningful when editable)
    pub visual_anchor:  usize,    // byte offset where Visual mode was entered
    pub vim_pending:    Option<char>, // first key of a two-key sequence (dd / yy / gg)
    pub yank_buf:       Vec<u8>,  // unnamed yank register
    undo_stack:         Vec<(Vec<u8>, usize)>, // (buf snapshot, cursor); capped at 100
    pattern_undo_stack: Vec<(String, usize)>,  // (pattern snapshot, cursor_col); capped at 50
    pub pattern_visual_anchor: usize,          // byte offset into pattern for Visual anchor

    // --- Misc ---
    pub show_help:    bool,
    pub quit:         bool,
    pub notification: Option<(String, Instant)>,
}

impl App {
    pub fn new(buf: Vec<u8>, filename: String, editable: bool) -> Self {
        let display_lines = build_display_lines(&buf);
        let focus = if editable { Focus::Content } else { Focus::Pattern };
        App {
            buf,
            filename,
            pattern:         String::new(),
            cursor_col:      0,
            focus,
            compiled:        None,
            compile_error:   None,
            matches:         Vec::new(),
            match_index:     0,
            display_lines,
            scroll_row:      0,
            viewport_height: 0,
            pattern_dirty:   false,
            last_key_time:   None,
            editable,
            content_cursor:  0,
            vim_mode:        VimMode::Normal,
            visual_anchor:   0,
            vim_pending:     None,
            yank_buf:              Vec::new(),
            undo_stack:            Vec::new(),
            pattern_undo_stack:    Vec::new(),
            pattern_visual_anchor: 0,
            show_help:             false,
            quit:            false,
            notification:    None,
        }
    }

    // --- Pattern editing ---

    pub fn pattern_insert(&mut self, ch: char) {
        let byte_idx = self.cursor_col;
        self.pattern.insert(byte_idx, ch);
        self.cursor_col += ch.len_utf8();
        self.mark_dirty();
    }

    pub fn pattern_backspace(&mut self) {
        if self.cursor_col == 0 {
            return;
        }
        let end = self.cursor_col;
        let ch = self.pattern[..end].chars().next_back().unwrap();
        let start = end - ch.len_utf8();
        self.pattern.drain(start..end);
        self.cursor_col = start;
        self.mark_dirty();
    }

    pub fn pattern_cursor_left(&mut self) {
        if self.cursor_col > 0 {
            let ch = self.pattern[..self.cursor_col].chars().next_back().unwrap();
            self.cursor_col -= ch.len_utf8();
        }
    }

    pub fn pattern_cursor_right(&mut self) {
        if self.cursor_col < self.pattern.len() {
            let ch = self.pattern[self.cursor_col..].chars().next().unwrap();
            self.cursor_col += ch.len_utf8();
        }
    }

    pub fn pattern_cursor_home(&mut self) { self.cursor_col = 0; }
    pub fn pattern_cursor_end(&mut self)  { self.cursor_col = self.pattern.len(); }

    // --- Pattern vim editing ---

    pub(crate) fn pattern_push_undo(&mut self) {
        if self.pattern_undo_stack.len() >= 50 {
            self.pattern_undo_stack.remove(0);
        }
        self.pattern_undo_stack.push((self.pattern.clone(), self.cursor_col));
    }

    pub fn pattern_vim_undo(&mut self) {
        if let Some((pat, col)) = self.pattern_undo_stack.pop() {
            self.pattern = pat;
            self.cursor_col = col.min(self.pattern.len());
            self.vim_mode = VimMode::Normal;
            self.mark_dirty();
        } else {
            self.notification = Some(("Already at oldest change".into(), Instant::now()));
        }
    }

    pub fn pattern_vim_enter_insert(&mut self) {
        self.pattern_push_undo();
        self.vim_mode = VimMode::Insert;
    }

    pub fn pattern_vim_enter_insert_home(&mut self) {
        self.pattern_push_undo();
        self.cursor_col = 0;
        self.vim_mode = VimMode::Insert;
    }

    pub fn pattern_vim_enter_insert_after(&mut self) {
        self.pattern_push_undo();
        self.pattern_cursor_right();
        self.vim_mode = VimMode::Insert;
    }

    pub fn pattern_vim_enter_insert_end(&mut self) {
        self.pattern_push_undo();
        self.cursor_col = self.pattern.len();
        self.vim_mode = VimMode::Insert;
    }

    pub fn pattern_vim_enter_visual(&mut self) {
        self.pattern_visual_anchor = self.cursor_col;
        self.vim_mode = VimMode::Visual;
    }

    pub fn pattern_vim_delete_char(&mut self) {
        if self.cursor_col >= self.pattern.len() { return; }
        self.pattern_push_undo();
        let ch = self.pattern[self.cursor_col..].chars().next().unwrap();
        let end = self.cursor_col + ch.len_utf8();
        self.pattern.drain(self.cursor_col..end);
        self.cursor_col = self.cursor_col.min(self.pattern.len());
        self.mark_dirty();
    }

    pub fn pattern_vim_delete_to_end(&mut self) {
        if self.cursor_col >= self.pattern.len() { return; }
        self.pattern_push_undo();
        self.yank_buf = self.pattern[self.cursor_col..].as_bytes().to_vec();
        self.pattern.truncate(self.cursor_col);
        self.mark_dirty();
    }

    pub fn pattern_vim_delete_all(&mut self) {
        if self.pattern.is_empty() { return; }
        self.pattern_push_undo();
        self.yank_buf = self.pattern.as_bytes().to_vec();
        self.pattern.clear();
        self.cursor_col = 0;
        self.mark_dirty();
    }

    pub fn pattern_vim_change_all(&mut self) {
        self.pattern_push_undo();
        self.yank_buf = self.pattern.as_bytes().to_vec();
        self.pattern.clear();
        self.cursor_col = 0;
        self.vim_mode = VimMode::Insert;
        self.mark_dirty();
    }

    pub fn pattern_vim_yank_all(&mut self) {
        self.yank_buf = self.pattern.as_bytes().to_vec();
        self.notification = Some(("Pattern yanked".into(), Instant::now()));
    }

    pub fn pattern_vim_paste_after(&mut self) {
        if self.yank_buf.is_empty() { return; }
        let Ok(s) = std::str::from_utf8(&self.yank_buf) else { return };
        let s = s.to_owned();
        self.pattern_push_undo();
        let pos = if self.pattern.is_empty() { 0 } else {
            let ch = self.pattern[self.cursor_col..].chars().next();
            self.cursor_col + ch.map(|c| c.len_utf8()).unwrap_or(0)
        };
        self.pattern.insert_str(pos, &s);
        self.cursor_col = pos;
        self.mark_dirty();
    }

    pub fn pattern_vim_paste_before(&mut self) {
        if self.yank_buf.is_empty() { return; }
        let Ok(s) = std::str::from_utf8(&self.yank_buf) else { return };
        let s = s.to_owned();
        self.pattern_push_undo();
        self.pattern.insert_str(self.cursor_col, &s);
        self.mark_dirty();
    }

    pub fn pattern_vim_word_fwd(&mut self) {
        let bytes = self.pattern.as_bytes();
        let len = bytes.len();
        let mut pos = self.cursor_col;
        if pos >= len { return; }
        if is_word(bytes[pos]) {
            while pos < len && is_word(bytes[pos]) { pos += 1; }
        } else if !is_ws(bytes[pos]) {
            while pos < len && !is_word(bytes[pos]) && !is_ws(bytes[pos]) { pos += 1; }
        }
        while pos < len && is_ws(bytes[pos]) { pos += 1; }
        self.cursor_col = pos;
    }

    pub fn pattern_vim_word_back(&mut self) {
        if self.cursor_col == 0 { return; }
        let bytes = self.pattern.as_bytes();
        let mut pos = self.cursor_col.saturating_sub(1);
        while pos > 0 && is_ws(bytes[pos]) { pos -= 1; }
        if is_word(bytes[pos]) {
            while pos > 0 && is_word(bytes[pos - 1]) { pos -= 1; }
        } else {
            while pos > 0 && !is_word(bytes[pos - 1]) && !is_ws(bytes[pos - 1]) { pos -= 1; }
        }
        self.cursor_col = pos;
    }

    pub fn pattern_vim_word_end(&mut self) {
        let bytes = self.pattern.as_bytes();
        let len = bytes.len();
        if self.cursor_col + 1 >= len { return; }
        let mut pos = self.cursor_col + 1;
        while pos < len && is_ws(bytes[pos]) { pos += 1; }
        if pos >= len { self.cursor_col = len - 1; return; }
        if is_word(bytes[pos]) {
            while pos + 1 < len && is_word(bytes[pos + 1]) { pos += 1; }
        } else {
            while pos + 1 < len && !is_word(bytes[pos + 1]) && !is_ws(bytes[pos + 1]) { pos += 1; }
        }
        self.cursor_col = pos;
    }

    pub fn pattern_visual_range(&self) -> Range<usize> {
        let lo = self.pattern_visual_anchor.min(self.cursor_col);
        let hi = self.pattern_visual_anchor.max(self.cursor_col);
        lo..hi.saturating_add(1).min(self.pattern.len())
    }

    pub fn pattern_vim_delete_selection(&mut self) {
        let range = self.pattern_visual_range();
        if range.is_empty() { return; }
        self.pattern_push_undo();
        self.yank_buf = self.pattern[range.clone()].as_bytes().to_vec();
        self.pattern.drain(range.clone());
        self.cursor_col = range.start.min(self.pattern.len());
        self.vim_mode = VimMode::Normal;
        self.mark_dirty();
    }

    pub fn pattern_vim_yank_selection(&mut self) {
        let range = self.pattern_visual_range();
        if range.is_empty() { return; }
        self.yank_buf = self.pattern[range.clone()].as_bytes().to_vec();
        self.cursor_col = range.start;
        self.vim_mode = VimMode::Normal;
        self.notification = Some(("Selection yanked".into(), Instant::now()));
    }

    pub fn pattern_vim_change_selection(&mut self) {
        let range = self.pattern_visual_range();
        if range.is_empty() { return; }
        self.pattern_push_undo();
        self.yank_buf = self.pattern[range.clone()].as_bytes().to_vec();
        self.pattern.drain(range.clone());
        self.cursor_col = range.start.min(self.pattern.len());
        self.vim_mode = VimMode::Insert;
        self.mark_dirty();
    }

    // --- Content editing (scratch mode only) ---

    // --- Vim mode transitions ---

    pub fn vim_enter_insert(&mut self) {
        self.push_undo();
        self.vim_mode = VimMode::Insert;
    }

    pub fn vim_enter_insert_home(&mut self) {
        self.push_undo();
        self.content_cursor_home();
        self.vim_mode = VimMode::Insert;
    }

    pub fn vim_enter_insert_after(&mut self) {
        self.push_undo();
        if self.content_cursor < self.buf.len() { self.content_cursor += 1; }
        self.vim_mode = VimMode::Insert;
    }

    pub fn vim_enter_insert_end(&mut self) {
        self.push_undo();
        self.content_cursor_end();
        self.vim_mode = VimMode::Insert;
    }

    pub fn vim_enter_visual(&mut self) {
        self.vim_mode = VimMode::Visual;
        self.visual_anchor = self.content_cursor;
    }

    pub fn vim_exit_to_normal(&mut self) {
        self.vim_mode = VimMode::Normal;
        self.vim_pending = None;
    }

    // --- Undo ---

    pub(crate) fn push_undo(&mut self) {
        if self.undo_stack.len() >= 100 {
            self.undo_stack.remove(0);
        }
        self.undo_stack.push((self.buf.clone(), self.content_cursor));
    }

    pub fn vim_undo(&mut self) {
        if let Some((buf, cursor)) = self.undo_stack.pop() {
            self.buf = buf;
            self.content_cursor = cursor;
            self.vim_mode = VimMode::Normal;
            self.rebuild_and_recompute();
        } else {
            self.notification = Some(("Already at oldest change".into(), Instant::now()));
        }
    }

    // --- Word motions ---

    pub fn vim_word_fwd(&mut self) {
        let len = self.buf.len();
        let mut pos = self.content_cursor;
        if pos >= len { return; }
        if is_word(self.buf[pos]) {
            while pos < len && is_word(self.buf[pos]) { pos += 1; }
        } else if !is_ws(self.buf[pos]) {
            while pos < len && !is_word(self.buf[pos]) && !is_ws(self.buf[pos]) { pos += 1; }
        }
        while pos < len && is_ws(self.buf[pos]) { pos += 1; }
        self.content_cursor = pos;
        self.scroll_to_cursor();
    }

    pub fn vim_word_back(&mut self) {
        if self.content_cursor == 0 { return; }
        let buf = &self.buf;
        let mut pos = self.content_cursor.saturating_sub(1);
        while pos > 0 && is_ws(buf[pos]) { pos -= 1; }
        if is_word(buf[pos]) {
            while pos > 0 && is_word(buf[pos - 1]) { pos -= 1; }
        } else {
            while pos > 0 && !is_word(buf[pos - 1]) && !is_ws(buf[pos - 1]) { pos -= 1; }
        }
        self.content_cursor = pos;
        self.scroll_to_cursor();
    }

    pub fn vim_word_end(&mut self) {
        let len = self.buf.len();
        if self.content_cursor + 1 >= len { return; }
        let buf = &self.buf;
        let mut pos = self.content_cursor + 1;
        while pos < len && is_ws(buf[pos]) { pos += 1; }
        if pos >= len { self.content_cursor = len - 1; return; }
        if is_word(buf[pos]) {
            while pos + 1 < len && is_word(buf[pos + 1]) { pos += 1; }
        } else {
            while pos + 1 < len && !is_word(buf[pos + 1]) && !is_ws(buf[pos + 1]) { pos += 1; }
        }
        self.content_cursor = pos;
        self.scroll_to_cursor();
    }

    // --- Vim navigation ---

    pub fn vim_goto_top(&mut self) {
        self.content_cursor = 0;
        self.scroll_to_cursor();
    }

    pub fn vim_goto_bottom(&mut self) {
        if let Some(last) = self.display_lines.last() {
            self.content_cursor = last.byte_start;
        }
        self.scroll_to_cursor();
    }

    // --- Vim edit operations ---

    pub fn vim_delete_char(&mut self) {
        let len = self.buf.len();
        if len == 0 || self.content_cursor >= len { return; }
        self.push_undo();
        self.buf.remove(self.content_cursor);
        self.content_cursor = self.content_cursor.min(self.buf.len().saturating_sub(1));
        self.rebuild_and_recompute();
    }

    pub fn vim_delete_line(&mut self) {
        if self.buf.is_empty() { return; }
        let (line_idx, _) = self.cursor_line_col();
        let line_start = self.display_lines[line_idx].byte_start;
        self.push_undo();
        if line_idx + 1 < self.display_lines.len() {
            let next_start = self.display_lines[line_idx + 1].byte_start;
            self.yank_buf = self.buf[line_start..next_start].to_vec();
            self.buf.drain(line_start..next_start);
            self.content_cursor = line_start.min(self.buf.len());
        } else if line_start > 0 {
            // Last line with lines above: remove preceding \n too.
            self.yank_buf = self.buf[line_start..].to_vec();
            self.yank_buf.push(b'\n');
            self.buf.drain(line_start - 1..);
            self.content_cursor = (line_start - 1).min(self.buf.len());
        } else {
            // Only line.
            self.yank_buf = self.buf.clone();
            self.yank_buf.push(b'\n');
            self.buf.clear();
            self.content_cursor = 0;
        }
        self.rebuild_and_recompute();
    }

    pub fn vim_yank_line(&mut self) {
        let (line_idx, _) = self.cursor_line_col();
        let line_start = self.display_lines[line_idx].byte_start;
        if line_idx + 1 < self.display_lines.len() {
            let next_start = self.display_lines[line_idx + 1].byte_start;
            self.yank_buf = self.buf[line_start..next_start].to_vec();
        } else {
            self.yank_buf = self.buf[line_start..].to_vec();
            self.yank_buf.push(b'\n');
        }
        self.notification = Some(("Line yanked".into(), Instant::now()));
    }

    pub fn vim_paste_after(&mut self) {
        if self.yank_buf.is_empty() { return; }
        self.push_undo();
        let yb = self.yank_buf.clone();
        if yb.last() == Some(&b'\n') {
            let (line_idx, _) = self.cursor_line_col();
            let insert_pos = if line_idx + 1 < self.display_lines.len() {
                self.display_lines[line_idx + 1].byte_start
            } else {
                self.buf.len()
            };
            // If pasting at end and buffer doesn't end with \n, prepend one.
            let need_nl = insert_pos == self.buf.len()
                && !self.buf.is_empty()
                && self.buf.last() != Some(&b'\n');
            if need_nl {
                self.buf.push(b'\n');
                self.buf.extend_from_slice(&yb);
                self.content_cursor = insert_pos + 1;
            } else {
                self.buf.splice(insert_pos..insert_pos, yb.iter().copied());
                self.content_cursor = insert_pos;
            }
        } else {
            let pos = if self.buf.is_empty() { 0 } else { (self.content_cursor + 1).min(self.buf.len()) };
            self.buf.splice(pos..pos, yb.iter().copied());
            self.content_cursor = pos;
        }
        self.rebuild_and_recompute();
    }

    pub fn vim_paste_before(&mut self) {
        if self.yank_buf.is_empty() { return; }
        self.push_undo();
        let yb = self.yank_buf.clone();
        if yb.last() == Some(&b'\n') {
            let (line_idx, _) = self.cursor_line_col();
            let insert_pos = self.display_lines[line_idx].byte_start;
            self.buf.splice(insert_pos..insert_pos, yb.iter().copied());
            self.content_cursor = insert_pos;
        } else {
            let pos = self.content_cursor;
            self.buf.splice(pos..pos, yb.iter().copied());
            self.content_cursor = pos;
        }
        self.rebuild_and_recompute();
    }

    pub fn vim_open_below(&mut self) {
        self.push_undo();
        let (line_idx, _) = self.cursor_line_col();
        let insert_pos = if line_idx + 1 < self.display_lines.len() {
            self.display_lines[line_idx + 1].byte_start
        } else {
            self.buf.len()
        };
        let at_end = insert_pos == self.buf.len();
        self.buf.insert(insert_pos, b'\n');
        self.content_cursor = if at_end { insert_pos + 1 } else { insert_pos };
        self.vim_mode = VimMode::Insert;
        self.rebuild_and_recompute();
    }

    pub fn vim_open_above(&mut self) {
        self.push_undo();
        let (line_idx, _) = self.cursor_line_col();
        let insert_pos = self.display_lines[line_idx].byte_start;
        self.buf.insert(insert_pos, b'\n');
        self.content_cursor = insert_pos;
        self.vim_mode = VimMode::Insert;
        self.rebuild_and_recompute();
    }

    // --- Visual mode helpers ---

    pub fn visual_range(&self) -> std::ops::Range<usize> {
        let lo = self.visual_anchor.min(self.content_cursor);
        let hi = self.visual_anchor.max(self.content_cursor);
        lo..hi.saturating_add(1).min(self.buf.len())
    }

    pub fn vim_delete_selection(&mut self) {
        let range = self.visual_range();
        if range.is_empty() { return; }
        self.push_undo();
        self.yank_buf = self.buf[range.clone()].to_vec();
        self.buf.drain(range.clone());
        self.content_cursor = range.start.min(self.buf.len());
        self.vim_mode = VimMode::Normal;
        self.rebuild_and_recompute();
    }

    pub fn vim_yank_selection(&mut self) {
        let range = self.visual_range();
        if range.is_empty() { return; }
        self.yank_buf = self.buf[range.clone()].to_vec();
        self.content_cursor = range.start;
        self.vim_mode = VimMode::Normal;
        self.scroll_to_cursor();
        self.notification = Some(("Selection yanked".into(), Instant::now()));
    }

    pub fn vim_change_selection(&mut self) {
        let range = self.visual_range();
        if range.is_empty() { return; }
        self.push_undo();
        self.yank_buf = self.buf[range.clone()].to_vec();
        self.buf.drain(range.clone());
        self.content_cursor = range.start.min(self.buf.len());
        self.vim_mode = VimMode::Insert;
        self.rebuild_and_recompute();
    }

    pub fn content_insert_str(&mut self, s: &str) {
        let bytes = normalize_line_endings(s);
        let pos = self.content_cursor;
        self.buf.splice(pos..pos, bytes.iter().copied());
        self.content_cursor += bytes.len();
        self.rebuild_and_recompute();
    }

    pub fn content_insert(&mut self, ch: char) {
        let mut tmp = [0u8; 4];
        let encoded = ch.encode_utf8(&mut tmp);
        let pos = self.content_cursor;
        for (i, &b) in encoded.as_bytes().iter().enumerate() {
            self.buf.insert(pos + i, b);
        }
        self.content_cursor += encoded.len();
        self.rebuild_and_recompute();
    }

    pub fn content_newline(&mut self) {
        let pos = self.content_cursor;
        self.buf.insert(pos, b'\n');
        self.content_cursor = pos + 1;
        self.rebuild_and_recompute();
    }

    pub fn content_backspace(&mut self) {
        if self.content_cursor == 0 { return; }
        self.content_cursor -= 1;
        self.buf.remove(self.content_cursor);
        self.rebuild_and_recompute();
    }

    pub fn content_delete(&mut self) {
        if self.content_cursor < self.buf.len() {
            self.buf.remove(self.content_cursor);
            self.rebuild_and_recompute();
        }
    }

    pub fn content_cursor_left(&mut self) {
        if self.content_cursor > 0 {
            self.content_cursor -= 1;
        }
    }

    pub fn content_cursor_right(&mut self) {
        if self.content_cursor < self.buf.len() {
            self.content_cursor += 1;
        }
    }

    pub fn content_cursor_up(&mut self) {
        let (line_idx, col) = self.cursor_line_col();
        if line_idx == 0 { return; }
        let prev = &self.display_lines[line_idx - 1];
        self.content_cursor = line_col_to_byte(prev, col);
    }

    pub fn content_cursor_down(&mut self) {
        let (line_idx, col) = self.cursor_line_col();
        if line_idx + 1 >= self.display_lines.len() { return; }
        let next = &self.display_lines[line_idx + 1];
        self.content_cursor = line_col_to_byte(next, col);
    }

    pub fn content_cursor_home(&mut self) {
        let (line_idx, _) = self.cursor_line_col();
        self.content_cursor = self.display_lines[line_idx].byte_start;
    }

    pub fn content_cursor_end(&mut self) {
        let (line_idx, _) = self.cursor_line_col();
        self.content_cursor = self.line_end_byte(line_idx);
    }

    /// Returns the (display-line index, display-column) for content_cursor.
    pub fn cursor_line_col(&self) -> (usize, usize) {
        let pos = self.content_cursor;
        let line_idx = self.display_lines
            .partition_point(|dl| dl.byte_start <= pos)
            .saturating_sub(1);
        let line_idx = line_idx.min(self.display_lines.len().saturating_sub(1));
        let dl = &self.display_lines[line_idx];
        let col = dl.col_to_byte.partition_point(|&b| b < pos);
        (line_idx, col)
    }

    /// Scroll the viewport so content_cursor is visible.
    pub fn scroll_to_cursor(&mut self) {
        let (line_idx, _) = self.cursor_line_col();
        let vh = self.viewport_height as usize;
        if vh == 0 { return; }
        if line_idx < self.scroll_row {
            self.scroll_row = line_idx;
        } else if line_idx >= self.scroll_row + vh {
            self.scroll_row = line_idx.saturating_sub(vh - 1);
        }
    }

    fn line_end_byte(&self, line_idx: usize) -> usize {
        if line_idx + 1 < self.display_lines.len() {
            // The \n separator sits just before the next line's byte_start.
            self.display_lines[line_idx + 1].byte_start.saturating_sub(1)
        } else {
            self.buf.len()
        }
    }

    fn rebuild_and_recompute(&mut self) {
        self.display_lines = build_display_lines(&self.buf);
        self.content_cursor = self.content_cursor.min(self.buf.len());
        self.recompute();
        self.scroll_to_cursor();
    }

    fn mark_dirty(&mut self) {
        self.pattern_dirty = true;
        self.last_key_time = Some(Instant::now());
    }

    // --- Debounce tick ---

    /// Called every event-loop iteration. Triggers recompile when debounce timer expires.
    pub fn tick(&mut self) {
        // Expire notification after 2 s.
        if let Some((_, ts)) = self.notification {
            if ts.elapsed() >= Duration::from_secs(2) {
                self.notification = None;
            }
        }

        if !self.pattern_dirty {
            return;
        }
        let elapsed = self.last_key_time
            .map(|t| t.elapsed().as_millis())
            .unwrap_or(u128::MAX);
        if elapsed >= DEBOUNCE_MS {
            self.recompute();
        }
    }

    pub fn copy_pattern_to_clipboard(&mut self) {
        if self.pattern.is_empty() {
            self.notification = Some(("Nothing to copy — pattern is empty".into(), Instant::now()));
            return;
        }
        let msg = match clipboard_write(self.pattern.as_bytes()) {
            Ok(())  => format!("Copied: {}", self.pattern),
            Err(e)  => format!("Clipboard error: {e}"),
        };
        self.notification = Some((msg, Instant::now()));
    }

    fn recompute(&mut self) {
        self.pattern_dirty = false;

        if self.pattern.is_empty() {
            self.compiled      = None;
            self.compile_error = None;
            self.matches       = Vec::new();
            self.match_index   = 0;
            return;
        }

        // Flags are set via inline PCRE2 modifiers in the pattern (e.g. (?i), (?ms)).
        match compile(&self.pattern, Flags::default()) {
            Ok(mut cp) => {
                self.matches       = run_matches(&mut cp, &self.buf);
                self.compiled      = Some(cp);
                self.compile_error = None;
                self.match_index   = 0;
            }
            Err(e) => {
                self.compiled      = None;
                self.compile_error = Some(e);
                self.matches       = Vec::new();
                self.match_index   = 0;
            }
        }
    }

    // --- Match navigation ---

    pub fn next_match(&mut self) {
        if !self.matches.is_empty() {
            self.match_index = (self.match_index + 1) % self.matches.len();
            self.scroll_to_current_match();
        }
    }

    pub fn prev_match(&mut self) {
        if !self.matches.is_empty() {
            self.match_index = self.match_index
                .checked_sub(1)
                .unwrap_or(self.matches.len() - 1);
            self.scroll_to_current_match();
        }
    }

    fn scroll_to_current_match(&mut self) {
        let Some(m) = self.matches.get(self.match_index) else { return };
        let target_byte = m.range.start;
        let line_idx = self.display_lines.iter().position(|dl| {
            dl.byte_start <= target_byte
        }).unwrap_or(0);
        let vh = self.viewport_height as usize;
        if line_idx < self.scroll_row || line_idx >= self.scroll_row + vh {
            self.scroll_row = line_idx.saturating_sub(vh / 3);
        }
    }

    // --- Scrolling ---

    pub fn scroll_up(&mut self, n: usize) { self.scroll_row = self.scroll_row.saturating_sub(n); }
    pub fn scroll_down(&mut self, n: usize) {
        let max = self.display_lines.len().saturating_sub(1);
        self.scroll_row = (self.scroll_row + n).min(max);
    }
    pub fn scroll_top(&mut self)    { self.scroll_row = 0; }
    pub fn scroll_bottom(&mut self) { self.scroll_row = self.display_lines.len().saturating_sub(1); }

    pub fn page_up(&mut self)   { let n = self.viewport_height as usize; self.scroll_up(n); }
    pub fn page_down(&mut self) { let n = self.viewport_height as usize; self.scroll_down(n); }

    // --- Match info helpers ---

    pub fn current_match(&self) -> Option<&Match> {
        self.matches.get(self.match_index)
    }

    /// Collect all match byte ranges overlapping the given line byte range [line_start, line_end).
    pub fn matches_for_line(&self, line_start: usize, line_end: usize)
        -> Vec<(Range<usize>, bool)> // (range, is_capture)
    {
        let mut out = Vec::new();
        for m in &self.matches {
            if m.range.end > line_start && m.range.start < line_end {
                out.push((m.range.clone(), false));
            }
            for cap in &m.captures {
                if cap.range.end > line_start && cap.range.start < line_end {
                    out.push((cap.range.clone(), true));
                }
            }
        }
        out
    }
}

fn is_word(b: u8) -> bool { b.is_ascii_alphanumeric() || b == b'_' }
fn is_ws(b: u8) -> bool   { b == b' ' || b == b'\t' || b == b'\n' }

/// Normalize line endings to `\n`: converts `\r\n` (Windows) and bare `\r` (old Mac) to `\n`.
fn normalize_line_endings(s: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len());
    let mut prev_cr = false;
    for b in s.bytes() {
        match b {
            b'\r' => { out.push(b'\n'); prev_cr = true; }
            b'\n' => { if !prev_cr { out.push(b'\n'); } prev_cr = false; }
            _     => { out.push(b);    prev_cr = false; }
        }
    }
    out
}

/// Given a display line and a target column, return the buffer byte offset
/// that corresponds to that column (clamped to end-of-line if column is past it).
fn line_col_to_byte(dl: &DisplayLine, col: usize) -> usize {
    if col < dl.col_to_byte.len() {
        dl.col_to_byte[col]
    } else if let Some(&last) = dl.col_to_byte.last() {
        last + 1 // position after last visible char (at or past the \n)
    } else {
        dl.byte_start // empty line
    }
}

/// Write `data` to the system clipboard.
/// Tries wl-copy (Wayland), then xclip, then xsel (X11).
fn clipboard_write(data: &[u8]) -> Result<(), String> {
    let candidates: &[(&str, &[&str])] = &[
        ("wl-copy",  &[]),
        ("pbcopy",   &[]),
        ("xclip",    &["-selection", "clipboard"]),
        ("xsel",     &["--clipboard", "--input"]),
    ];
    for (cmd, args) in candidates {
        let Ok(mut child) = Command::new(cmd)
            .args(*args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        else {
            continue; // binary not found
        };
        if let Some(stdin) = child.stdin.take() {
            let mut stdin = stdin;
            let _ = stdin.write_all(data);
        }
        return match child.wait() {
            Ok(s) if s.success() => Ok(()),
            Ok(s)  => Err(format!("{cmd} exited with {s}")),
            Err(e) => Err(format!("{cmd}: {e}")),
        };
    }
    Err("No clipboard utility found (install wl-clipboard, xclip, or xsel)".into())
}
