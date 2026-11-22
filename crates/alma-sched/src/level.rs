//! Resource leveling and smoothing (SPEC.md §7.4, knowledge notes resources §1.6, ADR 0011).
//!
//! A serial schedule generation scheme: tasks are placed one at a time, each as soon as its
//! predecessors are placed, picking among the ready tasks by priority (least total float, then
//! earliest late finish, then source order, all from the unleveled schedule). A task starts at
//! the earliest time its logic allows; when one of its resources would go above `max_units`, it
//! waits until the booking that blocks it ends, and tries again. The search is exact: the load
//! only drops when a booking ends, so no feasible start is skipped.
//!
//! - `leveling` may delay a task as far as needed, so the finish date may move.
//! - `smoothing` may delay a task only within its free float: its successors and the finish do
//!   not move. A task that does not fit stays at its logic date and is reported as unresolved.
//!
//! Tasks with `MSO`/`MFO` constraints, milestones and level-of-effort tasks are never moved, but
//! the first two still book their resources.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use alma_core::duration::Duration;
use alma_core::model::{ConstraintKind, Leveling, Plan};
use alma_core::time::Instant;
use rust_decimal::Decimal;

use crate::cpm::{Calendars, Driver, Edge};
use crate::resources::{Booking, Pool};

/// What leveling did to the schedule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LevelingReport {
    /// The mode used.
    pub mode: Leveling,
    /// Finish of the schedule before leveling.
    pub unleveled_finish: Instant,
    /// Tasks delayed, in source order.
    pub delays: Vec<Delay>,
    /// Tasks that could not be placed without over-allocation, in source order: a single
    /// assignment above `max_units`, or (smoothing) no room within free float.
    pub unresolved: Vec<usize>,
}

/// One delayed task.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Delay {
    /// Task index.
    pub task: usize,
    /// Resource that made it wait (index in `plan.resources`).
    pub resource: usize,
    /// Task holding that resource.
    pub by: usize,
    /// Start that logic alone allowed.
    pub from: Instant,
    /// Leveled start.
    pub to: Instant,
}

/// The logic network, from the unleveled schedule.
pub(crate) struct Sgs<'a> {
    pub(crate) order: &'a [usize],
    pub(crate) edges: &'a [Edge],
    pub(crate) incoming: &'a [Vec<usize>],
    pub(crate) outgoing: &'a [Vec<usize>],
    pub(crate) durations: &'a [Duration],
    pub(crate) total: &'a [Option<i64>],
    pub(crate) free: &'a [Option<i64>],
    pub(crate) lf: &'a [Instant],
}

/// Early dates being rewritten.
pub(crate) struct Early<'a> {
    pub(crate) es: &'a mut [Instant],
    pub(crate) ef: &'a mut [Instant],
    pub(crate) driver: &'a mut [Driver],
}

/// The logic-only earliest start of a task given the current early dates, and its driver.
pub(crate) type Earliest<'a> = dyn Fn(usize, &[Instant], &[Instant]) -> (Instant, Driver) + 'a;

/// Result of the pass.
pub(crate) struct Leveled {
    /// Delayed tasks, source order.
    pub(crate) delays: Vec<Delay>,
    /// Unresolved tasks, source order.
    pub(crate) unresolved: Vec<usize>,
    /// The order tasks were placed in: topological for logic links and for the resource links
    /// implied by `delays`.
    pub(crate) placed: Vec<usize>,
}

/// Runs the pass.
pub(crate) fn level(
    plan: &Plan,
    cals: &Calendars,
    pool: &Pool,
    net: &Sgs<'_>,
    early: &mut Early<'_>,
    earliest: &Earliest<'_>,
    dur_add: &dyn Fn(usize, Instant) -> Instant,
) -> Leveled {
    let smoothing = plan.settings.leveling == Leveling::Smoothing;
    let unleveled_es: Vec<Instant> = early.es.to_vec();
    let mut indegree: Vec<usize> = net.incoming.iter().map(Vec::len).collect();
    let key = |j: usize| Reverse((net.total[j].unwrap_or(0), net.lf[j], j));
    let mut ready: BinaryHeap<Reverse<(i64, Instant, usize)>> = net
        .order
        .iter()
        .filter(|&&j| indegree[j] == 0)
        .map(|&j| key(j))
        .collect();
    let mut book: Vec<Vec<Booking>> = vec![Vec::new(); pool.max.len()];
    let mut delays = Vec::new();
    let mut unresolved = Vec::new();
    let mut placed = Vec::with_capacity(net.order.len());

    while let Some(Reverse((_, _, j))) = ready.pop() {
        placed.push(j);
        let (logic, logic_why) = earliest(j, early.es, early.ef);
        let mut start = logic;
        let mut why = logic_why;
        let assigned = &pool.assigned[j];
        let fixed = plan.tasks[j]
            .constraint
            .as_ref()
            .is_some_and(|c| matches!(c.kind, ConstraintKind::Mso | ConstraintKind::Mfo));
        if net.durations[j].minutes > 0 && !assigned.is_empty() && !fixed {
            let cal = cals.of_task(plan, j);
            let too_big = assigned
                .iter()
                .any(|&(r, u)| pool.max[r].is_some_and(|m| u > m));
            let limit = smoothing.then(|| cal.add(unleveled_es[j], net.free[j].unwrap_or(0)));
            if too_big {
                unresolved.push(j);
            } else {
                let mut blocked = None;
                loop {
                    let finish = dur_add(j, start);
                    let Some((r, by, release)) = conflict(&book, pool, assigned, start, finish)
                    else {
                        break;
                    };
                    let next = cal.next_start(release.max(start + 1));
                    if limit.is_some_and(|l| next > l) {
                        unresolved.push(j);
                        start = logic;
                        blocked = None;
                        break;
                    }
                    start = next;
                    blocked = Some((r, by));
                }
                if let Some((resource, by)) = blocked {
                    why = Driver::Leveling { resource, by };
                    delays.push(Delay {
                        task: j,
                        resource,
                        by,
                        from: logic,
                        to: start,
                    });
                }
            }
        }
        early.es[j] = start;
        early.ef[j] = dur_add(j, start);
        early.driver[j] = why;
        if early.ef[j] > early.es[j] {
            for &(r, units) in assigned {
                book[r].push(Booking {
                    start: early.es[j],
                    end: early.ef[j],
                    units,
                    task: j,
                });
            }
        }
        for &e in &net.outgoing[j] {
            let s = net.edges[e].succ;
            indegree[s] -= 1;
            if indegree[s] == 0 {
                ready.push(key(s));
            }
        }
    }
    delays.sort_by_key(|d| d.task);
    unresolved.sort_unstable();
    Leveled {
        delays,
        unresolved,
        placed,
    }
}

/// The first resource of `assigned` that would go above capacity over `[start, finish)`, with
/// the task whose booking ends first among those in the way and when it ends.
fn conflict(
    book: &[Vec<Booking>],
    pool: &Pool,
    assigned: &[(usize, Decimal)],
    start: Instant,
    finish: Instant,
) -> Option<(usize, usize, Instant)> {
    let mut found: Option<(usize, usize, Instant)> = None;
    for &(r, units) in assigned {
        let Some(max) = pool.max[r] else { continue };
        let overlapping: Vec<&Booking> = book[r]
            .iter()
            .filter(|b| b.start < finish && b.end > start)
            .collect();
        let peak = overlapping
            .iter()
            .map(|p| p.start.max(start))
            .map(|at| {
                overlapping
                    .iter()
                    .filter(|b| b.start <= at && at < b.end)
                    .map(|b| b.units)
                    .sum::<Decimal>()
            })
            .max()
            .unwrap_or(Decimal::ZERO);
        if peak + units > max
            && let Some(first) = overlapping.iter().min_by_key(|b| (b.end, b.task))
            && found.is_none_or(|(_, _, end)| first.end < end)
        {
            found = Some((r, first.task, first.end));
        }
    }
    found
}
