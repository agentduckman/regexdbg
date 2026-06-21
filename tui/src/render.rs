use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

use crate::app::{App, Focus, VimMode};
use crate::byte_display::byte_range_to_col_range;

// Colour scheme
const STYLE_MATCH:    Style = Style::new().bg(Color::Yellow).fg(Color::Black);
const STYLE_CAPTURE:  Style = Style::new().bg(Color::Cyan).fg(Color::Black);
const STYLE_SELECTED: Style = Style::new().bg(Color::Green).fg(Color::Black).add_modifier(Modifier::BOLD);
const STYLE_VISUAL:   Style = Style::new().bg(Color::Blue).fg(Color::White);
const STYLE_ERROR:    Style = Style::new().fg(Color::Red);
const STYLE_DIM:      Style = Style::new().fg(Color::DarkGray);
const STYLE_BOLD:     Style = Style::new().add_modifier(Modifier::BOLD);

pub fn draw(frame: &mut Frame, app: &mut App) {
    let area = frame.area();

    // ┌ outer vertical split ──────────────────────┐
    // │ pattern input (3 lines)                    │
    // │ flags bar    (1 line)                      │
    // │ content pane (fill)                        │
    // │ match-info   (5 lines)                     │
    // │ status line  (1 line)                      │
    // └────────────────────────────────────────────┘
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // pattern
            Constraint::Length(1), // flags
            Constraint::Min(1),    // content
            Constraint::Length(5), // match-info
            Constraint::Length(1), // status
        ])
        .split(area);

    app.viewport_height = chunks[2].height;

    draw_pattern(frame, app, chunks[0]);
    draw_flags(frame, app, chunks[1]);
    draw_content(frame, app, chunks[2]);
    draw_match_info(frame, app, chunks[3]);
    draw_status(frame, app, chunks[4]);

    if app.show_help {
        draw_help(frame, area);
    }
}

fn draw_pattern(frame: &mut Frame, app: &App, area: Rect) {
    let border_style = if app.focus == Focus::Pattern {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title("Pattern  (Tab = content  i = insert  F1 = help)")
        .border_style(border_style);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Build styled line: highlight visual selection if active.
    let paragraph = if app.focus == Focus::Pattern && app.vim_mode == VimMode::Visual {
        let range = app.pattern_visual_range();
        let before = &app.pattern[..range.start];
        let selected = &app.pattern[range.clone()];
        let after   = &app.pattern[range.end..];
        let line = Line::from(vec![
            Span::raw(before.to_owned()),
            Span::styled(selected.to_owned(), STYLE_VISUAL),
            Span::raw(after.to_owned()),
        ]);
        Paragraph::new(line)
    } else {
        Paragraph::new(app.pattern.as_str())
    };
    frame.render_widget(paragraph, inner);

    // Place cursor
    if app.focus == Focus::Pattern {
        let display_col = app.pattern[..app.cursor_col].chars().count() as u16;
        frame.set_cursor_position((inner.x + display_col, inner.y));
    }
}

fn draw_flags(frame: &mut Frame, _app: &App, area: Rect) {
    let line = Line::from(vec![
        Span::styled(" Modifiers: ", STYLE_DIM),
        Span::styled("(?i)", STYLE_BOLD), Span::styled(" caseless  ", STYLE_DIM),
        Span::styled("(?m)", STYLE_BOLD), Span::styled(" multiline  ", STYLE_DIM),
        Span::styled("(?s)", STYLE_BOLD), Span::styled(" dotall  ", STYLE_DIM),
        Span::styled("(?x)", STYLE_BOLD), Span::styled(" extended  ", STYLE_DIM),
        Span::styled("(?U)", STYLE_BOLD), Span::styled(" ungreedy  ", STYLE_DIM),
        Span::styled("(?u)", STYLE_BOLD), Span::styled(" utf+ucp", STYLE_DIM),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

fn draw_content(frame: &mut Frame, app: &App, area: Rect) {
    let focused = app.editable && app.focus == Focus::Content;
    let border_style = if focused {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };
    let title = if app.editable {
        format!("Content  [scratch · {} bytes]", app.buf.len())
    } else {
        format!("Content  [{} bytes]", app.buf.len())
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(border_style);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let height = inner.height as usize;
    let start  = app.scroll_row;
    let end    = (start + height).min(app.display_lines.len());

    let current_match_range = app.current_match().map(|m| m.range.clone());

    let mut lines: Vec<Line> = Vec::with_capacity(height);

    for line_idx in start..end {
        let dl = &app.display_lines[line_idx];

        // Byte range for this display line.
        let line_byte_end = if line_idx + 1 < app.display_lines.len() {
            // next line's byte_start is after our '\n', so it marks our exclusive end+1.
            // We need byte_end to be the '\n' position (not included in col_to_byte).
            // The next line's byte_start is dl.byte_start + col_to_byte.len() + 1 (the \n).
            app.display_lines[line_idx + 1].byte_start
        } else {
            app.buf.len() + 1
        };

        // Collect highlight spans: (col_start, col_end, style).
        // Visual selection goes first (lowest priority — match highlights overdraw it).
        let mut spans_info: Vec<(usize, usize, Style)> = Vec::new();

        if app.vim_mode == VimMode::Visual {
            let vr = app.visual_range();
            if let Some((cs, ce)) = byte_range_to_col_range(dl, line_byte_end, vr.start, vr.end) {
                spans_info.push((cs, ce, STYLE_VISUAL));
            }
        }

        for (byte_range, is_capture) in app.matches_for_line(dl.byte_start, line_byte_end) {
            if let Some((cs, ce)) = byte_range_to_col_range(dl, line_byte_end, byte_range.start, byte_range.end) {
                let style = if is_capture {
                    STYLE_CAPTURE
                } else if current_match_range.as_ref().map(|r| r == &byte_range).unwrap_or(false) {
                    STYLE_SELECTED
                } else {
                    STYLE_MATCH
                };
                spans_info.push((cs, ce, style));
            }
        }

        // Sort by column start; captures last so they overdraw match bg.
        spans_info.sort_by_key(|(cs, _, _)| *cs);

        if spans_info.is_empty() {
            lines.push(Line::from(dl.text.clone()));
        } else {
            lines.push(build_highlighted_line(&dl.text, &spans_info));
        }
    }

    frame.render_widget(Paragraph::new(lines), inner);

    // Show cursor in editable scratch mode.
    if focused {
        let (line_idx, col) = app.cursor_line_col();
        let screen_row = line_idx.saturating_sub(app.scroll_row);
        if screen_row < inner.height as usize {
            frame.set_cursor_position((
                inner.x + col as u16,
                inner.y + screen_row as u16,
            ));
        }
    }
}

/// Slice `text` into styled spans according to highlight ranges over display columns.
fn build_highlighted_line(text: &str, spans: &[(usize, usize, Style)]) -> Line<'static> {
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    let mut result: Vec<Span<'static>> = Vec::new();
    let mut col = 0usize;

    // Build a per-column style map (last writer wins).
    let mut style_map: Vec<Option<Style>> = vec![None; len];
    for &(cs, ce, style) in spans {
        for c in cs..ce.min(len) {
            style_map[c] = Some(style);
        }
    }

    // Merge consecutive columns with the same style into single spans.
    while col < len {
        let cur_style = style_map[col];
        let mut end = col + 1;
        while end < len && style_map[end] == cur_style {
            end += 1;
        }
        let s: String = chars[col..end].iter().collect();
        result.push(match cur_style {
            Some(st) => Span::styled(s, st),
            None     => Span::raw(s),
        });
        col = end;
    }

    Line::from(result)
}

fn draw_match_info(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::default().borders(Borders::ALL).title("Match info");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if app.matches.is_empty() {
        let msg = if app.compile_error.is_some() {
            "No matches (pattern error)"
        } else if app.pattern.is_empty() {
            "Enter a pattern above"
        } else {
            "No matches"
        };
        frame.render_widget(Paragraph::new(msg).style(STYLE_DIM), inner);
        return;
    }

    let total = app.matches.len();
    let idx   = app.match_index;
    let m     = &app.matches[idx];

    let mut lines: Vec<Line> = Vec::new();

    lines.push(Line::from(vec![
        Span::styled(format!("Match {}/{}", idx + 1, total), STYLE_BOLD),
        Span::raw(format!("  offset {}  len {}", m.range.start, m.range.end - m.range.start)),
    ]));

    if m.captures.is_empty() {
        lines.push(Line::from(Span::styled("  (no capture groups)", STYLE_DIM)));
    } else {
        for cap in &m.captures {
            let label = match &cap.name {
                Some(n) => format!("  \\{}  \"{}\"", n, cap.index),
                None    => format!("  \\{}", cap.index),
            };
            let bytes = escape_bytes(&app.buf[cap.range.start..cap.range.end]);
            lines.push(Line::from(format!(
                "{}  [{}-{}]  {}",
                label, cap.range.start, cap.range.end, bytes
            )));
        }
    }

    frame.render_widget(Paragraph::new(lines), inner);
}

fn draw_status(frame: &mut Frame, app: &App, area: Rect) {
    let line = if let Some((msg, _)) = &app.notification {
        Line::from(vec![
            Span::raw(" "),
            Span::styled(msg.as_str(), Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
        ])
    } else if let Some(e) = &app.compile_error {
        Line::from(vec![
            Span::styled(format!(" {} ", app.filename), STYLE_DIM),
            Span::styled(
                format!("ERROR at offset {}: {}", e.offset, e.message),
                STYLE_ERROR,
            ),
        ])
    } else {
        let matches = app.matches.len();
        let scroll  = app.scroll_row;
        let total   = app.display_lines.len();
        let base = format!(
            " {}  [BYTE]  line {}/{}  {} match{}",
            app.filename, scroll + 1, total, matches,
            if matches == 1 { "" } else { "es" }
        );

        if app.focus == Focus::Pattern || (app.editable && app.focus == Focus::Content) {
            let (mode_str, mode_style) = match app.vim_mode {
                VimMode::Normal => (" -- NORMAL -- ", STYLE_DIM),
                VimMode::Insert => (" -- INSERT -- ", Style::new().fg(Color::Green).add_modifier(Modifier::BOLD)),
                VimMode::Visual => (" -- VISUAL -- ", Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            };
            Line::from(vec![
                Span::styled(mode_str, mode_style),
                Span::raw(base),
            ])
        } else {
            Line::from(base)
        }
    };

    frame.render_widget(Paragraph::new(line), area);
}

fn draw_help(frame: &mut Frame, area: Rect) {
    let help = vec![
        Line::from(Span::styled("  regexdbg keybindings", STYLE_BOLD)),
        Line::from(""),
        Line::from("  Tab            toggle focus (pattern ↔ content)"),
        Line::from("  Enter          confirm / switch to content pane"),
        Line::from(""),
        Line::from(Span::styled("  Pattern box (vim keys):", STYLE_BOLD)),
        Line::from("  Normal:  h/l  w/b/e  0/^/$  i/I/a/A  x/D"),
        Line::from("           dd  cc  yy  p/P  v  u"),
        Line::from("  Insert:  type freely  Esc → Normal"),
        Line::from("  Visual:  motion to extend  d/y/c"),
        Line::from("  Tab / Enter → content"),
        Line::from(""),
        Line::from(Span::styled("  Content — nav mode (file loaded):", STYLE_BOLD)),
        Line::from("  j/k  ↑/↓       scroll one line"),
        Line::from("  f/b  PgDn/PgUp scroll one page"),
        Line::from("  g/G  Home/End   top / bottom"),
        Line::from("  n / N           next / previous match"),
        Line::from("  q / F12         quit"),
        Line::from(""),
        Line::from(Span::styled("  Content — scratch mode (vim keys):", STYLE_BOLD)),
        Line::from("  Normal:  h/j/k/l  w/b/e  0/$/gg/G"),
        Line::from("           i/I/a/A/o/O  insert mode"),
        Line::from("           x  dd  yy  p/P  u  v"),
        Line::from("  Insert:  Esc → Normal  (type freely)"),
        Line::from("  Visual:  motion to extend  d/y/c"),
        Line::from("  F3 / F4   next / prev match"),
        Line::from("  F12 / Ctrl+Q  quit"),
        Line::from(""),
        Line::from(Span::styled("  Inline modifiers (write in pattern):", STYLE_BOLD)),
        Line::from("  (?i)  caseless"),
        Line::from("  (?m)  multiline — ^ and $ match line boundaries"),
        Line::from("  (?s)  dotall   — . matches \\n"),
        Line::from("  (?x)  extended — ignore whitespace, allow # comments"),
        Line::from("  (?U)  ungreedy — quantifiers lazy by default"),
        Line::from("  (?u)  utf+ucp  — UTF-8 + Unicode property support"),
        Line::from("  Combine: (?ims)foo  or  (?i)foo(?-i)bar"),
        Line::from(""),
        Line::from("  F1              this help"),
        Line::from("  F2              copy pattern to clipboard"),
        Line::from("  F5              open live railroad diagram in browser"),
        Line::from(""),
        Line::from(Span::styled("  (any key to close)", STYLE_DIM)),
    ];

    // Centre a box.
    let w = 56u16.min(area.width.saturating_sub(4));
    let h = (help.len() as u16 + 2).min(area.height.saturating_sub(4));
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    let popup = Rect { x, y, width: w, height: h };

    frame.render_widget(Clear, popup);
    frame.render_widget(
        Paragraph::new(help)
            .block(Block::default().borders(Borders::ALL).title("Help"))
            .wrap(Wrap { trim: false }),
        popup,
    );
}

/// Escape non-printable bytes for display in match info panel.
fn escape_bytes(b: &[u8]) -> String {
    let mut s = String::new();
    for &byte in b {
        if byte >= 0x20 && byte <= 0x7E {
            s.push(byte as char);
        } else {
            s.push_str(&format!("\\x{:02X}", byte));
        }
    }
    s
}
