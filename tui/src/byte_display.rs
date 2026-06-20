/// A single displayable line parsed from the raw byte buffer.
#[derive(Debug, Clone)]
pub struct DisplayLine {
    /// The byte offset in the buffer where this line starts.
    pub byte_start: usize,
    /// The rendered text (printable ASCII kept as-is, everything else as \xNN).
    pub text: String,
    /// Maps display-column index → byte offset within the buffer.
    /// Entry i is the buffer offset of the byte(s) rendered at column i.
    pub col_to_byte: Vec<usize>,
}

pub fn build_display_lines(buf: &[u8]) -> Vec<DisplayLine> {
    let mut lines: Vec<DisplayLine> = Vec::new();
    let mut i = 0usize;

    while i <= buf.len() {
        let line_byte_start = i;
        let mut text = String::new();
        let mut col_to_byte: Vec<usize> = Vec::new();

        loop {
            if i >= buf.len() || buf[i] == b'\n' {
                // End of line or end of buffer.
                let end = i;
                if i < buf.len() {
                    i += 1; // consume the '\n'
                } else {
                    i += 1; // sentinel to exit outer loop
                }
                lines.push(DisplayLine { byte_start: line_byte_start, text, col_to_byte });
                // Don't push a trailing newline byte into col_to_byte — it has no column.
                let _ = end;
                break;
            }

            let b = buf[i];
            if b == b'\t' {
                col_to_byte.push(i);
                text.push(' ');
            } else if b >= 0x20 && b <= 0x7E {
                // Printable ASCII.
                col_to_byte.push(i);
                text.push(b as char);
            } else {
                // Non-printable or non-ASCII: render as \xNN (4 display columns).
                let hex = format!("\\x{:02X}", b);
                for _ in 0..hex.len() {
                    col_to_byte.push(i);
                }
                text.push_str(&hex);
            }
            i += 1;
        }
    }

    // Always emit at least one line.
    if lines.is_empty() {
        lines.push(DisplayLine { byte_start: 0, text: String::new(), col_to_byte: Vec::new() });
    }

    lines
}

/// Returns the display-column range [col_start, col_end) for a byte range within a line.
/// Returns None if the byte range doesn't overlap this line at all.
pub fn byte_range_to_col_range(
    line: &DisplayLine,
    line_byte_end: usize, // exclusive — the byte offset just past the last byte of this line
    byte_start: usize,
    byte_end: usize,
) -> Option<(usize, usize)> {
    // Clamp to this line's byte range.
    let ls = line.byte_start;
    let le = line_byte_end;
    if byte_end <= ls || byte_start >= le {
        return None;
    }
    let clamped_start = byte_start.max(ls);
    let clamped_end   = byte_end.min(le);

    let first_col = line.col_to_byte.iter().position(|&b| b >= clamped_start)?;
    // Last col: last column whose source byte is < clamped_end.
    let last_col = line.col_to_byte.iter().rposition(|&b| b < clamped_end)?;

    Some((first_col, last_col + 1))
}
