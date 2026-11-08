//! Resource queries: `alma res ls`, `alma res usage`, and the leveling summary that `alma level`
//! prints (SPEC.md §7.4, knowledge notes resources §1.5–§1.6).

use alma_core::model::Plan;
use alma_core::time::{Instant, finish_date, start_date};
use alma_sched::Schedule;
use alma_sched::cpm::Calendars;
use alma_sched::resources::{Overload, Period, bookings, histogram, overloads};
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use serde::Serialize;

use crate::OUTPUT_SCHEMA;

fn num(d: Decimal) -> f64 {
    d.round_dp(2).to_f64().unwrap_or(0.0)
}

fn hours(unit_minutes: Decimal) -> f64 {
    num(unit_minutes / Decimal::from(60))
}

/// An over-allocation, as shown to agents.
#[derive(Debug, Clone, Serialize)]
pub struct OverloadView {
    /// Resource id.
    pub resource: String,
    /// First day.
    pub from: String,
    /// Last day.
    pub to: String,
    /// Highest demand, in units.
    pub peak: f64,
    /// Capacity.
    pub max: f64,
    /// Competing tasks.
    pub tasks: Vec<String>,
}

fn overload_view(plan: &Plan, o: &Overload) -> OverloadView {
    OverloadView {
        resource: plan.resources[o.resource].id.clone(),
        from: start_date(o.from).to_string(),
        to: finish_date(o.to).to_string(),
        peak: num(o.peak),
        max: num(o.max),
        tasks: o.tasks.iter().map(|&t| plan.tasks[t].id.clone()).collect(),
    }
}

/// One resource in `alma res ls`.
#[derive(Debug, Clone, Serialize)]
pub struct ResourceView {
    /// Id.
    pub id: String,
    /// Name.
    pub name: String,
    /// Kind.
    pub kind: String,
    /// Capacity.
    pub max_units: f64,
    /// Rate, as `AMOUNT/PER`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate: Option<String>,
    /// Tasks assigned, source order.
    pub tasks: Vec<String>,
    /// Total booked work, in hours.
    pub work_h: f64,
    /// Number of over-allocated stretches.
    pub over_allocations: usize,
}

/// `alma res ls`.
pub fn list(plan: &Plan, schedule: &Schedule) -> Vec<ResourceView> {
    let cals = Calendars::new(plan);
    let books = bookings(plan, schedule);
    let over = overloads(plan, schedule);
    plan.resources
        .iter()
        .enumerate()
        .map(|(r, res)| {
            let tasks = plan
                .tasks
                .iter()
                .filter(|t| t.assign.iter().any(|a| a.resource == res.id))
                .map(|t| t.id.clone())
                .collect();
            let work: Decimal = books[r]
                .iter()
                .map(|b| {
                    b.units * Decimal::from(cals.of_task(plan, b.task).between(b.start, b.end))
                })
                .sum();
            ResourceView {
                id: res.id.clone(),
                name: res.name.clone(),
                kind: res.kind.as_str().into(),
                max_units: num(res.max_units),
                rate: res
                    .rate
                    .as_ref()
                    .map(|r| format!("{}/{}", r.amount.normalize(), r.per.as_str())),
                tasks,
                work_h: hours(work),
                over_allocations: over.iter().filter(|o| o.resource == r).count(),
            }
        })
        .collect()
}

/// One period of `alma res usage`.
#[derive(Debug, Clone, Serialize)]
pub struct BarView {
    /// First day of the period.
    pub start: String,
    /// Last day of the period.
    pub end: String,
    /// Booked work, in hours.
    pub demand_h: f64,
    /// Available hours (max_units × working hours of the resource calendar).
    pub capacity_h: f64,
    /// Demand above capacity, in hours (F-COST-04 summed over the period). A period can be
    /// over-allocated on some days even when this is 0: see `over_allocated`.
    pub over_h: f64,
    /// Tasks with demand in the period.
    pub tasks: Vec<String>,
}

/// Result of `alma res usage`.
#[derive(Debug, Clone, Serialize)]
pub struct UsageView {
    /// Output schema version.
    pub schema: u32,
    /// Resource id.
    pub resource: String,
    /// Capacity in units.
    pub max_units: f64,
    /// `day`, `week` or `month`.
    pub period: String,
    /// One entry per period, from the project start to the finish.
    pub periods: Vec<BarView>,
    /// Stretches booked above capacity, exact to the minute.
    pub over_allocated: Vec<OverloadView>,
}

/// `alma res usage`: demand vs capacity of one resource per period (F-COST-03, F-COST-04).
pub fn usage(
    plan: &Plan,
    schedule: &Schedule,
    id: &str,
    period: &str,
) -> Result<UsageView, String> {
    let r = plan
        .resources
        .iter()
        .position(|x| x.id == id)
        .ok_or_else(|| {
            let ids: Vec<&str> = plan.resources.iter().map(|x| x.id.as_str()).collect();
            format!("no resource `{id}`; resources: {}", ids.join(", "))
        })?;
    let p = Period::parse(period)
        .ok_or_else(|| format!("period `{period}` must be day, week or month"))?;
    let periods = histogram(plan, schedule, r, p)
        .into_iter()
        .map(|b| {
            let last = b
                .end
                .checked_sub(jiff::Span::new().days(1))
                .unwrap_or(b.end);
            BarView {
                start: b.start.to_string(),
                end: last.to_string(),
                demand_h: hours(b.demand),
                capacity_h: hours(b.capacity),
                over_h: hours((b.demand - b.capacity).max(Decimal::ZERO)),
                tasks: b.tasks.iter().map(|&t| plan.tasks[t].id.clone()).collect(),
            }
        })
        .collect();
    Ok(UsageView {
        schema: OUTPUT_SCHEMA,
        resource: plan.resources[r].id.clone(),
        max_units: num(plan.resources[r].max_units),
        period: period.into(),
        periods,
        over_allocated: overloads(plan, schedule)
            .iter()
            .filter(|o| o.resource == r)
            .map(|o| overload_view(plan, o))
            .collect(),
    })
}

/// A task delayed by leveling.
#[derive(Debug, Clone, Serialize)]
pub struct DelayView {
    /// Task id.
    pub id: String,
    /// Resource it waited for.
    pub resource: String,
    /// Task that held the resource.
    pub waits_for: String,
    /// Start from logic alone.
    pub from: String,
    /// Leveled start.
    pub to: String,
    /// Delay in working days on the task calendar.
    pub days: f64,
}

/// What leveling did (the audit trail of SPEC.md §7.4).
#[derive(Debug, Clone, Serialize)]
pub struct LevelingView {
    /// `off`, `smoothing` or `leveling`.
    pub mode: String,
    /// Project finish with logic alone.
    pub unleveled_finish: String,
    /// Project finish now.
    pub finish: String,
    /// Delayed tasks, source order.
    pub delayed: Vec<DelayView>,
    /// Tasks that could not be placed without over-allocation.
    pub unresolved: Vec<String>,
    /// Over-allocation left in the schedule.
    pub over_allocated: Vec<OverloadView>,
}

/// The leveling summary of a schedule.
pub fn leveling(plan: &Plan, schedule: &Schedule) -> LevelingView {
    let cals = Calendars::new(plan);
    let day = Decimal::from(plan.settings.units.minutes_per_day);
    let date = |t: Instant| finish_date(t).to_string();
    let (delayed, unresolved, unleveled) = match &schedule.leveling {
        Some(r) => (
            r.delays
                .iter()
                .map(|d| DelayView {
                    id: plan.tasks[d.task].id.clone(),
                    resource: plan.resources[d.resource].id.clone(),
                    waits_for: plan.tasks[d.by].id.clone(),
                    from: start_date(d.from).to_string(),
                    to: start_date(d.to).to_string(),
                    days: num(
                        Decimal::from(cals.of_task(plan, d.task).between(d.from, d.to)) / day,
                    ),
                })
                .collect(),
            r.unresolved
                .iter()
                .map(|&t| plan.tasks[t].id.clone())
                .collect(),
            r.unleveled_finish,
        ),
        None => (Vec::new(), Vec::new(), schedule.finish),
    };
    LevelingView {
        mode: plan.settings.leveling.as_str().into(),
        unleveled_finish: date(unleveled),
        finish: date(schedule.finish),
        delayed,
        unresolved,
        over_allocated: overloads(plan, schedule)
            .iter()
            .map(|o| overload_view(plan, o))
            .collect(),
    }
}
