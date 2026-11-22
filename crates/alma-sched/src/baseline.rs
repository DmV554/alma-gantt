//! Baseline snapshots and variance against them (knowledge notes, scheduling §4.14 and
//! resources §6; ADR 0013).
//!
//! A snapshot freezes what a baseline promised: each task's dates and cost, the project finish
//! and the time-phased budget as a daily cumulative curve. Planned value (PV, F-COST-13) comes
//! from that curve, so later edits to calendars or rates never rewrite it.

use std::collections::HashMap;

use alma_core::model::{Plan, SnapTask, Snapshot};
use alma_core::time::{DAY, Instant, date_of_day, day_of};
use jiff::civil::Date;
use rust_decimal::Decimal;

use crate::cost::{Costs, costs, share};
use crate::cpm::{Calendars, Schedule};

/// Freezes a scheduled plan as baseline `id`.
pub fn snapshot(id: &str, plan: &Plan, schedule: &Schedule) -> Snapshot {
    let c = costs(plan, schedule);
    Snapshot {
        baseline: id.into(),
        start: schedule.start,
        finish: schedule.finish,
        bac: c.total,
        tasks: plan
            .tasks
            .iter()
            .zip(&schedule.tasks)
            .zip(&c.task_total)
            .map(|((t, d), cost)| SnapTask {
                id: t.id.clone(),
                start: d.es,
                finish: d.ef,
                cost: *cost,
            })
            .collect(),
        pv: daily(plan, &c),
    }
}

/// Daily cumulative cost, only on days where it changes.
fn daily(plan: &Plan, c: &Costs) -> Vec<(Date, Decimal)> {
    let cals = Calendars::new(plan);
    let mut per_day: HashMap<i64, Decimal> = HashMap::new();
    for line in &c.lines {
        let (from, to) = match line.accrual {
            crate::cost::Accrual::At(t) => (t, t + 1),
            crate::cost::Accrual::Spread { from, to, .. } => (from, to),
        };
        for day in day_of(from)..=day_of((to - 1).max(from)) {
            let a = day * DAY;
            let part = share(plan, &cals, line, a, a + DAY);
            if !part.is_zero() {
                *per_day.entry(day).or_default() += part;
            }
        }
    }
    let mut days: Vec<i64> = per_day.keys().copied().collect();
    days.sort_unstable();
    let mut cum = Decimal::ZERO;
    days.into_iter()
        .map(|d| {
            cum += per_day[&d];
            (date_of_day(d), cum)
        })
        .collect()
}

/// How one task moved against the baseline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskVariance {
    /// Task index in the current plan.
    pub task: usize,
    /// Baseline start and finish; `None` when the task is new.
    pub baseline: Option<(Instant, Instant)>,
    /// Start variance in working minutes on the task calendar (positive = later).
    pub start_var: i64,
    /// Finish variance in working minutes (positive = later).
    pub finish_var: i64,
    /// Baseline cost; zero for new tasks.
    pub baseline_cost: Decimal,
    /// Current cost.
    pub cost: Decimal,
}

/// Variance of every current task against a snapshot, plan order. Tasks the baseline has but
/// the plan lost are returned by id as the second value.
pub fn variance(
    plan: &Plan,
    schedule: &Schedule,
    snap: &Snapshot,
) -> (Vec<TaskVariance>, Vec<String>) {
    let cals = Calendars::new(plan);
    let c = costs(plan, schedule);
    let by_id: HashMap<&str, &SnapTask> = snap.tasks.iter().map(|t| (t.id.as_str(), t)).collect();
    let signed = |cal: &crate::WorkCalendar, from: Instant, to: Instant| {
        if to >= from {
            cal.between(from, to)
        } else {
            -cal.between(to, from)
        }
    };
    let rows = plan
        .tasks
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let d = &schedule.tasks[i];
            let cal = cals.of_task(plan, i);
            match by_id.get(t.id.as_str()) {
                Some(b) => TaskVariance {
                    task: i,
                    baseline: Some((b.start, b.finish)),
                    start_var: signed(cal, b.start, d.es),
                    finish_var: signed(cal, b.finish, d.ef),
                    baseline_cost: b.cost,
                    cost: c.task_total[i],
                },
                None => TaskVariance {
                    task: i,
                    baseline: None,
                    start_var: 0,
                    finish_var: 0,
                    baseline_cost: Decimal::ZERO,
                    cost: c.task_total[i],
                },
            }
        })
        .collect();
    let current: std::collections::HashSet<&str> =
        plan.tasks.iter().map(|t| t.id.as_str()).collect();
    let removed = snap
        .tasks
        .iter()
        .filter(|t| !current.contains(t.id.as_str()))
        .map(|t| t.id.clone())
        .collect();
    (rows, removed)
}

/// Planned value per period from a snapshot: the cumulative value at the end of each bucket.
pub fn pv_curve(snap: &Snapshot, buckets: &[(Date, Date)]) -> Vec<Decimal> {
    buckets
        .iter()
        .map(|(_, end)| {
            let last = end.checked_sub(jiff::Span::new().days(1)).unwrap_or(*end);
            snap.pv_through(last)
        })
        .collect()
}

/// The first day of a snapshot's money, for aligning curves.
pub fn first_day(snap: &Snapshot) -> Date {
    snap.pv
        .first()
        .map(|(d, _)| *d)
        .unwrap_or_else(|| date_of_day(day_of(snap.start)))
}
