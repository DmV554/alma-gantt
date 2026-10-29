//! Domain model of alma-gantt: projects, WBS nodes, activities, dependencies, calendars,
//! resources, money and durations. No I/O, no UI (SPEC.md §6, §7.6, §9).

pub mod diag;
pub mod duration;
pub mod model;
pub mod time;

pub use diag::{Diagnostic, Severity, Span};
pub use duration::{Duration, DurationError, WorkUnits, parse_duration};
pub use model::*;

/// Checks the id pattern `^[a-z0-9][a-z0-9-]{0,62}$` (`docs/format.md` §3.2).
pub fn is_valid_id(id: &str) -> bool {
    let bytes = id.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 63
        && (bytes[0].is_ascii_lowercase() || bytes[0].is_ascii_digit())
        && bytes
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_pattern() {
        assert!(is_valid_id("gate-ocr"));
        assert!(is_valid_id("s01"));
        assert!(!is_valid_id("-gate"));
        assert!(!is_valid_id("Gate"));
        assert!(!is_valid_id("gate_ocr"));
        assert!(!is_valid_id(""));
        assert!(!is_valid_id(&"a".repeat(64)));
    }
}
