//! Command layer shared by the CLI, the MCP server and the GUI: typed commands, validation,
//! atomic transactions, operation log and JSON schemas (SPEC.md §8a, §9).

pub mod agile;
pub mod baseline;
pub mod cost;
pub mod diff;
pub mod evm;
pub mod export;
pub mod files;
pub mod finance;
pub mod mutate;
pub mod progress;
pub mod query;
pub mod resources;
pub mod schema;

/// The format key registry, for `alma keys`.
pub use alma_format::keys;

pub use alma_core::diag::{Diagnostic, Severity};
pub use alma_core::model::Plan;
use alma_sched::Schedule;
use serde::Serialize;

/// Version of the alma-gantt toolchain, taken from the workspace manifest.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Version of the JSON output schemas.
pub const OUTPUT_SCHEMA: u32 = 0;

/// A plan read from text: model, schedule and every diagnostic (parse, schedule, lint).
#[derive(Debug, Clone)]
pub struct Loaded {
    /// The plan, when it parsed without errors.
    pub plan: Option<Plan>,
    /// The schedule, when the plan could be scheduled.
    pub schedule: Option<Schedule>,
    /// Parse errors, scheduling errors and lint findings, in that order.
    pub diagnostics: Vec<Diagnostic>,
}

impl Loaded {
    /// True when any diagnostic is an error.
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error)
    }
}

/// The project finish date (the day of the last early finish), as `YYYY-MM-DD`.
pub fn project_finish(schedule: &Schedule) -> String {
    alma_core::time::finish_date(schedule.finish).to_string()
}

/// Parses, schedules and lints plan text.
pub fn load(src: &str) -> Loaded {
    let parsed = alma_format::parse(src);
    let mut diagnostics = parsed.diagnostics;
    let Some(plan) = parsed.plan else {
        return Loaded {
            plan: None,
            schedule: None,
            diagnostics,
        };
    };
    let schedule = match alma_sched::schedule(&plan) {
        Ok(s) => {
            diagnostics.extend(s.diagnostics.iter().cloned());
            Some(s)
        }
        Err(errs) => {
            diagnostics.extend(errs);
            None
        }
    };
    diagnostics.extend(alma_sched::lint(&plan, schedule.as_ref()));
    Loaded {
        plan: Some(plan),
        schedule,
        diagnostics,
    }
}

/// Like [`load`] for the plan at `path`: adds the rules that measure the plan against its
/// active baseline (`L-BAS-03`, `L-EVM-*`; ADR 0019) when its snapshot can be read.
pub fn load_file(path: &std::path::Path, src: &str) -> Loaded {
    let mut l = load(src);
    if let (Some(plan), Some(schedule)) = (&l.plan, &l.schedule)
        && let Ok(Some(snap)) = baseline::snapshot_for(plan, path, None)
    {
        let parent = plan
            .baselines
            .iter()
            .find(|b| b.id == snap.baseline)
            .and_then(|b| b.parent.as_deref())
            .and_then(|p| baseline::read_snapshot(path, p).ok());
        l.diagnostics.extend(alma_sched::lint_baseline(
            plan,
            schedule,
            &snap,
            parent.as_ref(),
        ));
    }
    l
}

/// Counts of diagnostics by severity.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Counts {
    /// Errors.
    pub error: usize,
    /// Warnings.
    pub warn: usize,
    /// Infos.
    pub info: usize,
}

impl Counts {
    /// Counts a diagnostic list.
    pub fn of(diags: &[Diagnostic]) -> Self {
        let mut c = Counts::default();
        for d in diags {
            match d.severity {
                Severity::Error => c.error += 1,
                Severity::Warn => c.warn += 1,
                Severity::Info => c.info += 1,
            }
        }
        c
    }
}
