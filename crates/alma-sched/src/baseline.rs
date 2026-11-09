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
    let cals = Calendars::new(plan);
    // Each task's own budget: its assignments and fixed cost; reserves are not earned
    // (ADR 0019).
    let mut own: Vec<Vec<&crate::cost::CostLine>> = vec![Vec::new(); plan.tasks.len()];
    for line in &c.lines {
        if line.task < plan.tasks.len()
            && matches!(
                line.source,
                crate::cost::Source::Resource(_) | crate::cost::Source::Fixed
            )
        {
            own[line.task].push(line);
        }
    }
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
            .zip(&own)
            .map(|(((t, d), cost), lines)| SnapTask {
                id: t.id.clone(),
                start: d.es,
                finish: d.ef,
                cost: *cost,
                budget: lines.iter().map(|l| l.amount).sum(),
                pv: daily_of(plan, &cals, lines.iter().copied()),
            })
            .collect(),
        pv: daily(plan, &c),
        per_task: true,
        revised_at: None,
    }
}

/// A cumulative curve that follows `old` through `at` and then grows as `new` does after `at`.
fn splice(old: &[(Date, Decimal)], new: &[(Date, Decimal)], at: Date) -> Vec<(Date, Decimal)> {
    let through = |c: &[(Date, Decimal)]| match c.binary_search_by(|(d, _)| d.cmp(&at)) {
        Ok(i) => c[i].1,
        Err(0) => Decimal::ZERO,
        Err(i) => c[i - 1].1,
    };
    let (base, cut) = (through(old), through(new));
    let mut out: Vec<(Date, Decimal)> = old.iter().filter(|(d, _)| *d <= at).copied().collect();
    out.extend(
        new.iter()
            .filter(|(d, _)| *d > at)
            .map(|(d, x)| (*d, base + *x - cut)),
    );
    out
}

/// Revises baseline `parent` at status date `at` (knowledge notes, resources §6.1; ADR 0020):
/// planned value through `at` stays the parent's, so past performance is never rewritten, and
/// the work after `at` follows the current plan. Dates are the current plan's: the new target.
/// A task the plan no longer has keeps the planned value it had through `at`, and no more.
pub fn revise(id: &str, plan: &Plan, schedule: &Schedule, parent: &Snapshot, at: Date) -> Snapshot {
    let cur = snapshot(id, plan, schedule);
    let old: HashMap<&str, &SnapTask> = parent.tasks.iter().map(|t| (t.id.as_str(), t)).collect();
    let empty: Vec<(Date, Decimal)> = Vec::new();
    let mut tasks: Vec<SnapTask> = cur
        .tasks
        .iter()
        .map(|t| {
            let past = old.get(t.id.as_str()).map_or(&empty, |p| &p.pv);
            let pv = splice(past, &t.pv, at);
            let budget = pv.last().map_or(Decimal::ZERO, |(_, x)| *x);
            SnapTask {
                pv,
                budget,
                ..t.clone()
            }
        })
        .collect();
    let current: std::collections::HashSet<&str> =
        cur.tasks.iter().map(|t| t.id.as_str()).collect();
    for p in parent
        .tasks
        .iter()
        .filter(|p| !current.contains(p.id.as_str()))
    {
        let pv: Vec<(Date, Decimal)> = p.pv.iter().filter(|(d, _)| *d <= at).copied().collect();
        let budget = pv.last().map_or(Decimal::ZERO, |(_, x)| *x);
        if !budget.is_zero() {
            tasks.push(SnapTask {
                pv,
                budget,
                ..p.clone()
            });
        }
    }
    let pv = splice(&parent.pv, &cur.pv, at);
    let bac = pv.last().map_or(Decimal::ZERO, |(_, x)| *x);
    Snapshot {
        bac,
        tasks,
        pv,
        revised_at: Some(at),
        ..cur
    }
}

/// Daily cumulative cost, only on days where it changes.
pub fn daily(plan: &Plan, c: &Costs) -> Vec<(Date, Decimal)> {
    daily_of(plan, &Calendars::new(plan), c.lines.iter())
}

/// Daily cumulative cost of some cost lines, only on days where it changes.
fn daily_of<'a>(
    plan: &Plan,
    cals: &Calendars,
    lines: impl Iterator<Item = &'a crate::cost::CostLine>,
) -> Vec<(Date, Decimal)> {
    let mut per_day: HashMap<i64, Decimal> = HashMap::new();
    for line in lines {
        let (from, to) = match line.accrual {
            crate::cost::Accrual::At(t) => (t, t + 1),
            crate::cost::Accrual::Spread { from, to, .. } => (from, to),
        };
        for day in day_of(from)..=day_of((to - 1).max(from)) {
            let a = day * DAY;
            let part = share(plan, cals, line, a, a + DAY);
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
