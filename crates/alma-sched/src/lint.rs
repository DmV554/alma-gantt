//! Lint rules for plans (knowledge notes, Annex A; codes per ADR 0008).
//!
//! Each rule has a default severity; `[rules.severity]` overrides it and `allow` on a task
//! silences it for that task. Rules that need dates are skipped when the plan cannot be
//! scheduled.

use std::collections::HashSet;

use alma_core::diag::{Diagnostic, Severity};
use alma_core::model::{
    Approach, ConstraintKind, DepNature, DepScope, DepType, Leveling, MilestoneClass, ParamValue,
    Phase, Plan, ReportingPeriod, ReserveKind, RiskStatus, SchedType, TaskKind, WbsRole,
};
use alma_core::time::{finish_date, format_instant, start_date};
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;

use crate::cpm::{Calendars, PlanIndex, Schedule, resolve_duration};
use crate::{finance, resources};
use alma_core::model::Snapshot;

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
    rule("L-BAS-01", Some(W), "Progress recorded but no baseline"),
    rule("L-BAS-02", Some(E), "Progress recorded but no status date"),
    rule(
        "L-BAS-03",
        Some(W),
        "Milestone or project finish later than the baseline by more than finish_slip",
    ),
    rule(
        "L-BAS-04",
        Some(E),
        "Actual date after the status date, or progress against unmet mandatory logic",
    ),
    rule("L-BAS-05", Some(I), "Rebaseline without a change reference"),
    rule(
        "L-BAS-06",
        Some(E),
        "Revised baseline rewrites planned value before its revision date",
    ),
    rule(
        "L-BAS-09",
        Some(W),
        "Baseline revision above change_threshold without approved_by",
    ),
    rule(
        "L-CAL-01",
        Some(W),
        "Calendar working day differs from hours_per_day",
    ),
    rule("L-COST-01", Some(W), "Fixed cost without a category"),
    rule(
        "L-COST-02",
        Some(E),
        "Control account with no work packages",
    ),
    rule(
        "L-COST-04",
        Some(E),
        "Cumulative spend exceeds cumulative funding (warn while draft)",
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
        "L-EVM-01",
        Some(W),
        "Cost or schedule variance beyond the control threshold at the status date",
    ),
    rule(
        "L-EVM-02",
        Some(E),
        "Progress on a task with no budget in the baseline",
    ),
    rule(
        "L-EVM-03",
        Some(W),
        "Actual cost without progress, or earned value without any actual cost",
    ),
    rule(
        "L-EVM-04",
        Some(E),
        "Earned value needs a status date and a format 1 baseline",
    ),
    rule(
        "L-EVM-05",
        Some(W),
        "To-complete performance index above tcpi_max",
    ),
    rule("L-EVM-08", Some(I), "Long task measured with 0/100"),
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
        "L-RES-03",
        Some(E),
        "Task and assigned resource calendars share no working time",
    ),
    rule(
        "L-RES-05",
        Some(W),
        "Resource with assignments but no rate (its cost counts as zero)",
    ),
    rule(
        "L-RISK-01",
        Some(W),
        "Contingency reserve without a basis or linked risks",
    ),
    rule(
        "L-RISK-02",
        Some(I),
        "Contingency differs from the EMV of its risks by more than emv_tolerance",
    ),
    rule(
        "L-RISK-03",
        Some(I),
        "Correlated risks covered by one reserve as if independent",
    ),
    rule(
        "L-RISK-05",
        Some(I),
        "Contingency held for risks that are all closed",
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

    fn param_number(&self, key: &str, default: i64) -> Decimal {
        match self.plan.rules.params.get(key) {
            Some(ParamValue::Number(n)) => *n,
            _ => Decimal::from(default),
        }
    }

    fn hint_last(&mut self, before: usize, hint: &str) {
        if self.out.len() > before
            && let Some((_, d)) = self.out.last_mut()
        {
            *d = d.clone().with_hint(hint.to_string());
        }
    }

    fn finish(self) -> Vec<Diagnostic> {
        let mut out = self.out;
        out.sort_by(|(a, x), (b, y)| a.cmp(b).then(x.code.cmp(y.code)));
        // Plan-level findings (index usize::MAX) go first.
        out.sort_by_key(|(i, _)| *i != usize::MAX);
        out.into_iter().map(|(_, d)| d).collect()
    }
}

/// Rules that measure the plan against its active baseline (ADR 0019, ADR 0020): `L-BAS-03`,
/// `L-BAS-06`, `L-BAS-09` and the earned value rules. `snap` is the active baseline's
/// snapshot; `parent` the snapshot of the baseline it revises, when it has one.
pub fn lint_baseline(
    plan: &Plan,
    schedule: &Schedule,
    snap: &Snapshot,
    parent: Option<&Snapshot>,
) -> Vec<Diagnostic> {
    let mut l = Linter {
        plan,
        index: PlanIndex::new(plan),
        out: Vec::new(),
    };
    let units = plan.settings.units;
    let day = units.minutes_per_day;
    let cals = Calendars::new(plan);

    // L-BAS-06 and L-BAS-09: a revision keeps the past and records who approved a big change.
    let meta = plan.baselines.iter().find(|b| b.id == snap.baseline);
    if let (Some(parent), Some(meta)) = (parent, meta) {
        // Planned value before the revision date must be the parent's.
        let cutoff = snap.revised_at.or(meta.date);
        if let Some(cutoff) = cutoff {
            let mut days: Vec<jiff::civil::Date> = snap
                .pv
                .iter()
                .chain(parent.pv.iter())
                .map(|(d, _)| *d)
                .filter(|d| match snap.revised_at {
                    Some(at) => *d <= at,
                    None => *d < cutoff,
                })
                .collect();
            days.sort_unstable();
            days.dedup();
            if let Some(d) = days
                .into_iter()
                .find(|d| snap.pv_through(*d) != parent.pv_through(*d))
            {
                let before = l.out.len();
                l.report(
                    usize::MAX,
                    "L-BAS-06",
                    E,
                    format!(
                        "baseline `{}` changes the planned value of `{}` before {cutoff}: on {d} it is {} instead of {}",
                        snap.baseline,
                        parent.baseline,
                        snap.pv_through(d).round_dp(2),
                        parent.pv_through(d).round_dp(2)
                    ),
                    vec![snap.baseline.clone(), parent.baseline.clone()],
                );
                l.hint_last(
                    before,
                    "past performance is never rewritten: revise from the status date instead, `alma baseline revise ID --change CR-…`",
                );
            }
        }
        let threshold = match plan.rules.params.get("change_threshold") {
            Some(ParamValue::Number(n)) => Some(*n),
            _ => None,
        };
        let change = snap.bac - parent.bac;
        if let Some(t) = threshold
            && change.abs() > t
            && meta.approved_by.is_none()
        {
            let before = l.out.len();
            l.report(
                usize::MAX,
                "L-BAS-09",
                W,
                format!(
                    "baseline `{}` changes the budget by {} (above change_threshold {t}) and records no approval",
                    snap.baseline,
                    change.round_dp(2)
                ),
                vec![snap.baseline.clone()],
            );
            l.hint_last(
                before,
                "record who approved it: edit `approved_by` on the [[baseline]], or revise again with --approved-by",
            );
        }
    }

    // L-BAS-03: milestones and the project finish against the baseline.
    let slip = l.param_minutes("finish_slip", 5 * day);
    let base: std::collections::HashMap<&str, &alma_core::model::SnapTask> =
        snap.tasks.iter().map(|t| (t.id.as_str(), t)).collect();
    let late_by = |cal: &crate::WorkCalendar, from: i64, to: i64| {
        if to > from { cal.between(from, to) } else { 0 }
    };
    let project_slip = late_by(cals.project(), snap.finish, schedule.finish);
    if project_slip > slip {
        let before = l.out.len();
        l.report(
            usize::MAX,
            "L-BAS-03",
            W,
            format!(
                "the project finishes {} working days after baseline `{}` ({} instead of {})",
                fmt_days(project_slip, day),
                snap.baseline,
                finish_date(schedule.finish),
                finish_date(snap.finish)
            ),
            vec![snap.baseline.clone()],
        );
        l.hint_last(
            before,
            "recover the critical path (`alma critical`, `alma explain ID`), or revise the baseline through change control",
        );
    }
    for (i, t) in plan.tasks.iter().enumerate() {
        if t.kind != TaskKind::Milestone || l.index.is_summary(i) {
            continue;
        }
        let Some(b) = base.get(t.id.as_str()) else {
            continue;
        };
        let late = late_by(cals.of_task(plan, i), b.finish, schedule.tasks[i].ef);
        if late > slip {
            l.on(
                i,
                "L-BAS-03",
                format!(
                    "milestone `{}` is forecast {} working days after its baseline date ({} instead of {})",
                    t.id,
                    fmt_days(late, day),
                    start_date(schedule.tasks[i].ef),
                    start_date(b.finish)
                ),
            );
        }
    }

    // Earned value at the status date.
    let e = match crate::evm::evm(plan, schedule, snap) {
        Ok(e) => e,
        Err(_) => return l.finish(),
    };
    let tracked = plan
        .tasks
        .iter()
        .any(|t| t.has_progress() || t.actual_cost.is_some());
    if !tracked {
        return l.finish();
    }
    // L-EVM-01: the project and every control account.
    let cv_max = l.param_number("cv_threshold", 10);
    let sv_max = l.param_number("sv_threshold", 10);
    let hundred = Decimal::from(100);
    let mut rows: Vec<(usize, crate::evm::Measures)> = vec![(usize::MAX, e.project)];
    for (i, t) in plan.tasks.iter().enumerate() {
        if t.wbs_role == Some(WbsRole::ControlAccount) {
            rows.push((i, e.rows[i]));
        }
    }
    for (i, m) in rows {
        let x = m.indices();
        let name = if i == usize::MAX {
            "the project".to_string()
        } else {
            format!("control account `{}`", plan.tasks[i].id)
        };
        let mut over = Vec::new();
        if let Some(cv) = x.cv_pct.filter(|v| *v * hundred < -cv_max) {
            over.push(format!(
                "cost variance {}% (CPI {})",
                (cv * hundred).round_dp(1),
                x.cpi.map_or("n/a".into(), |c| c.round_dp(2).to_string())
            ));
        }
        if let Some(sv) = x.sv_pct.filter(|v| *v * hundred < -sv_max) {
            over.push(format!(
                "schedule variance {}% (SPI {})",
                (sv * hundred).round_dp(1),
                x.spi.map_or("n/a".into(), |c| c.round_dp(2).to_string())
            ));
        }
        if over.is_empty() {
            continue;
        }
        let ids = if i == usize::MAX {
            Vec::new()
        } else {
            vec![plan.tasks[i].id.clone()]
        };
        let before = l.out.len();
        l.report(
            i,
            "L-EVM-01",
            W,
            format!(
                "{name} is beyond the control threshold at {}: {}",
                e.status,
                over.join(", ")
            ),
            ids,
        );
        l.hint_last(
            before,
            "find the cause with `alma evm` and `alma progress ls`; thresholds are rule parameters cv_threshold and sv_threshold (percent)",
        );
    }
    // L-EVM-02 and L-EVM-03 per task.
    let cost = crate::cost::costs(plan, schedule);
    for (i, t) in plan.tasks.iter().enumerate() {
        let d = &schedule.tasks[i];
        if d.in_network
            && d.progress.started()
            && e.own[i].bac.is_zero()
            && !cost.task_total[i].is_zero()
        {
            l.on(
                i,
                "L-EVM-02",
                format!(
                    "`{}` has progress but no budget in baseline `{}`: its work earns nothing",
                    t.id, snap.baseline
                ),
            );
        }
        if d.in_network && !d.progress.started() && t.actual_cost.is_some_and(|c| !c.is_zero()) {
            l.on(
                i,
                "L-EVM-03",
                format!(
                    "`{}` has actual cost but has not started: cost and progress are measured differently",
                    t.id
                ),
            );
        }
    }
    if e.project.ev > Decimal::ZERO && !plan.tasks.iter().any(|t| t.actual_cost.is_some()) {
        let before = l.out.len();
        l.report(
            usize::MAX,
            "L-EVM-03",
            W,
            format!(
                "work worth {} has been earned but no actual cost is recorded, so CPI cannot be measured",
                e.project.ev.round_dp(2)
            ),
            Vec::new(),
        );
        l.hint_last(
            before,
            "record what was spent: `alma progress set ID --cost 1200` on tasks or control accounts",
        );
    }
    // L-EVM-05: recovery the plan assumes.
    let tcpi_max = match plan.rules.params.get("tcpi_max") {
        Some(ParamValue::Number(n)) => *n,
        _ => Decimal::new(11, 1),
    };
    if let Some(t) = e.project.indices().tcpi_bac.filter(|t| *t > tcpi_max) {
        let before = l.out.len();
        l.report(
            usize::MAX,
            "L-EVM-05",
            W,
            format!(
                "finishing within budget needs a cost efficiency of {} on the remaining work (TCPI), above {tcpi_max}",
                t.round_dp(3)
            ),
            Vec::new(),
        );
        l.hint_last(
            before,
            "the budget assumes a recovery that is unlikely; forecast with the EAC from `alma evm` and raise it through change control",
        );
    }
    l.finish()
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

    // Progress (knowledge notes, scheduling §4.14 and §4.16; ADR 0018).
    let progressed: Vec<usize> = (0..n).filter(|&i| plan.tasks[i].has_progress()).collect();
    if !progressed.is_empty() {
        let ids: Vec<String> = progressed
            .iter()
            .take(5)
            .map(|&i| plan.tasks[i].id.clone())
            .collect();
        let hint = |l: &mut Linter, before: usize, hint: &str| {
            if l.out.len() > before
                && let Some((_, d)) = l.out.last_mut()
            {
                *d = d.clone().with_hint(hint.to_string());
            }
        };
        if plan.baselines.is_empty() {
            let before = l.out.len();
            l.report(
                usize::MAX,
                "L-BAS-01",
                W,
                format!(
                    "progress is recorded on {} task(s) ({}) but the plan has no baseline to measure it against",
                    progressed.len(),
                    ids.join(", ")
                ),
                ids.clone(),
            );
            hint(
                &mut l,
                before,
                "save a baseline before recording progress: `alma baseline save ID`",
            );
        }
        if plan.project.status_date.is_none() {
            let before = l.out.len();
            l.report(
                usize::MAX,
                "L-BAS-02",
                E,
                format!(
                    "progress is recorded on {} task(s) ({}) but [project] has no status_date",
                    progressed.len(),
                    ids.join(", ")
                ),
                ids,
            );
            hint(
                &mut l,
                before,
                "set the day the progress was measured: `alma progress date YYYY-MM-DD`",
            );
        }
    }
    if let Some(status) = plan.project.status_date {
        for &i in &progressed {
            let t = &plan.tasks[i];
            for (what, date) in [
                ("actual_start", t.actual_start),
                ("actual_finish", t.actual_finish),
            ] {
                if let Some(d) = date.filter(|d| *d > status) {
                    l.on(
                        i,
                        "L-BAS-04",
                        format!(
                            "`{}` has {what} {d}, after the status date {status}: progress cannot be recorded in the future",
                            t.id
                        ),
                    );
                }
            }
        }
    }
    for &i in &progressed {
        if plan.tasks[i].actual_start.is_none() && plan.tasks[i].actual_finish.is_none() {
            continue;
        }
        // Links on the task and on its ancestors apply to it.
        let mut owners = vec![i];
        let mut cur = l.index.parent[i];
        while let Some(p) = cur {
            owners.push(p);
            cur = l.index.parent[p];
        }
        let mut waiting: Vec<String> = Vec::new();
        for &o in &owners {
            for d in &plan.tasks[o].deps {
                if d.kind != DepType::Fs || d.nature != DepNature::Mandatory {
                    continue;
                }
                let Some(&p) = l.index.by_id.get(&d.pred) else {
                    continue;
                };
                for leaf in l.index.leaves(p) {
                    let pt = &plan.tasks[leaf];
                    let started = pt.actual_start.is_some() || pt.actual_finish.is_some();
                    if !started && pt.kind != TaskKind::Loe && !waiting.contains(&pt.id) {
                        waiting.push(pt.id.clone());
                    }
                }
            }
        }
        if !waiting.is_empty() {
            let id = plan.tasks[i].id.clone();
            let mut ids = vec![id.clone()];
            ids.extend(waiting.iter().cloned());
            let before = l.out.len();
            l.report(
                i,
                "L-BAS-04",
                E,
                format!(
                    "`{id}` has started but its mandatory finish-to-start predecessor(s) {} have not",
                    waiting.join(", ")
                ),
                ids,
            );
            if l.out.len() > before
                && let Some((_, d)) = l.out.last_mut()
            {
                *d = d.clone().with_hint(
                    "record the predecessors' actual dates, or, if the work really overlapped, make the link discretionary or SS",
                );
            }
        }
    }

    // L-EVM-08: a long task measured 0/100 earns nothing until it finishes.
    if let Some(s) = schedule {
        let period_days = match plan.settings.reporting_period {
            Some(ReportingPeriod::Week) => units.days_per_week,
            _ => units.days_per_week * 52 / 12,
        };
        for (i, t) in plan.tasks.iter().enumerate() {
            if t.ev_method != Some(alma_core::model::EvMethod::Fixed { start: 0 })
                || !s.tasks[i].in_network
            {
                continue;
            }
            let d = s.tasks[i].duration;
            let days = if d.elapsed {
                d.minutes / (24 * 60) * units.days_per_week / 7
            } else {
                d.minutes / day
            };
            if days > 2 * period_days {
                l.on(
                    i,
                    "L-EVM-08",
                    format!(
                        "`{}` lasts {days} working days but earns nothing until it finishes (0/100); earned value will jump",
                        t.id
                    ),
                );
            }
        }
    }

    // Baselines (knowledge notes, scheduling §1.13 and §4.14).
    for b in &plan.baselines {
        if let (Some(parent), None) = (&b.parent, &b.change_ref) {
            l.report(
                usize::MAX,
                "L-BAS-05",
                I,
                format!(
                    "baseline `{}` revises `{parent}` without a change reference (change_ref)",
                    b.id
                ),
                vec![b.id.clone(), parent.clone()],
            );
        }
    }

    // Reserves and risks (knowledge notes, resources §4.1, §7.1; F-COST-37).
    let tolerance = match plan.rules.params.get("emv_tolerance") {
        Some(ParamValue::Number(n)) => *n,
        _ => Decimal::from(10),
    };
    for r in plan
        .reserves
        .iter()
        .filter(|r| r.kind == ReserveKind::Contingency)
    {
        if r.risks.is_empty() {
            continue;
        }
        let linked: Vec<&alma_core::model::Risk> = plan
            .risks
            .iter()
            .filter(|k| r.risks.contains(&k.id))
            .collect();
        if !linked.is_empty() && linked.iter().all(|k| k.status == RiskStatus::Closed) {
            l.report(
                usize::MAX,
                "L-RISK-05",
                I,
                format!(
                    "contingency `{}` still holds {} for risks that are all closed; release it",
                    r.id,
                    r.amount.normalize()
                ),
                vec![r.id.clone()],
            );
            continue;
        }
        let emv = finance::covered_emv(plan, &r.risks);
        let off = if emv.is_zero() {
            !r.amount.is_zero()
        } else {
            ((r.amount - emv).abs() * Decimal::from(100) / emv.abs()) > tolerance
        };
        if off {
            l.report(
                usize::MAX,
                "L-RISK-02",
                I,
                format!(
                    "contingency `{}` is {} but the EMV of its open risks is {}",
                    r.id,
                    r.amount.normalize(),
                    emv.round_dp(2).normalize()
                ),
                vec![r.id.clone()],
            );
        }
        let mut groups: Vec<&str> = linked
            .iter()
            .filter_map(|k| k.correlation_group.as_deref())
            .collect();
        groups.sort_unstable();
        let shared: Vec<&str> = groups
            .windows(2)
            .filter(|w| w[0] == w[1])
            .map(|w| w[0])
            .collect::<Vec<_>>();
        if let Some(g) = shared.first() {
            l.report(
                usize::MAX,
                "L-RISK-03",
                I,
                format!(
                    "contingency `{}` covers several risks of correlation group `{g}`; they tend to happen together, so their sum understates the exposure",
                    r.id
                ),
                vec![r.id.clone()],
            );
        }
    }
    for r in &plan.reserves {
        if r.kind == ReserveKind::Contingency && r.basis.is_none() && r.risks.is_empty() {
            let before = l.out.len();
            l.report(
                usize::MAX,
                "L-RISK-01",
                W,
                format!(
                    "contingency `{}` has no basis and no linked risks: a reserve nobody can justify reads as padding",
                    r.id
                ),
                vec![r.id.clone()],
            );
            if l.out.len() > before
                && let Some((_, d)) = l.out.last_mut()
            {
                *d = d.clone().with_hint(
                    "link the risks it covers (`--risks r1,r2`) or say how it was sized (`--basis \"10% of work, per policy\"`)",
                );
            }
        }
    }

    // Resource calendars (knowledge notes, resources §1.3).
    for &(i, r) in &cals.disjoint {
        let t = &plan.tasks[i];
        let res = &plan.resources[r];
        l.on(
            i,
            "L-RES-03",
            format!(
                "`{}` cannot be worked by `{}`: the task calendar and calendar `{}` share no working time, so the task keeps its own calendar",
                t.id,
                res.id,
                res.calendar.clone().unwrap_or_default()
            ),
        );
    }

    // Costs (F-COST-07).
    for res in &plan.resources {
        if res.rate.is_some() {
            continue;
        }
        let tasks: Vec<String> = plan
            .tasks
            .iter()
            .filter(|t| t.assign.iter().any(|a| a.resource == res.id))
            .map(|t| t.id.clone())
            .collect();
        if tasks.is_empty() {
            continue;
        }
        let msg = format!(
            "`{}` is assigned to {} but has no rate, so its cost counts as zero",
            res.id,
            tasks.join(", ")
        );
        let before = l.out.len();
        l.report(usize::MAX, "L-RES-05", W, msg, vec![res.id.clone()]);
        if l.out.len() > before
            && let Some((_, d)) = l.out.last_mut()
        {
            *d = d.clone().with_hint(format!(
                "set it with `alma res set {} --rate AMOUNT/day` (or /hour, /week, /month, /use)",
                res.id
            ));
        }
    }
    for (i, t) in plan.tasks.iter().enumerate() {
        if let Some(fc) = &t.fixed_cost
            && fc.category.is_none()
        {
            l.on(
                i,
                "L-COST-01",
                format!(
                    "fixed cost of `{}` has no category, so cost and cash-flow reports cannot group it",
                    t.id
                ),
            );
        }
    }

    // Funding (F-COST-11).
    if let Some(s) = schedule
        && let Some(g) = finance::funding_gap(plan, &crate::cost::costs(plan, s))
    {
        let sev = if plan.settings.phase == Phase::Draft {
            W
        } else {
            E
        };
        let before = l.out.len();
        // The entries available by then: adding later funding does not change the finding.
        let ids: Vec<String> = plan
            .funding
            .iter()
            .filter(|f| f.date <= g.date)
            .map(|f| f.id.clone())
            .collect();
        l.report(
            usize::MAX,
            "L-COST-04",
            sev,
            format!(
                "by {} the plan has spent {} but only {} is funded (short by {})",
                g.date,
                g.spend.round_dp(2).normalize(),
                g.funded.round_dp(2).normalize(),
                (g.spend - g.funded).round_dp(2).normalize()
            ),
            ids,
        );
        if l.out.len() > before
            && let Some((_, d)) = l.out.last_mut()
        {
            *d = d.clone().with_hint(
                "delay flexible work with a start-no-earlier-than constraint (`alma task set ID --constraint SNET:DATE`), or add funding (`alma funding add`)",
            );
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

    l.finish()
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
