//! Resource demand over a schedule: usage, over-allocation and histograms (knowledge notes,
//! resources §1.5; formulas F-COST-03 and F-COST-04).
//!
//! An assignment of `units` to a task demands `units` of the resource for every working minute
//! of the task. Only `work` and `equipment` resources have a capacity; `material` and `cost`
//! resources are consumed, not booked, and never over-allocate. A lag between two tasks
//! demands nothing (knowledge notes, resources §1.7).

use std::collections::HashMap;

use alma_core::model::{Plan, ResourceKind, TaskKind};
use alma_core::time::{Instant, start_of};
use jiff::civil::Date;
use rust_decimal::Decimal;

use crate::calendar::WorkCalendar;
use crate::cpm::{Calendars, Schedule};

/// Resources with a capacity, and the assignments that book them.
#[derive(Debug, Clone)]
pub struct Pool {
    /// Capacity per resource (`max_units`), `None` for resources that are not booked.
    pub max: Vec<Option<Decimal>>,
    /// Bookable assignments per task: (resource index, units), units > 0.
    pub assigned: Vec<Vec<(usize, Decimal)>>,
}

impl Pool {
    /// Builds the pool. Assignments to unknown resources are skipped (the parser reports them).
    pub fn new(plan: &Plan) -> Self {
        let by_id: HashMap<&str, usize> = plan
            .resources
            .iter()
            .enumerate()
            .map(|(i, r)| (r.id.as_str(), i))
            .collect();
        let max = plan
            .resources
            .iter()
            .map(|r| booked(r.kind).then_some(r.max_units))
            .collect::<Vec<_>>();
        let assigned = plan
            .tasks
            .iter()
            .map(|t| {
                t.assign
                    .iter()
                    .filter_map(|a| {
                        let r = *by_id.get(a.resource.as_str())?;
                        (max[r].is_some() && a.units > Decimal::ZERO).then_some((r, a.units))
                    })
                    .collect()
            })
            .collect();
        Self { max, assigned }
    }

    /// True when some task in `in_network` books a resource.
    pub fn any_assigned(&self, in_network: &[bool]) -> bool {
        self.assigned
            .iter()
            .zip(in_network)
            .any(|(a, &net)| net && !a.is_empty())
    }
}

/// True for resource kinds that have a capacity over time.
pub fn booked(kind: ResourceKind) -> bool {
    matches!(kind, ResourceKind::Work | ResourceKind::Equipment)
}

/// One task's demand on one resource: `units` over `[start, end)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Booking {
    /// Start.
    pub start: Instant,
    /// End (exclusive).
    pub end: Instant,
    /// Units.
    pub units: Decimal,
    /// Task index.
    pub task: usize,
}

/// Bookings per resource over a schedule, in task source order. Summaries count only when
/// they carry assignments themselves; milestones and zero-length tasks demand nothing.
pub fn bookings(plan: &Plan, schedule: &Schedule) -> Vec<Vec<Booking>> {
    let pool = Pool::new(plan);
    let mut out = vec![Vec::new(); plan.resources.len()];
    for (i, list) in pool.assigned.iter().enumerate() {
        let d = &schedule.tasks[i];
        if d.ef <= d.es || plan.tasks[i].kind == TaskKind::Milestone {
            continue;
        }
        for &(r, units) in list {
            out[r].push(Booking {
                start: d.es,
                end: d.ef,
                units,
                task: i,
            });
        }
    }
    out
}

/// A stretch of time when a resource is booked above its capacity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Overload {
    /// Resource index.
    pub resource: usize,
    /// Start of the stretch.
    pub from: Instant,
    /// End of the stretch (exclusive).
    pub to: Instant,
    /// Highest demand in the stretch, in units.
    pub peak: Decimal,
    /// Capacity.
    pub max: Decimal,
    /// Tasks booked during the stretch, source order.
    pub tasks: Vec<usize>,
}

/// Every over-allocation (F-COST-04 > 0), by resource then time. Demand is checked only on
/// working time of the tasks involved: two tasks on calendars that never overlap in working
/// time do not compete.
pub fn overloads(plan: &Plan, schedule: &Schedule) -> Vec<Overload> {
    let pool = Pool::new(plan);
    let cals = Calendars::new(plan);
    let mut out = Vec::new();
    for (r, list) in bookings(plan, schedule).iter().enumerate() {
        let Some(max) = pool.max[r] else { continue };
        let mut cuts: Vec<Instant> = list.iter().flat_map(|b| [b.start, b.end]).collect();
        cuts.sort_unstable();
        cuts.dedup();
        let mut current: Option<Overload> = None;
        for w in cuts.windows(2) {
            let (a, b) = (w[0], w[1]);
            let active: Vec<&Booking> = list.iter().filter(|x| x.start <= a && a < x.end).collect();
            // Demand counts only where the task is working.
            let load: Decimal = active
                .iter()
                .filter(|x| cals.of_task(plan, x.task).between(a, b) > 0)
                .map(|x| x.units)
                .sum();
            if load > max {
                let o = current.get_or_insert_with(|| Overload {
                    resource: r,
                    from: a,
                    to: b,
                    peak: load,
                    max,
                    tasks: Vec::new(),
                });
                o.to = b;
                o.peak = o.peak.max(load);
                for x in active {
                    if !o.tasks.contains(&x.task) {
                        o.tasks.push(x.task);
                    }
                }
            } else if let Some(o) = current.take() {
                out.push(o);
            }
        }
        out.extend(current);
    }
    for o in &mut out {
        o.tasks.sort_unstable();
    }
    out
}

/// A reporting period for histograms and cost curves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Period {
    /// Calendar day.
    Day,
    /// ISO week, Monday to Sunday.
    Week,
    /// Calendar month.
    Month,
}

impl Period {
    /// Parses `day`, `week` or `month`.
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "day" => Some(Self::Day),
            "week" => Some(Self::Week),
            "month" => Some(Self::Month),
            _ => None,
        }
    }

    /// The first day of the period that contains `date`.
    pub fn start_of(self, date: Date) -> Date {
        match self {
            Self::Day => date,
            Self::Week => {
                let back = i64::from(date.weekday().to_monday_zero_offset());
                date.checked_sub(jiff::Span::new().days(back))
                    .unwrap_or(date)
            }
            Self::Month => date.first_of_month(),
        }
    }

    /// The first day of the next period.
    pub fn next(self, start: Date) -> Date {
        let span = match self {
            Self::Day => jiff::Span::new().days(1),
            Self::Week => jiff::Span::new().weeks(1),
            Self::Month => jiff::Span::new().months(1),
        };
        start.checked_add(span).unwrap_or(start)
    }

    /// Consecutive periods covering `[from, to)`, as (first day, first day of the next one).
    pub fn buckets(self, from: Instant, to: Instant) -> Vec<(Date, Date)> {
        let mut out = Vec::new();
        if to <= from {
            return out;
        }
        let first = alma_core::time::date_of_day(alma_core::time::day_of(from));
        let mut d = self.start_of(first);
        while start_of(d) < to {
            let next = self.next(d);
            out.push((d, next));
            if out.len() > 100_000 {
                break;
            }
            d = next;
        }
        out
    }
}

/// One histogram bar (F-COST-03, F-COST-04): demand and capacity in working minutes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bar {
    /// First day of the period.
    pub start: Date,
    /// First day of the next period.
    pub end: Date,
    /// Booked demand, in unit-minutes (1 unit for 60 minutes = 60).
    pub demand: Decimal,
    /// Capacity, in unit-minutes: max_units × working minutes of the resource calendar.
    pub capacity: Decimal,
    /// Tasks with demand in the period, source order.
    pub tasks: Vec<usize>,
}

/// Demand and capacity of resource `r` per period, from the project start to the finish.
pub fn histogram(plan: &Plan, schedule: &Schedule, r: usize, period: Period) -> Vec<Bar> {
    let cals = Calendars::new(plan);
    let res_cal: &WorkCalendar = plan.resources[r]
        .calendar
        .as_ref()
        .and_then(|id| cals.by_id(id))
        .unwrap_or(cals.project());
    let max = plan.resources[r].max_units;
    let list = &bookings(plan, schedule)[r];
    let from = list
        .iter()
        .map(|b| b.start)
        .min()
        .unwrap_or(schedule.start)
        .min(schedule.start);
    let to = list
        .iter()
        .map(|b| b.end)
        .max()
        .unwrap_or(schedule.finish)
        .max(schedule.finish);
    period
        .buckets(from, to)
        .into_iter()
        .map(|(start, end)| {
            let (a, b) = (start_of(start), start_of(end));
            let mut demand = Decimal::ZERO;
            let mut tasks = Vec::new();
            for x in list {
                let (lo, hi) = (x.start.max(a), x.end.min(b));
                if lo >= hi {
                    continue;
                }
                let minutes = cals.of_task(plan, x.task).between(lo, hi);
                if minutes > 0 {
                    demand += x.units * Decimal::from(minutes);
                    if !tasks.contains(&x.task) {
                        tasks.push(x.task);
                    }
                }
            }
            tasks.sort_unstable();
            Bar {
                start,
                end,
                demand,
                capacity: max * Decimal::from(res_cal.between(a, b)),
                tasks,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn periods_start_on_monday_and_the_first_of_the_month() {
        let d = Date::constant(2027, 3, 10); // a Wednesday
        assert_eq!(Period::Week.start_of(d), Date::constant(2027, 3, 8));
        assert_eq!(Period::Month.start_of(d), Date::constant(2027, 3, 1));
        assert_eq!(
            Period::Month.next(Date::constant(2027, 1, 1)),
            Date::constant(2027, 2, 1)
        );
        let b = Period::Week.buckets(start_of(d), start_of(Date::constant(2027, 3, 16)));
        assert_eq!(b.len(), 2);
    }
}
