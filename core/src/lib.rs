mod error;
mod flags;
mod matcher;

pub use error::CompileError;
pub use flags::Flags;
// run_matches takes &mut CompiledPattern because CaptureLocations requires exclusive access.
pub use matcher::{CaptureGroup, CompiledPattern, Match, compile, run_matches};

#[cfg(test)]
mod tests {
    use super::*;

    fn pat(pattern: &str) -> CompiledPattern {
        compile(pattern, Flags::default()).expect("compile failed")
    }

    #[test]
    fn basic_match_byte_offsets() {
        let mut p = pat(r"foo");
        let matches = run_matches(&mut p, b"hello foo world");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].range, 6..9);
    }

    #[test]
    fn multiple_matches() {
        let mut p = pat(r"ab");
        let matches = run_matches(&mut p, b"ab__ab__ab");
        assert_eq!(matches.len(), 3);
        assert_eq!(matches[0].range, 0..2);
        assert_eq!(matches[1].range, 4..6);
        assert_eq!(matches[2].range, 8..10);
    }

    #[test]
    fn numbered_capture_group() {
        let mut p = pat(r"(foo)(bar)");
        let matches = run_matches(&mut p, b"foobar");
        assert_eq!(matches.len(), 1);
        let caps = &matches[0].captures;
        assert_eq!(caps.len(), 2);
        assert_eq!(caps[0].index, 1);
        assert_eq!(caps[0].range, 0..3);
        assert_eq!(caps[1].index, 2);
        assert_eq!(caps[1].range, 3..6);
    }

    #[test]
    fn named_capture_group() {
        let mut p = pat(r"(?P<word>\w+)");
        let matches = run_matches(&mut p, b"hello");
        assert_eq!(matches.len(), 1);
        let cap = &matches[0].captures[0];
        assert_eq!(cap.name.as_deref(), Some("word"));
        assert_eq!(cap.range, 0..5);
    }

    #[test]
    fn flag_caseless() {
        let mut p = compile("FOO", Flags { caseless: true, ..Default::default() }).unwrap();
        let matches = run_matches(&mut p, b"foo FOO Foo");
        assert_eq!(matches.len(), 3);
    }

    #[test]
    fn flag_multiline() {
        let mut p = compile(r"^bar", Flags { multiline: true, ..Default::default() }).unwrap();
        let matches = run_matches(&mut p, b"foo\nbar\nbaz");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].range, 4..7);
    }

    #[test]
    fn flag_dotall() {
        let mut p = compile(r"a.b", Flags { dotall: true, ..Default::default() }).unwrap();
        let matches = run_matches(&mut p, b"a\nb");
        assert_eq!(matches.len(), 1);
    }

    #[test]
    fn flag_ungreedy() {
        let mut p = compile(r"a.*b", Flags { ungreedy: true, ..Default::default() }).unwrap();
        let matches = run_matches(&mut p, b"aXbYb");
        // Ungreedy: matches shortest first -> "aXb"
        assert_eq!(matches[0].range, 0..3);
    }

    #[test]
    fn zero_width_match_no_infinite_loop() {
        let mut p = pat(r"a*");
        let matches = run_matches(&mut p, b"bbb");
        assert!(!matches.is_empty());
        // All positions must advance (no repeated start positions).
        let mut prev_start = usize::MAX;
        for m in &matches {
            assert_ne!(m.range.start, prev_start);
            prev_start = m.range.start;
        }
    }

    #[test]
    fn invalid_pattern_returns_error_with_offset() {
        let result = compile(r"(unclosed", Flags::default());
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(!err.message.is_empty());
        // PCRE2 reports an offset into the pattern.
        assert!(err.offset > 0 || err.message.contains("missing"));
    }

    #[test]
    fn invalid_utf8_buffer_no_panic() {
        let mut p = pat(r"foo");
        let buf: &[u8] = b"foo\xFF\xFEbar foo";
        let matches = run_matches(&mut p, buf);
        assert_eq!(matches.len(), 2);
    }

    #[test]
    fn null_bytes_in_buffer_no_panic() {
        let mut p = pat(r"foo");
        let buf: &[u8] = b"foo\x00foo";
        let matches = run_matches(&mut p, buf);
        assert_eq!(matches.len(), 2);
    }

    #[test]
    fn no_match_returns_empty() {
        let mut p = pat(r"xyz");
        let matches = run_matches(&mut p, b"hello world");
        assert!(matches.is_empty());
    }
}
