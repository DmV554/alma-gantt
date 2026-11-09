//! Iterations, stories and points (knowledge notes, scheduling §5.3–5.7; ADR 0021).
//!
//! A cadence generates equal iterations back to back: each starts at the first working minute
//! after the previous one ends and lasts `length` of working time on the cadence calendar.

use alma_core::model::Plan;
use alma_core::time::{Instant, start_of};

use crate::cpm::Calendars;

/// One generated iteration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Iteration {
    /// Id, `<prefix><nn>`.
    pub id: String,
    /// Index in `plan.cadences`.
    pub cadence: usize,
    /// Position in its cadence, from 0.
    pub index: usize,
    /// First working minute.
    pub start: Instant,
    /// End of its last working minute.
    pub end: Instant,
}

/// Every iteration of every cadence, cadence order then iteration order.
pub fn iterations(plan: &Plan, cals: &Calendars) -> Vec<Iteration> {
    let mut out = Vec::new();
    for (ci, c) in plan.cadences.iter().enumerate() {
        let w = windows(plan, cals, ci, c.count as usize);
        for (index, (id, (start, end))) in c.iteration_ids().into_iter().zip(w).enumerate() {
            out.push(Iteration {
                id,
                cadence: ci,
                index,
                start,
                end,
            });
        }
    }
    out
}

/// The first `n` iteration windows of cadence `ci`, also past its `count`: forecasts extend
/// the cadence.
pub fn windows(plan: &Plan, cals: &Calendars, ci: usize, n: usize) -> Vec<(Instant, Instant)> {
    let c = &plan.cadences[ci];
    let cal = c
        .calendar
        .as_deref()
        .and_then(|id| cals.by_id(id))
        .unwrap_or(cals.project());
    let mut start = cal.next_start(start_of(c.start));
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        let end = if c.length.elapsed {
            start + c.length.minutes
        } else {
            cal.add(start, c.length.minutes)
        };
        out.push((start, end));
        start = cal.next_start(end);
    }
    out
}
