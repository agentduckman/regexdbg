# PCRE2 Regex Debugger (TUI) — Build Spec

## What we're building
A terminal UI regex debugger for developing PCRE detections against malicious
script content. Functionally it's the match-highlighting half of debuggex
(https://www.debuggex.com/), minus the railroad diagram, PCRE2-only, built to
stay fast and not break on large or dirty input files. Single user, single file
at a time, fully local. No web, no telemetry, no required config.

## Stack / constraints
- Rust. Cargo workspace.
- TUI: `ratatui` + `crossterm` (current versions, not the old `tui-rs`).
- Regex matching: the `pcre2` crate (BurntSushi), byte mode
  (`pcre2::bytes::Regex`). Consult the crate's current docs for the real API
  rather than guessing signatures.
- Match against raw bytes (`&[u8]`/`Vec<u8>`), NOT `String`. Input is frequently
  invalid UTF-8 — malware samples, shellcode, embedded nulls, mixed encodings.
  Never assume valid UTF-8 anywhere.
- Targets Linux and macOS. Links system libpcre2 (pacman `pcre2` / brew `pcre2`).
  Static-link if it's straightforward; dynamic linking is acceptable — do not
  rabbit-hole chasing a perfect static build.
- Keep dependencies minimal.

## Architecture (important — keep these decoupled)
Two crates in the workspace:
1. `core` — all logic, zero UI. Compiles patterns, runs matches over a byte
   buffer, extracts capture groups, applies flags, reports compile errors with
   offsets. Pure functions where practical. Knows nothing about terminals.
2. `tui` — the app. A `ratatui`+`crossterm` front-end depending on `core`.

Rationale: a railroad-diagram visualizer is planned for a later phase. It will be
a NEW consumer of `core` (an SVG-export command and/or a separate GUI binary),
NOT a terminal-rendered diagram. So `core` must stay UI-agnostic and the matching
logic must never get baked into the TUI layer.

## Matching behavior (`core`)
- Byte mode via `pcre2::bytes::Regex`. All offsets are byte offsets.
- Recompute all matches ONCE whenever pattern, flags, or buffer changes; store
  the resulting match spans. Do NOT re-run the regex on every render or scroll.
- Per match: the overall byte range, plus every capture group (numbered AND
  named) with its byte range. Named groups must be exposed.
- Handle zero-width matches without infinite-looping (advance position).
- Compile errors: surface pcre2's error message AND the byte offset into the
  pattern where compilation failed.
- Toggleable PCRE2 options exposed as booleans: caseless (i), multiline (m),
  dotall (s), extended (x), ungreedy (U), and UTF+UCP (u). UTF is OFF by default
  (byte mode); enabling it turns on UTF-8 validation/semantics for when the input
  is known-clean Unicode.

## TUI behavior (`tui`)
- Launch: `regexdbg <path>` loads a file. If no path is given, read file content
  from stdin. NOTE: if content comes from stdin, read keyboard events from
  `/dev/tty` directly — stdin is consumed by the piped data and can't also drive
  the event loop.
- Layout:
  - Editable pattern input line at the top.
  - A flags bar showing which toggles are active.
  - Main content pane: the loaded bytes, scrollable, all matches highlighted;
    capture groups styled distinctly from the overall match; named groups shown.
  - A match-info panel: total match count, current match index, current match's
    byte offset + length, and its group captures (number, name if any, byte
    range, captured bytes shown escaped).
  - Status line: filename, byte/UTF mode indicator, scroll position, match count,
    and the compile-error message (pointing at the pattern offset) when invalid.
- Byte rendering: printable ASCII as-is; tabs handled sensibly; ALL
  non-printable / non-UTF bytes shown as escapes like `\xNN` so raw control bytes
  never reach the terminal. Split lines on `\n` (0x0A). Maintain a byte-offset ->
  display-column mapping for visible lines so highlights land on the right cells.
- Performance: virtualize — only build styled output for lines in the visible
  viewport, never the whole file. Debounce pattern edits (~150ms) so typing stays
  responsive on large buffers. Keep matching a single `core` call so it's trivial
  to move to a background thread later; synchronous-with-debounce is fine for v1.
- Keybinds (pick sane defaults, show them in a help overlay):
  - Toggle focus between pattern editing and content navigation.
  - Toggle each flag.
  - Next / previous match; jump the view to the current match.
  - Scroll by line / page / to top / to bottom.
  - Help.
  - Quit.

## Out of scope for v1 (do NOT build — but do NOT architect them out)
- Railroad / NFA diagram (planned later as a `core` consumer).
- Full backtracking step-debugger.
- Multiple files / directory / corpus matching.

## Tests
- Unit tests in `core` ONLY:
  - basic match returns correct byte offsets
  - numbered and named capture extraction
  - each flag changes behavior as expected
  - zero-width match handling (no infinite loop)
  - invalid pattern returns an error carrying the offset
  - buffers containing invalid UTF-8 / null bytes match without panicking
- No TUI tests required.

## How to work
- Scaffold the workspace, get `core` building with its tests passing FIRST, then
  build the `tui`.
- Don't gold-plate. Stick to the scope above. If something seems missing, or you
  want to add anything beyond this list, ASK me before building it.
- Idiomatic, readable Rust. Comment the non-obvious bits — specifically the
  byte<->display-column mapping and the match-recompute strategy.
