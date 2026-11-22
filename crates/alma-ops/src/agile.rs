//! `alma iter ls`: iterations with committed and finished points, velocity and a forecast
//! (knowledge notes, scheduling §5.3, §5.6; F-COST-29; ADR 0021).
//!
//! Velocity is per cadence, never summed across cadences: points are relative to one team.
//! An iteration counts as finished when it ends on or before the status date; its velocity is
//! the points of the work in it that has an `actual_finish`.

use alma_core::model::{Plan, TaskKind};
use alma_core::time::{DAY, finish_date, start_date, start_of};
use alma_sched::agile::{iterations, windows};
use alma_sched::cpm::{Calendars, PlanIndex};
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use serde::Serialize;

use crate::OUTPUT_SCHEMA;

fn num(d: Decimal) -> f64 {
    d.round_dp(2).to_f64().unwrap_or(0.0)
}

/// One iteration.
#[derive(Debug, Clone, Serialize)]
pub struct IterationRow {
    /// Id.
    pub id: String,
    /// First day.
    pub start: String,
    /// Last day.
    pub finish: String,
    /// `done` (ended by the status date), `current` or `planned`.
    pub state: &'static str,
    /// Points of the work planned in it.
    pub committed: f64,
    /// Points of that work already finished.
    pub done: f64,
    /// The cadence capacity, when set.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capacity: Option<f64>,
    /// Finished over committed, for ended iterations (F-COST-29).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spi: Option<f64>,
    /// Number of tasks planned in it.
    pub items: usize,
}

/// A forecast from velocity.
#[derive(Debug, Clone, Serialize)]
pub struct Forecast {
    /// Points not finished yet: in the cadence's iterations, plus the backlog when the plan
    /// has a single cadence.
    pub remaining: f64,
    /// Mean velocity of the ended iterations.
    pub velocity: f64,
    /// Iterations needed at that velocity, counted from the first one not ended.
    pub iterations: u32,
    /// Last day of the last of them.
    pub finish: String,
    /// Range from the best and worst ended iterations: (iterations, finish).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub optimistic: Option<(u32, String)>,
    /// Same, from the worst iteration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pessimistic: Option<(u32, String)>,
}

/// One cadence.
#[derive(Debug, Clone, Serialize)]
pub struct CadenceView {
    /// Id.
    pub id: String,
    /// Name.
    pub name: String,
    /// Its iterations.
    pub iterations: Vec<IterationRow>,
    /// Velocities of the ended iterations, in order.
    pub velocities: Vec<f64>,
    /// When there is a velocity and work left.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub forecast: Option<Forecast>,
    /// Why there is no forecast.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// Result of `alma iter ls`.
#[derive(Debug, Clone, Serialize)]
pub struct IterReport {
    /// Output schema version.
    pub schema: u32,
    /// Status date used to tell ended iterations.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_date: Option<String>,
    /// Cadences.
    pub cadences: Vec<CadenceView>,
    /// Points of stories in no iteration.
    pub backlog: f64,
    /// Number of stories in no iteration.
    pub backlog_items: usize,
}

/// `alma iter ls`, for every cadence or the one named.
pub fn report(plan: &Plan, cadence: Option<&str>) -> Result<IterReport, String> {
    if let Some(c) = cadence
        && !plan.cadences.iter().any(|x| x.id == c)
    {
        let ids: Vec<&str> = plan.cadences.iter().map(|c| c.id.as_str()).collect();
        return Err(format!("no cadence `{c}`; cadences: {}", ids.join(", ")));
    }
    let cals = Calendars::new(plan);
    let index = PlanIndex::new(plan);
    let iters = iterations(plan, &cals);
    let data = plan.project.status_date.map(|d| start_of(d) + DAY);
    let leaf = |i: usize| !index.is_summary(i);
    let points = |i: usize| plan.tasks[i].points.unwrap_or_default();
    let done = |i: usize| plan.tasks[i].actual_finish.is_some();
    let backlog: Vec<usize> = (0..plan.tasks.len())
        .filter(|&i| leaf(i) && plan.tasks[i].kind == TaskKind::Story)
        .filter(|&i| plan.tasks[i].iteration.is_none())
        .collect();
    let mut cadences = Vec::new();
    for (ci, c) in plan.cadences.iter().enumerate() {
        if cadence.is_some_and(|x| x != c.id) {
            continue;
        }
        let mut rows = Vec::new();
        let mut velocities = Vec::new();
        let mut remaining = Decimal::ZERO;
        let mut first_open = None;
        for it in iters.iter().filter(|it| it.cadence == ci) {
            let items: Vec<usize> = (0..plan.tasks.len())
                .filter(|&i| leaf(i) && plan.tasks[i].iteration.as_deref() == Some(it.id.as_str()))
                .collect();
            let committed: Decimal = items.iter().map(|&i| points(i)).sum();
            let finished: Decimal = items.iter().filter(|&&i| done(i)).map(|&i| points(i)).sum();
            remaining += committed - finished;
            let ended = data.is_some_and(|d| it.end <= d);
            let state = match data {
                Some(d) if it.end <= d => "done",
                Some(d) if it.start < d => "current",
                _ => "planned",
            };
            if ended {
                velocities.push(finished);
            } else if first_open.is_none() {
                first_open = Some(it.index);
            }
            rows.push(IterationRow {
                id: it.id.clone(),
                start: start_date(it.start).to_string(),
                finish: finish_date(it.end).to_string(),
                state,
                committed: num(committed),
                done: num(finished),
                capacity: c.capacity.map(num),
                spi: if ended && !committed.is_zero() {
                    (finished / committed).round_dp(3).to_f64()
                } else {
                    None
                },
                items: items.len(),
            });
        }
        if plan.cadences.len() == 1 {
            remaining += backlog
                .iter()
                .filter(|&&i| !done(i))
                .map(|&i| points(i))
                .sum::<Decimal>();
        }
        let first = first_open.unwrap_or(c.count as usize);
        let end_after = |n: u32| -> String {
            let w = windows(plan, &cals, ci, first + n as usize);
            w.last()
                .map(|(_, e)| finish_date(*e).to_string())
                .unwrap_or_default()
        };
        let needed = |v: Decimal| -> Option<u32> {
            (v > Decimal::ZERO)
                .then(|| (remaining / v).ceil().to_u32().unwrap_or(u32::MAX).min(999))
        };
        let (forecast, note) = if velocities.is_empty() {
            (
                None,
                Some(if data.is_none() {
                    "no status date: set one with `alma progress date` to measure velocity".into()
                } else {
                    "no iteration has ended by the status date yet".into()
                }),
            )
        } else if remaining.is_zero() {
            (None, Some("no points left".into()))
        } else {
            let mean =
                velocities.iter().copied().sum::<Decimal>() / Decimal::from(velocities.len());
            let best = velocities.iter().copied().max().unwrap_or_default();
            let worst = velocities.iter().copied().min().unwrap_or_default();
            match needed(mean) {
                None => (None, Some("velocity is zero: nothing finished yet".into())),
                Some(n) => (
                    Some(Forecast {
                        remaining: num(remaining),
                        velocity: num(mean),
                        iterations: n,
                        finish: end_after(n),
                        optimistic: needed(best).map(|k| (k, end_after(k))),
                        pessimistic: needed(worst).map(|k| (k, end_after(k))),
                    }),
                    None,
                ),
            }
        };
        cadences.push(CadenceView {
            id: c.id.clone(),
            name: c.name.clone(),
            iterations: rows,
            velocities: velocities.into_iter().map(num).collect(),
            forecast,
            note,
        });
    }
    Ok(IterReport {
        schema: OUTPUT_SCHEMA,
        status_date: plan.project.status_date.map(|d| d.to_string()),
        cadences,
        backlog: num(backlog.iter().map(|&i| points(i)).sum()),
        backlog_items: backlog.len(),
    })
}

/// Points left in an iteration, day by day (knowledge notes, scheduling §5.7).
#[derive(Debug, Clone, Serialize)]
pub struct Burndown {
    /// Iteration id.
    pub iteration: String,
    /// Working days of the iteration.
    pub days: Vec<String>,
    /// Points committed to it.
    pub committed: f64,
    /// The ideal line: points left at the end of each day if work burned down evenly.
    pub ideal: Vec<f64>,
    /// Points left at the end of each day, through the status date.
    pub remaining: Vec<f64>,
}

/// The burndown of iteration `id`.
pub fn burndown(plan: &Plan, id: &str) -> Result<Burndown, String> {
    let cals = Calendars::new(plan);
    let index = PlanIndex::new(plan);
    let iters = iterations(plan, &cals);
    let it = iters.iter().find(|it| it.id == id).ok_or_else(|| {
        let ids: Vec<&str> = iters.iter().map(|i| i.id.as_str()).collect();
        format!("no iteration `{id}`; iterations: {}", ids.join(", "))
    })?;
    let c = &plan.cadences[it.cadence];
    let cal = c
        .calendar
        .as_deref()
        .and_then(|x| cals.by_id(x))
        .unwrap_or(cals.project());
    let work_days: Vec<i64> = (alma_core::time::day_of(it.start)
        ..=alma_core::time::day_of(it.end - 1))
        .filter(|&d| !cal.intervals(d).is_empty())
        .collect();
    let items: Vec<usize> = (0..plan.tasks.len())
        .filter(|&i| !index.is_summary(i) && plan.tasks[i].iteration.as_deref() == Some(id))
        .collect();
    let committed: Decimal = items
        .iter()
        .map(|&i| plan.tasks[i].points.unwrap_or_default())
        .sum();
    let n = Decimal::from(work_days.len().max(1));
    let status = plan.project.status_date;
    let mut ideal = Vec::new();
    let mut remaining = Vec::new();
    for (k, &d) in work_days.iter().enumerate() {
        let date = alma_core::time::date_of_day(d);
        ideal.push(num(committed - committed * Decimal::from(k + 1) / n));
        if status.is_some_and(|s| date <= s) {
            let done: Decimal = items
                .iter()
                .filter(|&&i| plan.tasks[i].actual_finish.is_some_and(|f| f <= date))
                .map(|&i| plan.tasks[i].points.unwrap_or_default())
                .sum();
            remaining.push(num(committed - done));
        }
    }
    Ok(Burndown {
        iteration: id.to_string(),
        days: work_days
            .iter()
            .map(|&d| alma_core::time::date_of_day(d).to_string())
            .collect(),
        committed: num(committed),
        ideal,
        remaining,
    })
}

/// Points planned and finished per iteration of a cadence, cumulative (knowledge notes,
/// scheduling §5.7).
#[derive(Debug, Clone, Serialize)]
pub struct Burnup {
    /// Cadence id.
    pub cadence: String,
    /// Iteration ids.
    pub iterations: Vec<String>,
    /// Cumulative points committed through each iteration.
    pub planned: Vec<f64>,
    /// Cumulative points finished through each iteration, for iterations that have started
    /// by the status date.
    pub done: Vec<f64>,
    /// Total scope: all points of the cadence, plus the backlog when the plan has one cadence.
    pub scope: f64,
}

/// The burnup of cadence `id` (the only cadence when `None`).
pub fn burnup(plan: &Plan, id: Option<&str>) -> Result<Burnup, String> {
    let r = report(plan, id)?;
    let c = match (id, r.cadences.as_slice()) {
        (_, [one]) => one,
        (None, []) => return Err("the plan has no cadence; add one with `alma iter add`".into()),
        _ => {
            let ids: Vec<&str> = r.cadences.iter().map(|c| c.id.as_str()).collect();
            return Err(format!(
                "the plan has several cadences; pick one with --cadence ({})",
                ids.join(", ")
            ));
        }
    };
    let mut planned = Vec::new();
    let mut done = Vec::new();
    let (mut p, mut d) = (0.0, 0.0);
    for it in &c.iterations {
        p += it.committed;
        d += it.done;
        planned.push(p);
        if it.state != "planned" {
            done.push(d);
        }
    }
    let backlog = if plan.cadences.len() == 1 {
        r.backlog
    } else {
        0.0
    };
    Ok(Burnup {
        cadence: c.id.clone(),
        iterations: c.iterations.iter().map(|i| i.id.clone()).collect(),
        planned,
        done,
        scope: p + backlog,
    })
}
