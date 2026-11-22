//! Critical path method (SPEC.md §7.2, formulas F-SCH-06 to F-SCH-11).
//!
//! The network holds leaf tasks except level-of-effort tasks. A dependency on a summary applies
//! to its leaf descendants (flagged by `L-DEP-09`). Summaries roll up from their leaves; a
//! level-of-effort task takes its dates from the tasks it spans.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};

use alma_core::diag::Diagnostic;
use alma_core::duration::Duration;
use alma_core::model::{
    Calendar, ConstraintKind, CriticalDefinition, DepType, Dist, LagCalendar, LateAnchor, Leveling,
    Plan, ResourceCalendars, SchedType, TaskKind,
};
use alma_core::time::{DAY, Instant, start_of};
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;

use crate::calendar::WorkCalendar;
pub use crate::level::{Delay, LevelingReport};
use crate::level::{Early, Leveled, Sgs, level};
use crate::resources::Pool;

/// What set a task's early start (F-SCH-11).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Driver {
    /// The project start.
    ProjectStart,
    /// A dependency: `task` owns the link (it may be a summary), `dep` indexes its `deps`,
    /// `pred` is the leaf predecessor that drove the date.
    Dep {
        /// Task whose `deps` holds the link.
        task: usize,
        /// Index in that task's `deps`.
        dep: usize,
        /// Leaf predecessor.
        pred: usize,
    },
    /// The task's own constraint.
    Constraint(ConstraintKind),
    /// Rolled up from children (summaries).
    Rollup,
    /// Derived from the spanned tasks (level of effort).
    Span,
    /// Delayed by resource leveling: `resource` (index in `plan.resources`) was busy with
    /// task `by` until this start.
    Leveling {
        /// Resource index.
        resource: usize,
        /// The task holding the resource.
        by: usize,
    },
}

/// Computed dates and floats for one task. Floats are working minutes on the task's calendar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskDates {
    /// Early start.
    pub es: Instant,
    /// Early finish.
    pub ef: Instant,
    /// Late start.
    pub ls: Instant,
    /// Late finish.
    pub lf: Instant,
    /// Total float (F-SCH-08); `None` for level-of-effort tasks.
    pub total_float: Option<i64>,
    /// Free float (F-SCH-09); `None` for summaries and level-of-effort tasks.
    pub free_float: Option<i64>,
    /// On the critical set (F-SCH-10).
    pub critical: bool,
    /// What set the early start.
    pub driver: Driver,
    /// Resolved duration (zero for summaries).
    pub duration: Duration,
    /// True for leaf tasks in the logic network.
    pub in_network: bool,
}

/// The result of scheduling a plan. `tasks` is parallel to `plan.tasks`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Schedule {
    /// Dates per task, in source order.
    pub tasks: Vec<TaskDates>,
    /// Project start instant (first working minute).
    pub start: Instant,
    /// Project early finish.
    pub finish: Instant,
    /// Backward pass anchor.
    pub anchor: Instant,
    /// Diagnostics found while scheduling (they never stop it).
    pub diagnostics: Vec<Diagnostic>,
    /// What leveling did, when `settings.schedule.leveling` is on and some task has a
    /// leveled resource.
    pub leveling: Option<LevelingReport>,
}

/// One expanded link between two network leaves.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Edge {
    pub(crate) pred: usize,
    pub(crate) succ: usize,
    kind: DepType,
    lag: Duration,
    owner: usize,
    dep: usize,
}

/// Late dates and floats over one set of early dates.
struct Late {
    finish: Instant,
    anchor: Instant,
    ls: Vec<Instant>,
    lf: Vec<Instant>,
    total: Vec<Option<i64>>,
    free: Vec<Option<i64>>,
}

/// Index structures over a plan, shared with lint.
#[derive(Debug, Clone)]
pub struct PlanIndex {
    /// Task id → index.
    pub by_id: HashMap<String, usize>,
    /// Parent index per task.
    pub parent: Vec<Option<usize>>,
    /// Children per task, source order.
    pub children: Vec<Vec<usize>>,
}

impl PlanIndex {
    /// Builds the index. Unknown parents are ignored (the parser reports them).
    pub fn new(plan: &Plan) -> Self {
        let by_id: HashMap<String, usize> = plan
            .tasks
            .iter()
            .enumerate()
            .map(|(i, t)| (t.id.clone(), i))
            .collect();
        let mut parent = vec![None; plan.tasks.len()];
        let mut children = vec![Vec::new(); plan.tasks.len()];
        for (i, t) in plan.tasks.iter().enumerate() {
            if let Some(p) = t.parent.as_ref().and_then(|p| by_id.get(p)) {
                parent[i] = Some(*p);
                children[*p].push(i);
            }
        }
        Self {
            by_id,
            parent,
            children,
        }
    }

    /// True when the task has children.
    pub fn is_summary(&self, i: usize) -> bool {
        !self.children[i].is_empty()
    }

    /// Leaf descendants of `i` (or `i` itself when it is a leaf), source order.
    pub fn leaves(&self, i: usize) -> Vec<usize> {
        let mut out = Vec::new();
        let mut stack = vec![i];
        while let Some(n) = stack.pop() {
            if self.children[n].is_empty() {
                out.push(n);
            } else {
                stack.extend(self.children[n].iter().rev());
            }
        }
        out
    }

    /// True when `a` is `b` or one of its ancestors.
    pub fn is_ancestor_or_self(&self, a: usize, b: usize) -> bool {
        let mut cur = Some(b);
        let mut steps = 0;
        while let Some(n) = cur {
            if n == a {
                return true;
            }
            cur = self.parent[n];
            steps += 1;
            if steps > self.parent.len() {
                return false;
            }
        }
        false
    }

    /// Depth of a task (WBS level 1 = 0).
    pub fn depth(&self, i: usize) -> usize {
        let mut d = 0;
        let mut cur = self.parent[i];
        while let Some(p) = cur {
            d += 1;
            cur = self.parent[p];
            if d > self.parent.len() {
                break;
            }
        }
        d
    }
}

/// Expected value of a three-point estimate, in whole minutes rounded up (F-SCH-03/04).
pub fn expected_minutes(o: i64, m: i64, p: i64, dist: Dist) -> i64 {
    match dist {
        Dist::Beta => (o + 4 * m + p + 5).div_euclid(6),
        Dist::Triangular => (o + m + p + 2).div_euclid(3),
    }
}

/// Resolved duration of a leaf task.
pub fn resolve_duration(plan: &Plan, i: usize) -> Duration {
    let t = &plan.tasks[i];
    match t.kind {
        TaskKind::Milestone | TaskKind::Loe => return Duration::ZERO,
        TaskKind::Task | TaskKind::Buffer => {}
    }
    if t.sched_type == SchedType::FixedEffort
        && let Some(effort) = t.effort
    {
        let units: Decimal = t.assign.iter().map(|a| a.units).sum();
        if units > Decimal::ZERO {
            let minutes = (Decimal::from(effort.minutes) / units).ceil();
            return Duration::working(minutes.to_i64().unwrap_or(i64::MAX / 4));
        }
    }
    if let Some(d) = t.duration {
        return d;
    }
    if let Some(e) = &t.estimate {
        let dist = e.dist.unwrap_or(plan.settings.three_point);
        return Duration {
            minutes: expected_minutes(e.o.minutes, e.m.minutes, e.p.minutes, dist),
            elapsed: e.m.elapsed,
        };
    }
    Duration::ZERO
}

/// Compiled calendars of a plan, including the working time each task shares with the
/// resources assigned to it (ADR 0017).
pub struct Calendars {
    project: WorkCalendar,
    by_id: HashMap<String, WorkCalendar>,
    /// Per task, an index into `combined` when resource calendars narrow its working time.
    task: Vec<Option<usize>>,
    combined: Vec<WorkCalendar>,
    /// (task, resource) pairs whose calendars share no working time; the task keeps its own.
    pub disjoint: Vec<(usize, usize)>,
}

impl Calendars {
    /// Compiles every calendar in the plan.
    pub fn new(plan: &Plan) -> Self {
        let by_id: HashMap<String, WorkCalendar> = plan
            .calendars
            .iter()
            .map(|c| (c.id.clone(), WorkCalendar::new(c)))
            .collect();
        let project = plan
            .project
            .calendar
            .as_ref()
            .and_then(|id| by_id.get(id).cloned())
            .unwrap_or_else(|| WorkCalendar::new(&Calendar::standard()));
        let mut task = vec![None; plan.tasks.len()];
        let mut combined: Vec<WorkCalendar> = Vec::new();
        let mut cache: HashMap<(Option<String>, Vec<String>), Option<usize>> = HashMap::new();
        let mut disjoint = Vec::new();
        if plan.settings.resource_calendars == ResourceCalendars::Intersect {
            let res: HashMap<&str, usize> = plan
                .resources
                .iter()
                .enumerate()
                .map(|(i, r)| (r.id.as_str(), i))
                .collect();
            for (i, t) in plan.tasks.iter().enumerate() {
                let mut with_cal: Vec<(usize, String)> = t
                    .assign
                    .iter()
                    .filter_map(|a| res.get(a.resource.as_str()).copied())
                    .filter_map(|r| {
                        let c = plan.resources[r].calendar.clone()?;
                        by_id.contains_key(&c).then_some((r, c))
                    })
                    .collect();
                if with_cal.is_empty() {
                    continue;
                }
                with_cal.sort_by(|a, b| a.1.cmp(&b.1));
                let base_id = t.calendar.clone().filter(|c| by_id.contains_key(c));
                let key = (
                    base_id.clone(),
                    with_cal.iter().map(|(_, c)| c.clone()).collect::<Vec<_>>(),
                );
                let slot = if let Some(s) = cache.get(&key) {
                    *s
                } else {
                    let base = base_id
                        .as_ref()
                        .and_then(|c| by_id.get(c))
                        .unwrap_or(&project);
                    let mut cal = base.clone();
                    for (_, c) in &with_cal {
                        cal = cal.intersect(&by_id[c]);
                    }
                    let s = cal.has_work().then(|| {
                        combined.push(cal);
                        combined.len() - 1
                    });
                    cache.insert(key, s);
                    s
                };
                match slot {
                    Some(s) => task[i] = Some(s),
                    None => {
                        // Name the resources that leave no shared time with the task alone.
                        let base = base_id
                            .as_ref()
                            .and_then(|c| by_id.get(c))
                            .unwrap_or(&project);
                        for (r, c) in &with_cal {
                            if !base.intersect(&by_id[c]).has_work() {
                                disjoint.push((i, *r));
                            }
                        }
                        if !disjoint.iter().any(|(t, _)| *t == i) {
                            disjoint.push((i, with_cal[0].0));
                        }
                    }
                }
            }
        }
        Self {
            project,
            by_id,
            task,
            combined,
            disjoint,
        }
    }

    /// The project calendar.
    pub fn project(&self) -> &WorkCalendar {
        &self.project
    }

    /// A calendar by id.
    pub fn by_id(&self, id: &str) -> Option<&WorkCalendar> {
        self.by_id.get(id)
    }

    /// The calendar of a task (its override, or the project calendar).
    pub fn of_task(&self, plan: &Plan, i: usize) -> &WorkCalendar {
        if let Some(Some(k)) = self.task.get(i) {
            return &self.combined[*k];
        }
        plan.tasks[i]
            .calendar
            .as_ref()
            .and_then(|id| self.by_id.get(id))
            .unwrap_or(&self.project)
    }
}

/// Schedules a plan. Returns `Err` with `E-DEP-01`/`E-DEP-02` when the logic has a cycle.
pub fn schedule(plan: &Plan) -> Result<Schedule, Vec<Diagnostic>> {
    let n = plan.tasks.len();
    let index = PlanIndex::new(plan);
    let cals = Calendars::new(plan);
    let mut diagnostics = Vec::new();

    let durations: Vec<Duration> = (0..n).map(|i| resolve_duration(plan, i)).collect();
    let in_network: Vec<bool> = (0..n)
        .map(|i| !index.is_summary(i) && plan.tasks[i].kind != TaskKind::Loe)
        .collect();

    // Expand links over summaries; `edges` is in (owner, dep, pred leaf, succ leaf) order.
    let mut edges = Vec::new();
    let mut hard_errors = Vec::new();
    for (owner, task) in plan.tasks.iter().enumerate() {
        for (dep_idx, dep) in task.deps.iter().enumerate() {
            let Some(&pred) = index.by_id.get(&dep.pred) else {
                continue;
            };
            if index.is_ancestor_or_self(pred, owner) || index.is_ancestor_or_self(owner, pred) {
                hard_errors.push(
                    Diagnostic::error(
                        "E-DEP-02",
                        format!(
                            "`{}` depends on `{}`, which is itself or in its own branch",
                            task.id, dep.pred
                        ),
                    )
                    .with_ids([task.id.clone(), dep.pred.clone()])
                    .with_fix(format!("alma dep rm {} {}", dep.pred, task.id)),
                );
                continue;
            }
            let succs: Vec<usize> = index
                .leaves(owner)
                .into_iter()
                .filter(|&s| in_network[s])
                .collect();
            let preds: Vec<usize> = index
                .leaves(pred)
                .into_iter()
                .filter(|&p| in_network[p])
                .collect();
            for &p in &preds {
                for &s in &succs {
                    edges.push(Edge {
                        pred: p,
                        succ: s,
                        kind: dep.kind,
                        lag: dep.lag,
                        owner,
                        dep: dep_idx,
                    });
                }
            }
        }
    }
    if !hard_errors.is_empty() {
        return Err(hard_errors);
    }

    let mut incoming: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut outgoing: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (e, edge) in edges.iter().enumerate() {
        incoming[edge.succ].push(e);
        outgoing[edge.pred].push(e);
    }

    let order = topo_order(plan, &in_network, &edges, &incoming, &outgoing)?;

    let lag_cal = |edge: &Edge| -> &WorkCalendar {
        match plan.settings.lag_calendar {
            LagCalendar::Project => cals.project(),
            LagCalendar::Predecessor => cals.of_task(plan, edge.pred),
            LagCalendar::Successor => cals.of_task(plan, edge.succ),
        }
    };
    let lag_add = |edge: &Edge, t: Instant| -> Instant {
        if edge.lag.elapsed {
            t + edge.lag.minutes
        } else {
            lag_cal(edge).add(t, edge.lag.minutes)
        }
    };
    let lag_sub = |edge: &Edge, t: Instant| -> Instant {
        if edge.lag.elapsed {
            t - edge.lag.minutes
        } else {
            lag_cal(edge).sub(t, edge.lag.minutes)
        }
    };
    let dur_add = |i: usize, t: Instant| -> Instant {
        let d = durations[i];
        if d.elapsed {
            t + d.minutes
        } else {
            cals.of_task(plan, i).add(t, d.minutes)
        }
    };
    let dur_sub = |i: usize, t: Instant| -> Instant {
        let d = durations[i];
        if d.elapsed {
            t - d.minutes
        } else {
            cals.of_task(plan, i).sub(t, d.minutes)
        }
    };
    let date_start = |i: usize, date: jiff::civil::Date| -> Instant {
        cals.of_task(plan, i).next_start(start_of(date))
    };
    let date_end = |i: usize, date: jiff::civil::Date| -> Instant {
        cals.of_task(plan, i).prev_end(start_of(date) + DAY)
    };

    let project_start = cals.project().next_start(start_of(plan.project.start));

    // The earliest start that logic and constraints allow (F-SCH-06), before snapping.
    let earliest = |j: usize, es: &[Instant], ef: &[Instant]| -> (Instant, Driver) {
        let mut best = start_of(plan.project.start);
        let mut why = Driver::ProjectStart;
        for &e in &incoming[j] {
            let edge = &edges[e];
            let term = match edge.kind {
                DepType::Fs => lag_add(edge, ef[edge.pred]),
                DepType::Ss => lag_add(edge, es[edge.pred]),
                DepType::Ff => dur_sub(j, lag_add(edge, ef[edge.pred])),
                DepType::Sf => dur_sub(j, lag_add(edge, es[edge.pred])),
            };
            if term > best {
                best = term;
                why = Driver::Dep {
                    task: edge.owner,
                    dep: edge.dep,
                    pred: edge.pred,
                };
            }
        }
        if let Some(c) = &plan.tasks[j].constraint
            && let Some(date) = c.date
        {
            match c.kind {
                ConstraintKind::Snet => {
                    let t = date_start(j, date);
                    if t > best {
                        best = t;
                        why = Driver::Constraint(c.kind);
                    }
                }
                ConstraintKind::Fnet => {
                    let t = dur_sub(j, date_end(j, date));
                    if t > best {
                        best = t;
                        why = Driver::Constraint(c.kind);
                    }
                }
                ConstraintKind::Mso => {
                    best = date_start(j, date);
                    why = Driver::Constraint(c.kind);
                }
                ConstraintKind::Mfo => {
                    best = dur_sub(j, date_end(j, date));
                    why = Driver::Constraint(c.kind);
                }
                _ => {}
            }
        }
        let snap = durations[j].minutes > 0 || !matches!(why, Driver::Dep { .. });
        let start = if snap {
            cals.of_task(plan, j).next_start(best)
        } else {
            best
        };
        (start, why)
    };

    // Forward pass (F-SCH-06).
    let mut es = vec![project_start; n];
    let mut ef = vec![project_start; n];
    let mut driver = vec![Driver::ProjectStart; n];
    for &j in &order {
        let (start, why) = earliest(j, &es, &ef);
        es[j] = start;
        ef[j] = dur_add(j, start);
        driver[j] = why;
    }

    // Deadlines on summaries apply to their leaves.
    let mut deadline: Vec<Option<Instant>> = vec![None; n];
    for (i, t) in plan.tasks.iter().enumerate() {
        if let Some(d) = t.deadline {
            for leaf in index.leaves(i) {
                let end = date_end(leaf, d);
                deadline[leaf] = Some(deadline[leaf].map_or(end, |x: Instant| x.min(end)));
            }
        }
    }

    // Backward pass (F-SCH-07) and floats (F-SCH-08, F-SCH-09) over given early dates.
    let late = |es: &[Instant],
                ef: &[Instant],
                order: &[usize],
                edges: &[Edge],
                outgoing: &[Vec<usize>]|
     -> Late {
        let finish = order.iter().map(|&i| ef[i]).max().unwrap_or(project_start);
        let anchor = match (plan.settings.late_anchor, plan.project.deadline) {
            (LateAnchor::Deadline, Some(d)) => cals.project().prev_end(start_of(d) + DAY),
            _ => finish,
        };
        let mut ls = vec![anchor; n];
        let mut lf = vec![anchor; n];
        for &i in order.iter().rev() {
            let mut best = anchor;
            for &e in &outgoing[i] {
                let edge = &edges[e];
                let term = match edge.kind {
                    DepType::Fs => lag_sub(edge, ls[edge.succ]),
                    DepType::Ss => dur_add(i, lag_sub(edge, ls[edge.succ])),
                    DepType::Ff => lag_sub(edge, lf[edge.succ]),
                    DepType::Sf => dur_add(i, lag_sub(edge, lf[edge.succ])),
                };
                best = best.min(term);
            }
            if let Some(c) = &plan.tasks[i].constraint
                && let Some(date) = c.date
            {
                match c.kind {
                    ConstraintKind::Fnlt | ConstraintKind::Mfo => {
                        best = best.min(date_end(i, date));
                    }
                    ConstraintKind::Snlt | ConstraintKind::Mso => {
                        best = best.min(dur_add(i, date_start(i, date)));
                    }
                    _ => {}
                }
            }
            if let Some(d) = deadline[i] {
                best = best.min(d);
            }
            lf[i] = if durations[i].minutes > 0 {
                cals.of_task(plan, i).prev_end(best)
            } else {
                best
            };
            ls[i] = dur_sub(i, lf[i]);
        }
        let mut total = vec![None; n];
        let mut free = vec![None; n];
        for &i in order {
            let cal = cals.of_task(plan, i);
            total[i] = Some(cal.between(es[i], ls[i]).min(cal.between(ef[i], lf[i])));
            let mut ff = if outgoing[i].is_empty() {
                cal.between(ef[i], anchor)
            } else {
                i64::MAX
            };
            for &e in &outgoing[i] {
                let edge = &edges[e];
                let slack = match edge.kind {
                    DepType::Fs => cal.between(ef[i], lag_sub(edge, es[edge.succ])),
                    DepType::Ss => cal.between(es[i], lag_sub(edge, es[edge.succ])),
                    DepType::Ff => cal.between(ef[i], lag_sub(edge, ef[edge.succ])),
                    DepType::Sf => cal.between(es[i], lag_sub(edge, ef[edge.succ])),
                };
                ff = ff.min(slack);
            }
            // Free float is part of total float. F-SCH-09 only looks at successors, so with
            // SS/SF links a task that also drives the project finish could get FF > TF; cap it
            // (our interpretation, needed for 0 <= FF <= TF).
            let tf = total[i].unwrap_or(0);
            free[i] = Some(ff.min(tf).max(0));
        }
        Late {
            finish,
            anchor,
            ls,
            lf,
            total,
            free,
        }
    };
    let mut lt = late(&es, &ef, &order, &edges, &outgoing);

    // Resource leveling or smoothing (SPEC.md §7.4, ADR 0011): a serial schedule generation
    // pass over the logic network, in priority order, that delays tasks until their resources
    // fit.
    let mut leveling = None;
    if plan.settings.leveling != Leveling::Off {
        let pool = Pool::new(plan);
        if pool.any_assigned(&in_network) {
            let unleveled_finish = lt.finish;
            let Leveled {
                delays,
                unresolved,
                placed,
            } = level(
                plan,
                &cals,
                &pool,
                &Sgs {
                    order: &order,
                    edges: &edges,
                    outgoing: &outgoing,
                    incoming: &incoming,
                    durations: &durations,
                    total: &lt.total,
                    free: &lt.free,
                    lf: &lt.lf,
                },
                &mut Early {
                    es: &mut es,
                    ef: &mut ef,
                    driver: &mut driver,
                },
                &earliest,
                &dur_add,
            );
            // A delayed task now depends on the task that held its resource: add that as a
            // finish-to-start link for the backward pass, so float and the critical path
            // account for resources (the resource-constrained critical path; ADR 0011).
            let mut all_edges = edges.clone();
            let mut all_out = outgoing.clone();
            for d in &delays {
                all_out[d.by].push(all_edges.len());
                all_edges.push(Edge {
                    pred: d.by,
                    succ: d.task,
                    kind: DepType::Fs,
                    lag: Duration::ZERO,
                    owner: d.task,
                    dep: usize::MAX,
                });
            }
            lt = late(&es, &ef, &placed, &all_edges, &all_out);
            leveling = Some(LevelingReport {
                mode: plan.settings.leveling,
                unleveled_finish,
                delays,
                unresolved,
            });
        }
    }
    let Late {
        finish,
        anchor,
        ls,
        lf,
        total,
        free,
    } = lt;

    // Critical set (F-SCH-10).
    let theta = plan.settings.critical_threshold.minutes;
    let min_tf = order.iter().filter_map(|&i| total[i]).min().unwrap_or(0);
    let critical: Vec<bool> = (0..n)
        .map(|i| match total[i] {
            Some(tf) if in_network[i] => match plan.settings.critical_definition {
                CriticalDefinition::FloatThreshold => tf <= theta,
                CriticalDefinition::LongestPath => tf <= min_tf + theta,
            },
            _ => false,
        })
        .collect();

    let mut tasks: Vec<TaskDates> = (0..n)
        .map(|i| TaskDates {
            es: es[i],
            ef: ef[i],
            ls: ls[i],
            lf: lf[i],
            total_float: total[i],
            free_float: free[i],
            critical: critical[i],
            driver: driver[i],
            duration: if in_network[i] {
                durations[i]
            } else {
                Duration::ZERO
            },
            in_network: in_network[i],
        })
        .collect();

    // Summaries from network leaves, then level of effort, then summaries again.
    roll_up(plan, &index, &mut tasks, true);
    for (i, t) in plan.tasks.iter().enumerate() {
        if t.kind != TaskKind::Loe || index.is_summary(i) {
            continue;
        }
        if let Some((from, to)) = &t.spans
            && let (Some(&a), Some(&b)) = (index.by_id.get(from), index.by_id.get(to))
        {
            let (start, end) = (tasks[a].es, tasks[b].ef.max(tasks[a].es));
            let d = &mut tasks[i];
            d.es = start;
            d.ls = start;
            d.ef = end;
            d.lf = end;
            d.driver = Driver::Span;
            d.total_float = None;
            d.free_float = None;
        }
    }
    roll_up(plan, &index, &mut tasks, false);

    diagnostics.sort_by(|a: &Diagnostic, b| a.code.cmp(b.code));
    Ok(Schedule {
        tasks,
        start: project_start,
        finish,
        anchor,
        diagnostics,
        leveling,
    })
}

/// Rolls summary dates up from their leaves. With `network_only`, level-of-effort leaves are
/// skipped (their dates are not known yet).
fn roll_up(plan: &Plan, index: &PlanIndex, tasks: &mut [TaskDates], network_only: bool) {
    // Children come after parents in a well-formed file, but do not rely on it: walk leaves.
    for i in 0..plan.tasks.len() {
        if !index.is_summary(i) {
            continue;
        }
        let leaves: Vec<usize> = index
            .leaves(i)
            .into_iter()
            .filter(|&l| !network_only || tasks[l].in_network)
            .collect();
        if leaves.is_empty() {
            continue;
        }
        let es = leaves
            .iter()
            .map(|&l| tasks[l].es)
            .min()
            .unwrap_or_default();
        let ef = leaves
            .iter()
            .map(|&l| tasks[l].ef)
            .max()
            .unwrap_or_default();
        let ls = leaves
            .iter()
            .map(|&l| tasks[l].ls)
            .min()
            .unwrap_or_default();
        let lf = leaves
            .iter()
            .map(|&l| tasks[l].lf)
            .max()
            .unwrap_or_default();
        let tf = leaves.iter().filter_map(|&l| tasks[l].total_float).min();
        let critical = leaves.iter().any(|&l| tasks[l].critical);
        let d = &mut tasks[i];
        d.es = es;
        d.ef = ef;
        d.ls = ls;
        d.lf = lf;
        d.total_float = tf;
        d.free_float = None;
        d.critical = critical;
        d.driver = Driver::Rollup;
    }
}

/// Topological order of network leaves; ties broken by source order (Kahn with a min-heap).
fn topo_order(
    plan: &Plan,
    in_network: &[bool],
    edges: &[Edge],
    incoming: &[Vec<usize>],
    outgoing: &[Vec<usize>],
) -> Result<Vec<usize>, Vec<Diagnostic>> {
    let n = plan.tasks.len();
    let mut indegree: Vec<usize> = incoming.iter().map(Vec::len).collect();
    let mut heap: BinaryHeap<Reverse<usize>> = (0..n)
        .filter(|&i| in_network[i] && indegree[i] == 0)
        .map(Reverse)
        .collect();
    let mut order = Vec::with_capacity(n);
    while let Some(Reverse(i)) = heap.pop() {
        order.push(i);
        for &e in &outgoing[i] {
            let s = edges[e].succ;
            indegree[s] -= 1;
            if indegree[s] == 0 {
                heap.push(Reverse(s));
            }
        }
    }
    let expected = in_network.iter().filter(|&&x| x).count();
    if order.len() == expected {
        return Ok(order);
    }
    // Report one cycle: start at the first remaining node and follow remaining predecessors.
    let remaining: Vec<bool> = (0..n).map(|i| in_network[i] && indegree[i] > 0).collect();
    let start = (0..n).find(|&i| remaining[i]).unwrap_or(0);
    let mut path = vec![start];
    let mut seen = vec![usize::MAX; n];
    seen[start] = 0;
    let mut cur = start;
    let cycle = loop {
        let next = incoming[cur]
            .iter()
            .map(|&e| edges[e].pred)
            .find(|&p| remaining[p]);
        let Some(p) = next else { break path.clone() };
        if seen[p] != usize::MAX {
            let mut c: Vec<usize> = path[seen[p]..].to_vec();
            c.reverse();
            c.push(c[0]);
            break c;
        }
        seen[p] = path.len();
        path.push(p);
        cur = p;
    };
    let ids: Vec<String> = cycle.iter().map(|&i| plan.tasks[i].id.clone()).collect();
    let mut d = Diagnostic::error("E-DEP-01", format!("dependency cycle: {}", ids.join(" → ")))
        .with_ids(ids.clone());
    if ids.len() >= 2 {
        d = d.with_hint(format!(
            "remove one link of the cycle, for example `alma dep rm {} {}`",
            ids[0], ids[1]
        ));
    }
    Err(vec![d])
}
