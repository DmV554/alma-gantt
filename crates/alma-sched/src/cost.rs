//! Costs of a scheduled plan and their distribution over time (SPEC.md §7.5, knowledge notes
//! resources §2–§5, formulas F-COST-07, F-COST-08 and F-COST-12, ADR 0012).
//!
//! - An assignment to a resource with a time rate (`per = hour | day | week | month`) costs
//!   `units × working hours × hourly rate` and accrues evenly over the task's working time.
//! - An assignment to a resource with `per = "use"` costs `units × amount` (a per-use fee, or a
//!   quantity × unit price for materials) and accrues at the task start.
//! - A fixed cost accrues at the start, at the end, or evenly (`accrue`).
//!
//! Lags cost nothing: costs only accrue inside tasks. Money is `Decimal`. Every amount is kept
//! to 12 decimal places (a rate per month divided into hours does not terminate), so sums and
//! period shares stay exact; rounding to cents is for display only.

use std::collections::HashMap;

use alma_core::duration::WorkUnits;
use alma_core::model::{Accrue, Plan, Rate, RatePer};
use alma_core::time::{Instant, start_of};
use jiff::civil::Date;
use rust_decimal::Decimal;

use crate::cpm::{Calendars, PlanIndex, Schedule};
use crate::resources::Period;

/// Decimal places kept in every amount.
pub const PRECISION: u32 = 12;

/// When a cost accrues.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Accrual {
    /// All at one instant.
    At(Instant),
    /// Evenly over the working time of task `task` between `from` and `to`.
    Spread {
        /// Start.
        from: Instant,
        /// End.
        to: Instant,
        /// Task whose calendar measures working time.
        task: usize,
    },
}

/// What a cost line pays for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// An assignment: index in `plan.resources`.
    Resource(usize),
    /// The task's fixed cost.
    Fixed,
}

/// One cost element (F-COST-07).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CostLine {
    /// Task index.
    pub task: usize,
    /// What it pays for.
    pub source: Source,
    /// Category: the fixed cost's `category`; `None` for assignments.
    pub category: Option<String>,
    /// Amount in project currency.
    pub amount: Decimal,
    /// Work booked, in hours (time-rated assignments only).
    pub work_h: Decimal,
    /// When it accrues.
    pub accrual: Accrual,
}

/// Costs of a schedule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Costs {
    /// Every cost element, task source order, then assignments in order, then the fixed cost.
    pub lines: Vec<CostLine>,
    /// Cost per task including its descendants, parallel to `plan.tasks`.
    pub task_total: Vec<Decimal>,
    /// Cost per resource, parallel to `plan.resources`.
    pub by_resource: Vec<Decimal>,
    /// Total.
    pub total: Decimal,
    /// Resources assigned somewhere but with no rate (their cost is zero), source order.
    pub unpriced: Vec<usize>,
}

/// Working hours in one `per` unit; `None` for `use`. A month is 52/12 weeks of working time
/// (our interpretation, ADR 0012).
pub fn hours_per(per: RatePer, units: WorkUnits) -> Option<Decimal> {
    let day = Decimal::from(units.minutes_per_day) / Decimal::from(60);
    let week = day * Decimal::from(units.days_per_week);
    match per {
        RatePer::Hour => Some(Decimal::ONE),
        RatePer::Day => Some(day),
        RatePer::Week => Some(week),
        RatePer::Month => Some(week * Decimal::from(52) / Decimal::from(12)),
        RatePer::Use => None,
    }
}

/// The hourly rate of a time rate.
pub fn hourly(rate: &Rate, units: WorkUnits) -> Option<Decimal> {
    hours_per(rate.per, units)
        .filter(|h| !h.is_zero())
        .map(|h| rate.amount / h)
}

/// Computes every cost of a scheduled plan.
pub fn costs(plan: &Plan, schedule: &Schedule) -> Costs {
    let cals = Calendars::new(plan);
    let units = plan.settings.units;
    let res: HashMap<&str, usize> = plan
        .resources
        .iter()
        .enumerate()
        .map(|(i, r)| (r.id.as_str(), i))
        .collect();
    let mut lines = Vec::new();
    let mut unpriced = Vec::new();
    for (i, t) in plan.tasks.iter().enumerate() {
        let d = &schedule.tasks[i];
        let minutes = if d.ef > d.es {
            cals.of_task(plan, i).between(d.es, d.ef)
        } else {
            0
        };
        let hours = Decimal::from(minutes) / Decimal::from(60);
        let spread = if minutes > 0 {
            Accrual::Spread {
                from: d.es,
                to: d.ef,
                task: i,
            }
        } else {
            Accrual::At(d.es)
        };
        for a in &t.assign {
            let Some(&r) = res.get(a.resource.as_str()) else {
                continue;
            };
            let Some(rate) = &plan.resources[r].rate else {
                if !unpriced.contains(&r) {
                    unpriced.push(r);
                }
                continue;
            };
            let line = match hourly(rate, units) {
                Some(h) => CostLine {
                    task: i,
                    source: Source::Resource(r),
                    category: None,
                    amount: (a.units * hours * h).round_dp(PRECISION),
                    work_h: a.units * hours,
                    accrual: spread,
                },
                None => CostLine {
                    task: i,
                    source: Source::Resource(r),
                    category: None,
                    amount: (a.units * rate.amount).round_dp(PRECISION),
                    work_h: Decimal::ZERO,
                    accrual: Accrual::At(d.es),
                },
            };
            lines.push(line);
        }
        if let Some(fc) = &t.fixed_cost {
            lines.push(CostLine {
                task: i,
                source: Source::Fixed,
                category: fc.category.clone(),
                amount: fc.amount.round_dp(PRECISION),
                work_h: Decimal::ZERO,
                accrual: match fc.accrue {
                    Accrue::Start => Accrual::At(d.es),
                    Accrue::End => Accrual::At(d.ef),
                    Accrue::Prorated => spread,
                },
            });
        }
    }
    unpriced.sort_unstable();

    let index = PlanIndex::new(plan);
    let mut own = vec![Decimal::ZERO; plan.tasks.len()];
    let mut by_resource = vec![Decimal::ZERO; plan.resources.len()];
    for l in &lines {
        own[l.task] += l.amount;
        if let Source::Resource(r) = l.source {
            by_resource[r] += l.amount;
        }
    }
    let task_total = (0..plan.tasks.len())
        .map(|i| subtree(&index, i).map(|j| own[j]).sum())
        .collect();
    let total = own.iter().copied().sum();
    Costs {
        lines,
        task_total,
        by_resource,
        total,
        unpriced,
    }
}

/// `i` and its descendants.
fn subtree(index: &PlanIndex, i: usize) -> impl Iterator<Item = usize> + '_ {
    let mut stack = vec![i];
    let mut steps = 0usize;
    std::iter::from_fn(move || {
        let n = stack.pop()?;
        steps += 1;
        if steps <= index.parent.len() {
            stack.extend(index.children[n].iter().rev());
        }
        Some(n)
    })
}

/// The part of `line` that accrues in `[a, b)` (F-COST-08). Shares telescope, so the parts of
/// consecutive periods add up exactly to the amount.
pub fn share(plan: &Plan, cals: &Calendars, line: &CostLine, a: Instant, b: Instant) -> Decimal {
    match line.accrual {
        Accrual::At(t) => {
            if a <= t && t < b {
                line.amount
            } else {
                Decimal::ZERO
            }
        }
        Accrual::Spread { from, to, task } => {
            let cal = cals.of_task(plan, task);
            let total = cal.between(from, to);
            if total <= 0 {
                return if a <= from && from < b {
                    line.amount
                } else {
                    Decimal::ZERO
                };
            }
            // Partial amounts are rounded to 12 decimal places, so differences and sums of
            // them stay exact and the shares of all periods add up to the amount.
            let upto = |t: Instant| -> Decimal {
                let done = cal.between(from, t.clamp(from, to));
                if done >= total {
                    line.amount
                } else {
                    (line.amount * Decimal::from(done) / Decimal::from(total)).round_dp(PRECISION)
                }
            };
            upto(b) - upto(a)
        }
    }
}

/// One period of the cost curve.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Point {
    /// First day.
    pub start: Date,
    /// First day of the next period.
    pub end: Date,
    /// Cost accrued in the period.
    pub cost: Decimal,
    /// Cumulative cost at the end of the period (F-COST-12).
    pub cumulative: Decimal,
}

/// Cost per period and cumulative, from the project start to the finish. From the current
/// schedule this is a forecast, not planned value (PV comes from a baseline).
pub fn curve(plan: &Plan, schedule: &Schedule, costs: &Costs, period: Period) -> Vec<Point> {
    curve_over(plan, schedule, costs, period, None)
}

/// Like [`curve`], over at least `[from, to)` as well (to line it up with a baseline).
pub fn curve_over(
    plan: &Plan,
    schedule: &Schedule,
    costs: &Costs,
    period: Period,
    widen: Option<(Instant, Instant)>,
) -> Vec<Point> {
    let cals = Calendars::new(plan);
    let mut first = costs
        .lines
        .iter()
        .map(|l| match l.accrual {
            Accrual::At(t) => t,
            Accrual::Spread { from, .. } => from,
        })
        .min()
        .unwrap_or(schedule.start)
        .min(schedule.start);
    // A cost at the finish instant (an end accrual) belongs to the period that contains it.
    let mut last = schedule.finish + 1;
    if let Some((a, b)) = widen {
        first = first.min(a);
        last = last.max(b);
    }
    let mut cumulative = Decimal::ZERO;
    period
        .buckets(first, last)
        .into_iter()
        .map(|(start, end)| {
            let (a, b) = (start_of(start), start_of(end));
            let cost: Decimal = costs
                .lines
                .iter()
                .map(|l| share(plan, &cals, l, a, b))
                .sum();
            cumulative += cost;
            Point {
                start,
                end,
                cost,
                cumulative,
            }
        })
        .collect()
}
