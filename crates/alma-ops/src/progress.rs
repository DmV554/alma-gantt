//! Progress at the status date: `alma progress ls` (ADR 0018).
//!
//! Lists the work that should be moving: tasks started or finished, and tasks the baseline
//! expected to have started by the status date that have not. Dates of the current schedule
//! are forecasts from the status date; the baseline gives the promise to compare with.

use std::collections::HashMap;

use alma_core::duration::Duration;
use alma_core::model::{Plan, Snapshot, TaskKind};
use alma_core::time::{DAY, finish_date, start_date, start_of};
use alma_sched::Schedule;
use alma_sched::cpm::{Calendars, PlanIndex, Progress};
use serde::Serialize;

use crate::OUTPUT_SCHEMA;
use crate::query::Page;

/// Progress state of a task, as reports name it.
pub fn state_name(p: Progress) -> &'static str {
    match p {
        Progress::NotStarted => "not_started",
        Progress::InProgress { .. } => "in_progress",
        Progress::Complete => "complete",
    }
}

/// One task in `alma progress ls`.
#[derive(Debug, Clone, Serialize)]
pub struct ProgressRow {
    /// Task id.
    pub id: String,
    /// `not_started`, `in_progress` or `complete`.
    pub state: &'static str,
    /// Recorded actual start.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actual_start: Option<String>,
    /// Recorded actual finish.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actual_finish: Option<String>,
    /// Work left, for tasks in progress.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remaining: Option<String>,
    /// Current (forecast) start.
    pub start: String,
    /// Current (forecast) finish.
    pub finish: String,
    /// Baseline start.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub baseline_start: Option<String>,
    /// Baseline finish.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub baseline_finish: Option<String>,
    /// Finish variance in working days against the baseline (positive = later).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish_var: Option<f64>,
    /// The baseline expected it to have started by the status date, and it has not.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub late_start: bool,
    /// On the critical set.
    pub critical: bool,
}

/// Task counts by state.
#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct StateCounts {
    /// Finished.
    pub complete: usize,
    /// Started, not finished.
    pub in_progress: usize,
    /// Not started.
    pub not_started: usize,
}

/// Result of `alma progress ls`.
#[derive(Debug, Clone, Serialize)]
pub struct ProgressReport {
    /// Output schema version.
    pub schema: u32,
    /// The status date, when set.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_date: Option<String>,
    /// Leaf activities by state (summaries and level-of-effort tasks are not counted).
    pub counts: StateCounts,
    /// Forecast project finish.
    pub finish: String,
    /// Baseline compared against.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub baseline: Option<String>,
    /// Baseline project finish.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub baseline_finish: Option<String>,
    /// Finish variance in working days on the project calendar.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish_var: Option<f64>,
    /// Matching tasks.
    pub total: usize,
    /// Tasks on this page.
    pub tasks: Vec<ProgressRow>,
    /// Cursor for the next page.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

/// `alma progress ls`. With `all`, every leaf activity; otherwise the started ones and those
/// the baseline expected to have started by the status date.
pub fn report(
    plan: &Plan,
    schedule: &Schedule,
    snap: Option<&Snapshot>,
    all: bool,
    page: Page,
) -> ProgressReport {
    let index = PlanIndex::new(plan);
    let cals = Calendars::new(plan);
    let units = plan.settings.units;
    let day = units.minutes_per_day as f64;
    let days = |m: i64| (m as f64 / day * 100.0).round() / 100.0;
    let base: HashMap<&str, &alma_core::model::SnapTask> = snap
        .map(|s| s.tasks.iter().map(|t| (t.id.as_str(), t)).collect())
        .unwrap_or_default();
    let data = plan.project.status_date.map(|d| start_of(d) + DAY);
    let signed = |cal: &alma_sched::WorkCalendar, from: i64, to: i64| {
        if to >= from {
            cal.between(from, to)
        } else {
            -cal.between(to, from)
        }
    };
    let date_of = |es: i64, ef: i64, t: i64| {
        if es == ef {
            start_date(t).to_string()
        } else {
            finish_date(t).to_string()
        }
    };
    let mut counts = StateCounts::default();
    let mut rows = Vec::new();
    for (i, t) in plan.tasks.iter().enumerate() {
        let d = &schedule.tasks[i];
        if index.is_summary(i) || t.kind == TaskKind::Loe || !d.in_network {
            continue;
        }
        match d.progress {
            Progress::NotStarted => counts.not_started += 1,
            Progress::InProgress { .. } => counts.in_progress += 1,
            Progress::Complete => counts.complete += 1,
        }
        let b = base.get(t.id.as_str());
        let late_start =
            !d.progress.started() && matches!((b, data), (Some(b), Some(data)) if b.start < data);
        if !all && !d.progress.started() && !late_start {
            continue;
        }
        let cal = cals.of_task(plan, i);
        let remaining = match d.progress {
            Progress::InProgress { remaining, .. } => Some(
                Duration {
                    minutes: remaining,
                    elapsed: d.duration.elapsed,
                }
                .display(units),
            ),
            _ => None,
        };
        rows.push(ProgressRow {
            id: t.id.clone(),
            state: state_name(d.progress),
            actual_start: t.actual_start.map(|x| x.to_string()),
            actual_finish: t.actual_finish.map(|x| x.to_string()),
            remaining,
            start: start_date(d.es).to_string(),
            finish: date_of(d.es, d.ef, d.ef),
            baseline_start: b.map(|b| start_date(b.start).to_string()),
            baseline_finish: b.map(|b| date_of(b.start, b.finish, b.finish)),
            finish_var: b.map(|b| days(signed(cal, b.finish, d.ef))),
            late_start,
            critical: d.critical,
        });
    }
    let total = rows.len();
    let (tasks, next_cursor) = page.apply(&rows);
    ProgressReport {
        schema: OUTPUT_SCHEMA,
        status_date: plan.project.status_date.map(|d| d.to_string()),
        counts,
        finish: finish_date(schedule.finish).to_string(),
        baseline: snap.map(|s| s.baseline.clone()),
        baseline_finish: snap.map(|s| finish_date(s.finish).to_string()),
        finish_var: snap.map(|s| days(signed(cals.project(), s.finish, schedule.finish))),
        total,
        tasks,
        next_cursor,
    }
}
