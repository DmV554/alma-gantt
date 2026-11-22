//! `alma diff`: what changed between two versions of a plan, in schedule terms.

use alma_core::diag::Diagnostic;
use serde::Serialize;

use crate::mutate::{Moved, end_delta, moved, new_diagnostics};
use crate::{Loaded, OUTPUT_SCHEMA};

/// Result of `alma diff`.
#[derive(Debug, Clone, Serialize)]
pub struct DiffReport {
    /// Output schema version.
    pub schema: u32,
    /// Tasks added, removed or whose dates moved (start/finish deltas in working days).
    pub changed: Vec<Moved>,
    /// Change of the project finish.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_date_delta: Option<String>,
    /// Diagnostics present now but not before.
    pub new_diagnostics: Vec<Diagnostic>,
    /// Diagnostics present before but not now.
    pub resolved_diagnostics: Vec<Diagnostic>,
}

/// Compares an older version of a plan with the current one.
pub fn diff(before: &Loaded, after: &Loaded) -> DiffReport {
    DiffReport {
        schema: OUTPUT_SCHEMA,
        changed: moved(before, after),
        end_date_delta: end_delta(before, after),
        new_diagnostics: new_diagnostics(before, after),
        resolved_diagnostics: new_diagnostics(after, before),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_moves_additions_and_finish() {
        let old = "[project]\nformat = 0\nname = \"d\"\nstart = 2027-01-04\n\n[[task]]\nid = \"a\"\nname = \"A\"\nduration = \"5d\"\n\n[[task]]\nid = \"gone\"\nname = \"Gone\"\nduration = \"1d\"\n";
        let new = "[project]\nformat = 0\nname = \"d\"\nstart = 2027-01-04\n\n[[task]]\nid = \"a\"\nname = \"A\"\nduration = \"7d\"\n\n[[task]]\nid = \"b\"\nname = \"B\"\nduration = \"1d\"\ndeps = [{ pred = \"a\" }]\n";
        let r = diff(&crate::load(old), &crate::load(new));
        let find = |id: &str| r.changed.iter().find(|m| m.id == id).unwrap();
        assert_eq!(find("a").finish, "+2d");
        assert_eq!(find("b").start, "new");
        assert_eq!(find("gone").start, "removed");
        assert_eq!(r.end_date_delta.as_deref(), Some("+3d"));
    }
}
