//! Lint rules for plans (knowledge notes, Annex A; codes per ADR 0008).
//!
//! Each rule has a default severity; `[rules.severity]` overrides it and `allow` on a task
//! silences it for that task. Rules that need dates are skipped when the plan cannot be
//! scheduled.

use std::collections::HashSet;

use alma_core::diag::{Diagnostic, Severity};
use alma_core::model::{
    Approach, ConstraintKind, DepNature, DepScope, DepType, Leveling, MilestoneClass, ParamValue,
    Phase, Plan, ReportingPeriod, SchedType, TaskKind, WbsRole,
};
use alma_core::time::{finish_date, format_instant, start_date};
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;

use crate::cpm::{Calendars, PlanIndex, Schedule, resolve_duration};
use crate::resources;

/// A lint rule.
#[derive(Debug, Clone, Copy)]
pub struct Rule {
    /// Code.
    pub code: &'static str,
    /// Default severity; `None` = off by default.
    pub default: Option<Severity>,
    /// One-line summary.
    pub summary: &'static str,
}

const fn rule(code: &'static str, default: Option<Severity>, summary: &'static str) -> Rule {
    Rule {
        code,
        default,
        summary,
    }
}

use Severity::{Error as E, Info as I, Warn as W};

/// Every rule implemented, in code order.
pub const RULES: &[Rule] = &[
    rule(
        "L-CAL-01",
        Some(W),
        "Calendar working day differs from hours_per_day",
    ),
    rule(
        "L-COST-02",
        Some(E),
        "Control account with no work packages",
    ),
    rule(
        "L-DEP-01",
        Some(W),
        "Open start: activity with no predecessor",
    ),
    rule(
        "L-DEP-02",
        Some(W),
        "Open finish: activity with no successor",
    ),
    rule(
        "L-DEP-04",
        Some(W),
        "More than one link between the same pair",
    ),
    rule("L-DEP-05", Some(I), "Start-to-finish link used"),
    rule("L-DEP-06", Some(I), "Lead (negative lag) used"),
    rule("L-DEP-07", Some(I), "External dependency without rationale"),
    rule(
        "L-DEP-08",
        Some(I),
        "Discretionary dependency without rationale",
    ),
    rule(
        "L-DEP-09",
        Some(W),
        "Dependency attached to a WBS summary element",
    ),
    rule(
        "L-DEP-11",
        Some(W),
        "Lag longer than max_lag or than the predecessor",
    ),
    rule("L-DEP-12", Some(W), "Lag without rationale"),
    rule("L-EST-01", Some(I), "Estimate without basis"),
    rule(
        "L-EST-02",
        Some(E),
        "Three-point values not ordered o <= m <= p",
    ),
    rule(
        "L-EST-03",
        Some(W),
        "Duration, effort and units inconsistent",
    ),
    rule(
        "L-EST-09",
        Some(I),
        "Duration outside the three-point range",
    ),
    rule(
        "L-RES-01",
        Some(W),
        "Resource booked above its capacity (error once leveling is on)",
    ),
    rule(
        "L-RES-02",
        Some(W),
        "Smoothing left an over-allocation it could not resolve within free float",
    ),
    rule(
        "L-SCH-01",
        Some(E),
        "Milestone with duration, effort or assignments",
    ),
    rule(
        "L-SCH-02",
        Some(W),
        "Milestone without acceptance criterion or owner",
    ),
    rule("L-SCH-03", Some(W), "Work package with nothing to schedule"),
    rule("L-SCH-04", Some(W), "Activity without owner or assignment"),
    rule("L-SCH-05", Some(E), "Negative total float"),
    rule(
        "L-SCH-06",
        Some(W),
        "Hard constraint on an activity with predecessors",
    ),
    rule(
        "L-SCH-09",
        Some(E),
        "Level of effort with its own predecessors or successors",
    ),
    rule(
        "L-SCH-11",
        Some(I),
        "SNET constraint used instead of a predecessor",
    ),
    rule("L-WBS-01", Some(W), "WBS element with exactly one child"),
    rule("L-WBS-02", Some(W), "Plan with fewer than two WBS levels"),
    rule(
        "L-WBS-04",
        Some(W),
        "Work package without an accountable owner",
    ),
    rule(
        "L-WBS-05",
        Some(W),
        "Work package effort outside the configured range",
    ),
    rule(
        "L-WBS-06",
        Some(W),
        "Work package longer than the reporting period",
    ),
    rule(
        "L-WBS-07",
        Some(I),
        "Work package missing dictionary fields",
    ),
    rule("L-WBS-10", Some(I), "No final milestone"),
    rule(
        "L-WBS-12",
        Some(W),
        "Work package outside a control account, or nested accounts",
    ),
];

/// Looks up a rule.
pub fn rule_def(code: &str) -> Option<&'static Rule> {
    RULES.iter().find(|r| r.code == code)
}

struct Linter<'a> {
    plan: &'a Plan,
    index: PlanIndex,
    out: Vec<(usize, Diagnostic)>,
}

impl Linter<'_> {
    /// Reports `code` on task `i` (or the plan when `i` is `usize::MAX`), honoring config.
    fn report(
        &mut self,
        i: usize,
        code: &'static str,
        fallback: Severity,
        msg: String,
        ids: Vec<String>,
    ) {
        if i != usize::MAX && self.plan.tasks[i].allow.iter().any(|c| c == code) {
            return;
        }
        let severity = match self.plan.rules.severity.get(code) {
            Some(Some(s)) => *s,
            Some(None) => return,
            None => fallback,
        };
        let mut d = Diagnostic::new(code, severity, msg).with_ids(ids);
        if i != usize::MAX {
            d = d.with_span(self.plan.tasks[i].span);
        }
        self.out.push((i, d));
    }

    fn on(&mut self, i: usize, code: &'static str, msg: String) {
        let sev = rule_def(code)
            .and_then(|r| r.default)
            .unwrap_or(Severity::Info);
        let id = self.plan.tasks[i].id.clone();
        self.report(i, code, sev, msg, vec![id]);
    }

    fn effective_approach(&self, i: usize) -> Approach {
        let mut cur = Some(i);
        while let Some(n) = cur {
            if let Some(a) = self.plan.tasks[n].approach {
                return a;
            }
            cur = self.index.parent[n];
        }
        Approach::Predictive
    }

    fn has_ancestor_role(&self, i: usize, role: WbsRole) -> bool {
        let mut cur = self.index.parent[i];
        while let Some(n) = cur {
            if self.plan.tasks[n].wbs_role == Some(role) {
                return true;
            }
            cur = self.index.parent[n];
        }
        false
    }

    fn param_minutes(&self, key: &str, default: i64) -> i64 {
        match self.plan.rules.params.get(key) {
            Some(ParamValue::Duration(d)) => d.minutes,
            _ => default,
        }
    }
}

/// Runs every rule. `schedule` is `None` when the plan could not be scheduled.
pub fn lint(plan: &Plan, schedule: Option<&Schedule>) -> Vec<Diagnostic> {
    let mut l = Linter {
        plan,
        index: PlanIndex::new(plan),
        out: Vec::new(),
    };
    let n = plan.tasks.len();
    let units = plan.settings.units;
    let day = units.minutes_per_day;
    let cals = Calendars::new(plan);

    // Calendars.
    for c in &plan.calendars {
        if c.day_minutes() != day {
            let msg = format!(
                "calendar `{}` has {} working hours per day but hours_per_day is {}; \"1d\" will not match one calendar day",
                c.id,
                fmt_hours(c.day_minutes()),
                fmt_hours(day)
            );
            l.report(usize::MAX, "L-CAL-01", W, msg, vec![c.id.clone()]);
        }
    }

    // Which network leaves have predecessors / successors after expanding summary links.
    let leaf_ok: Vec<bool> = (0..n)
        .map(|i| !l.index.is_summary(i) && plan.tasks[i].kind != TaskKind::Loe)
        .collect();
    let mut has_pred = vec![false; n];
    let mut has_succ = vec![false; n];
    for (owner, t) in plan.tasks.iter().enumerate() {
        for d in &t.deps {
            let Some(&p) = l.index.by_id.get(&d.pred) else {
                continue;
            };
            for leaf in l.index.leaves(owner) {
                has_pred[leaf] = true;
            }
            for leaf in l.index.leaves(p) {
                has_succ[leaf] = true;
            }
        }
    }

    // WBS structure.
    if n > 1 && (0..n).all(|i| l.index.parent[i].is_none()) {
        l.report(
            usize::MAX,
            "L-WBS-02",
            W,
            "the plan has a single WBS level; group the work under deliverables or control accounts"
                .into(),
            Vec::new(),
        );
    }
    let uses_accounts = plan
        .tasks
        .iter()
        .any(|t| t.wbs_role == Some(WbsRole::ControlAccount));
    for i in 0..n {
        let t = &plan.tasks[i];
        if l.index.children[i].len() == 1 {
            let child = plan.tasks[l.index.children[i][0]].id.clone();
            l.on(
                i,
                "L-WBS-01",
                format!("`{}` has a single child (`{child}`): either it repeats the parent or siblings are missing", t.id),
            );
        }
        match t.wbs_role {
            Some(WbsRole::WorkPackage) => {
                if t.owner.is_none() {
                    l.on(
                        i,
                        "L-WBS-04",
                        format!("work package `{}` has no `owner`", t.id),
                    );
                }
                let missing: Vec<&str> = [
                    ("description", t.description.is_none()),
                    ("deliverable", t.deliverable.is_none()),
                    ("acceptance", t.acceptance.is_none()),
                ]
                .into_iter()
                .filter_map(|(k, m)| m.then_some(k))
                .collect();
                if !missing.is_empty() {
                    l.on(
                        i,
                        "L-WBS-07",
                        format!("work package `{}` has no {}", t.id, missing.join(", ")),
                    );
                }
                if uses_accounts && !l.has_ancestor_role(i, WbsRole::ControlAccount) {
                    l.on(
                        i,
                        "L-WBS-12",
                        format!("work package `{}` is not under any control account", t.id),
                    );
                }
                let leaves = l.index.leaves(i);
                let nothing = leaves.iter().all(|&x| {
                    let lt = &plan.tasks[x];
                    lt.kind != TaskKind::Milestone
                        && lt.duration.is_none()
                        && lt.effort.is_none()
                        && lt.estimate.is_none()
                });
                if nothing {
                    l.on(
                        i,
                        "L-SCH-03",
                        format!(
                            "work package `{}` has no activities, duration or effort",
                            t.id
                        ),
                    );
                }
                // Effort = explicit effort, or Σ units × duration, over the leaves.
                let mut effort = 0i64;
                let mut known = false;
                for &x in &leaves {
                    let lt = &plan.tasks[x];
                    if let Some(e) = lt.effort {
                        effort += e.minutes;
                        known = true;
                    } else if !lt.assign.is_empty() {
                        let d = resolve_duration(plan, x);
                        if !d.elapsed {
                            let u: Decimal = lt.assign.iter().map(|a| a.units).sum();
                            effort += (u * Decimal::from(d.minutes)).to_i64().unwrap_or(0);
                            known = true;
                        }
                    }
                }
                let (lo, hi) = (
                    l.param_minutes("wp_effort_min", 8 * 60),
                    l.param_minutes("wp_effort_max", 80 * 60),
                );
                if known && (effort < lo || effort > hi) {
                    l.on(
                        i,
                        "L-WBS-05",
                        format!(
                            "work package `{}` has {} h of effort, outside {}–{} h",
                            t.id,
                            fmt_hours(effort),
                            fmt_hours(lo),
                            fmt_hours(hi)
                        ),
                    );
                }
                if let (Some(s), Some(period)) = (schedule, plan.settings.reporting_period) {
                    let limit = match period {
                        ReportingPeriod::Week => units.days_per_week * day,
                        ReportingPeriod::Month => units.days_per_week * day * 52 / 12,
                    };
                    let d = &s.tasks[i];
                    let span = cals.of_task(plan, i).between(d.es, d.ef);
                    if span > limit {
                        l.on(
                            i,
                            "L-WBS-06",
                            format!(
                                "work package `{}` lasts {} working days, longer than one reporting period ({})",
                                t.id,
                                fmt_days(span, day),
                                period.as_str()
                            ),
                        );
                    }
                }
            }
            Some(WbsRole::ControlAccount) => {
                if l.has_ancestor_role(i, WbsRole::ControlAccount) {
                    let id = t.id.clone();
                    l.report(
                        i,
                        "L-WBS-12",
                        E,
                        format!("control account `{id}` is nested inside another control account"),
                        vec![id],
                    );
                }
                let has_wp = l.index.leaves(i).iter().any(|&x| {
                    x != i && {
                        let mut cur = Some(x);
                        let mut found = false;
                        while let Some(c) = cur {
                            if c == i {
                                break;
                            }
                            if plan.tasks[c].wbs_role == Some(WbsRole::WorkPackage) {
                                found = true;
                            }
                            cur = l.index.parent[c];
                        }
                        found
                    }
                });
                if !has_wp {
                    l.on(
                        i,
                        "L-COST-02",
                        format!("control account `{}` has no work packages", t.id),
                    );
                }
            }
            _ => {}
        }
    }

    // Milestones, activities, LOE.
    let start_node = (0..n).find(|&i| leaf_ok[i] && !has_pred[i]);
    let mut final_milestone = false;
    for i in 0..n {
        let t = &plan.tasks[i];
        if l.index.is_summary(i) {
            continue;
        }
        match t.kind {
            TaskKind::Milestone => {
                let dur = t.duration.is_some_and(|d| d.minutes != 0);
                if dur || t.effort.is_some_and(|d| d.minutes != 0) || !t.assign.is_empty() {
                    l.on(
                        i,
                        "L-SCH-01",
                        format!("milestone `{}` has a duration, effort or assignments; milestones take no time", t.id),
                    );
                }
                let missing: Vec<&str> = [
                    (
                        "an acceptance criterion (`acceptance`)",
                        t.acceptance.is_none(),
                    ),
                    ("an owner (`owner`)", t.owner.is_none()),
                ]
                .into_iter()
                .filter_map(|(k, m)| m.then_some(k))
                .collect();
                if !missing.is_empty() {
                    let sev = if t.class == Some(MilestoneClass::Contractual) {
                        W
                    } else {
                        I
                    };
                    let id = t.id.clone();
                    l.report(
                        i,
                        "L-SCH-02",
                        sev,
                        format!("milestone `{id}` has no {}", missing.join(" and no ")),
                        vec![id.clone()],
                    );
                }
                if !has_succ[i] {
                    final_milestone = true;
                }
            }
            TaskKind::Task => {
                let owned = t.owner.is_some() || !t.assign.is_empty() || {
                    let mut cur = l.index.parent[i];
                    let mut found = false;
                    while let Some(p) = cur {
                        if plan.tasks[p].owner.is_some() {
                            found = true;
                            break;
                        }
                        cur = l.index.parent[p];
                    }
                    found
                };
                if !owned {
                    l.on(
                        i,
                        "L-SCH-04",
                        format!("activity `{}` has no owner or assignment", t.id),
                    );
                }
            }
            TaskKind::Loe => {
                if !t.deps.is_empty() || has_succ[i] {
                    l.on(
                        i,
                        "L-SCH-09",
                        format!("level-of-effort `{}` has its own links; its dates come from `spans` only", t.id),
                    );
                }
            }
            TaskKind::Buffer => {}
        }

        // Open ends (not inside iterative/flow branches, not LOE, not external milestones).
        let exempt = t.kind == TaskKind::Loe
            || t.class == Some(MilestoneClass::External)
            || l.effective_approach(i) != Approach::Predictive;
        if !exempt && !has_pred[i] && Some(i) != start_node {
            let snet = t
                .constraint
                .as_ref()
                .is_some_and(|c| c.kind == ConstraintKind::Snet);
            if snet {
                l.on(
                    i,
                    "L-SCH-11",
                    format!("`{}` is positioned only by an SNET constraint; link it to the work it waits for", t.id),
                );
            }
            l.on(
                i,
                "L-DEP-01",
                format!("`{}` has no predecessor (open start)", t.id),
            );
        }
        let is_finish = t.kind == TaskKind::Milestone && !has_succ[i];
        if !exempt && !has_succ[i] && !is_finish {
            l.on(
                i,
                "L-DEP-02",
                format!("`{}` has no successor (open finish)", t.id),
            );
        }

        // Hard constraints that override logic.
        if let Some(c) = &t.constraint {
            let hard = matches!(
                c.kind,
                ConstraintKind::Mso
                    | ConstraintKind::Mfo
                    | ConstraintKind::Snlt
                    | ConstraintKind::Fnlt
            );
            if hard && has_pred[i] {
                l.on(
                    i,
                    "L-SCH-06",
                    format!(
                        "`{}` has a {} constraint and predecessors; prefer a `deadline`, which reports a miss without overriding logic",
                        t.id,
                        c.kind.as_str()
                    ),
                );
            }
        }
    }
    if n > 0 && !final_milestone {
        l.report(
            usize::MAX,
            "L-WBS-10",
            I,
            "the plan has no final milestone (a milestone with no successors)".into(),
            Vec::new(),
        );
    }

    // Dependencies.
    let max_lag = l.param_minutes("max_lag", 10 * day);
    for i in 0..n {
        let t = &plan.tasks[i];
        let mut pairs = HashSet::new();
        for d in &t.deps {
            let pred_idx = l.index.by_id.get(&d.pred).copied();
            if !pairs.insert(d.pred.as_str()) {
                l.on(
                    i,
                    "L-DEP-04",
                    format!(
                        "`{}` has more than one link from `{}`; keep the most constraining one",
                        t.id, d.pred
                    ),
                );
            }
            if d.kind == DepType::Sf {
                l.on(
                    i,
                    "L-DEP-05",
                    format!(
                        "`{}` uses a start-to-finish link from `{}`; confirm it is intended",
                        t.id, d.pred
                    ),
                );
            }
            if d.lag.minutes < 0 {
                l.on(i, "L-DEP-06", format!("`{}` overlaps `{}` with a lead of {}; if the overlap is real work, split the predecessor", t.id, d.pred, d.lag.display(units)));
            }
            if d.scope == DepScope::External && d.rationale.is_none() {
                l.on(
                    i,
                    "L-DEP-07",
                    format!("external link `{}` → `{}` has no rationale", d.pred, t.id),
                );
            }
            if d.nature == DepNature::Discretionary && d.rationale.is_none() {
                l.on(
                    i,
                    "L-DEP-08",
                    format!(
                        "discretionary link `{}` → `{}` has no rationale",
                        d.pred, t.id
                    ),
                );
            }
            let summary_end =
                l.index.is_summary(i) || pred_idx.is_some_and(|p| l.index.is_summary(p));
            if summary_end {
                l.on(i, "L-DEP-09", format!("link `{}` → `{}` is attached to a WBS summary; link the activities or a milestone instead", d.pred, t.id));
            }
            if d.lag.minutes > 0 {
                let pred_d = pred_idx
                    .filter(|&p| !l.index.is_summary(p))
                    .map(|p| resolve_duration(plan, p).minutes);
                let too_long =
                    d.lag.minutes > max_lag || pred_d.is_some_and(|pd| d.lag.minutes > pd);
                if too_long && !d.lag.elapsed || d.lag.elapsed && d.lag.minutes > max_lag * 3 {
                    l.on(i, "L-DEP-11", format!("lag of {} on `{}` → `{}` is long; model the wait as an activity or an external milestone so it can be owned and tracked", d.lag.display(units), d.pred, t.id));
                }
            }
            if d.lag.minutes != 0 && d.rationale.is_none() {
                l.on(
                    i,
                    "L-DEP-12",
                    format!(
                        "lag of {} on `{}` → `{}` has no rationale",
                        d.lag.display(units),
                        d.pred,
                        t.id
                    ),
                );
            }
        }
    }

    // Estimates.
    for i in 0..n {
        let t = &plan.tasks[i];
        if let Some(e) = &t.estimate {
            if e.basis.is_none() {
                l.on(
                    i,
                    "L-EST-01",
                    format!("estimate of `{}` has no basis", t.id),
                );
            }
            if !(e.o.minutes <= e.m.minutes && e.m.minutes <= e.p.minutes) {
                l.on(
                    i,
                    "L-EST-02",
                    format!("estimate of `{}` is not ordered o <= m <= p", t.id),
                );
            }
            if let Some(d) = t.duration
                && (d.minutes < e.o.minutes || d.minutes > e.p.minutes)
            {
                l.on(
                    i,
                    "L-EST-09",
                    format!(
                        "duration of `{}` ({}) is outside its estimate range",
                        t.id,
                        d.display(units)
                    ),
                );
            }
        }
        if t.sched_type != SchedType::FixedEffort
            && let (Some(d), Some(e)) = (t.duration, t.effort)
        {
            let u: Decimal = t.assign.iter().map(|a| a.units).sum();
            if !t.assign.is_empty() && !d.elapsed {
                let implied = (u * Decimal::from(d.minutes)).to_i64().unwrap_or(0);
                if (implied - e.minutes).abs() > 1 {
                    l.on(
                        i,
                        "L-EST-03",
                        format!(
                            "`{}`: duration × units = {} h but effort is {} h",
                            t.id,
                            fmt_hours(implied),
                            fmt_hours(e.minutes)
                        ),
                    );
                }
            }
        }
    }

    // Float.
    if let Some(s) = schedule {
        let draft = plan.settings.phase == Phase::Draft;
        for i in 0..n {
            let d = &s.tasks[i];
            if !d.in_network {
                continue;
            }
            if let Some(tf) = d.total_float.filter(|&tf| tf < 0) {
                let sev = if draft { W } else { E };
                let id = plan.tasks[i].id.clone();
                l.report(
                    i,
                    "L-SCH-05",
                    sev,
                    format!(
                        "`{id}` has negative total float ({} working days): a deadline or constraint cannot be met (late finish {})",
                        fmt_days(tf, day),
                        format_instant(d.lf)
                    ),
                    vec![id.clone()],
                );
            }
        }
    }

    // Resources (F-COST-04).
    if let Some(s) = schedule {
        for o in resources::overloads(plan, s) {
            let r = &plan.resources[o.resource];
            let tasks: Vec<String> = o.tasks.iter().map(|&t| plan.tasks[t].id.clone()).collect();
            let (code, sev, hint) = match plan.settings.leveling {
                Leveling::Off => (
                    "L-RES-01",
                    W,
                    "let the scheduler resolve it with `alma level` (the finish may move) or \
                     `alma level --mode smoothing` (only within free float); or lower the units, \
                     raise max_units, or add a dependency between the competing tasks",
                ),
                Leveling::Leveling => (
                    "L-RES-01",
                    E,
                    "leveling cannot fix this: one assignment is above max_units, or tasks with \
                     fixed dates (MSO/MFO) or level-of-effort tasks overlap; lower the units or \
                     raise max_units",
                ),
                Leveling::Smoothing => (
                    "L-RES-02",
                    W,
                    "there is no room within free float; `alma level --mode leveling` resolves it \
                     by letting the finish move",
                ),
            };
            let mut ids = vec![r.id.clone()];
            ids.extend(tasks.iter().cloned());
            let msg = format!(
                "`{}` is booked {} of {} units from {} to {} ({})",
                r.id,
                o.peak.normalize(),
                o.max.normalize(),
                start_date(o.from),
                finish_date(o.to),
                tasks.join(", ")
            );
            let before = l.out.len();
            l.report(usize::MAX, code, sev, msg, ids);
            if l.out.len() > before
                && let Some((_, d)) = l.out.last_mut()
            {
                *d = d.clone().with_hint(hint);
            }
        }
    }

    let mut out = l.out;
    out.sort_by(|(a, x), (b, y)| a.cmp(b).then(x.code.cmp(y.code)));
    // Plan-level findings (index usize::MAX) go first.
    out.sort_by_key(|(i, _)| *i != usize::MAX);
    out.into_iter().map(|(_, d)| d).collect()
}

fn fmt_hours(minutes: i64) -> String {
    if minutes % 60 == 0 {
        (minutes / 60).to_string()
    } else {
        format!("{:.2}", minutes as f64 / 60.0)
    }
}

fn fmt_days(minutes: i64, day: i64) -> String {
    let v = minutes as f64 / day as f64;
    if (v.fract()).abs() < 1e-9 {
        format!("{v:.0}")
    } else {
        format!("{v:.2}")
    }
}
