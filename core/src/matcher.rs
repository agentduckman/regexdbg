use std::ops::Range;

use pcre2::bytes::{CaptureLocations, Regex, RegexBuilder};

use crate::error::CompileError;
use crate::flags::Flags;

#[derive(Debug)]
pub struct CompiledPattern {
    regex:       Regex,
    locs:        CaptureLocations,
    group_names: Vec<Option<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureGroup {
    pub index: usize,
    pub name:  Option<String>,
    pub range: Range<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Match {
    pub range:    Range<usize>,
    pub captures: Vec<CaptureGroup>,
}

pub fn compile(pattern: &str, flags: Flags) -> Result<CompiledPattern, CompileError> {
    // PCRE2_UNGREEDY isn't exposed by RegexBuilder, so use the inline modifier instead.
    let effective_pattern = if flags.ungreedy {
        format!("(?U){pattern}")
    } else {
        pattern.to_string()
    };

    let mut builder = RegexBuilder::new();
    builder
        .caseless(flags.caseless)
        .multi_line(flags.multiline)
        .dotall(flags.dotall)
        .extended(flags.extended);

    if flags.utf_ucp {
        builder.utf(true).ucp(true);
    }

    let regex = builder.build(&effective_pattern).map_err(|e| CompileError {
        message: e.to_string(),
        offset:  e.offset().unwrap_or(0),
    })?;

    // capture_names() returns &[Option<String>] — index 0 is the full match (always None).
    let group_names: Vec<Option<String>> = regex.capture_names().to_vec();
    let locs = regex.capture_locations();

    Ok(CompiledPattern { regex, locs, group_names })
}

pub fn run_matches(pattern: &mut CompiledPattern, buf: &[u8]) -> Vec<Match> {
    let mut results = Vec::new();
    let mut pos = 0usize;

    while pos <= buf.len() {
        let m = match pattern.regex.captures_read_at(&mut pattern.locs, buf, pos) {
            Ok(Some(m)) => m,
            _ => break,
        };

        let start = m.start();
        let end   = m.end();

        let mut groups = Vec::new();
        // Groups start at index 1; index 0 is the full match (same as `range`).
        for i in 1..pattern.locs.len() {
            if let Some((gs, ge)) = pattern.locs.get(i) {
                groups.push(CaptureGroup {
                    index: i,
                    name:  pattern.group_names.get(i).and_then(|n| n.clone()),
                    range: gs..ge,
                });
            }
        }

        results.push(Match { range: start..end, captures: groups });

        // Advance past zero-width matches to avoid an infinite loop.
        pos = if end > start { end } else { start + 1 };
    }

    results
}
