use std::io::Write;
use std::ops::Range;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use core::{CompileError, CompiledPattern, Flags, Match, compile, run_matches};

use crate::byte_display::{DisplayLine, build_display_lines};

const DEBOUNCE_MS: u128 = 150;

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

    // --- Misc ---
    pub show_help:    bool,
    pub quit:         bool,
    pub notification: Option<(String, Instant)>,
}

impl App {
    pub fn new(buf: Vec<u8>, filename: String) -> Self {
        let display_lines = build_display_lines(&buf);
        App {
            buf,
            filename,
            pattern:         String::new(),
            cursor_col:      0,
            focus:           Focus::Pattern,
            compiled:        None,
            compile_error:   None,
            matches:         Vec::new(),
            match_index:     0,
            display_lines,
            scroll_row:      0,
            viewport_height: 0,
            pattern_dirty:   false,
            last_key_time:   None,
            show_help:       false,
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
