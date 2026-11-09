//! Earned value management (knowledge notes, resources §5.3–5.6; formulas F-COST-13 to
//! F-COST-30; ADR 0019).
//!
//! Every quantity is measured at the status date against a baseline snapshot (format 1), which
//! holds each task's own budget and its time-phased planned value. Reserves are not earned:
//! the budget at completion here is the sum of task budgets (work and fixed costs), and
//! contingency stays a reserve until a change moves it into tasks.
//!
//! - PV: the baseline's cumulative planned value of each task through the status date.
//! - EV: each task's budget × the fraction its progress earns, by `ev_method` (never above 1).
//! - AC: entered, cumulative, on any WBS node (`actual_cost`).
//!
//! Rows roll up bottom-up: a node's values are the sum over itself and its descendants.

use std::collections::{HashMap, HashSet};

use alma_core::model::{EvMethod, Plan, Snapshot, TaskKind};
use jiff::civil::Date;
use rust_decimal::Decimal;

use crate::cpm::{PlanIndex, Progress, Schedule};

/// The four base quantities of one WBS node or of the project.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Measures {
    /// Budget at completion.
    pub bac: Decimal,
    /// Planned value through the status date (F-COST-13).
    pub pv: Decimal,
    /// Earned value (F-COST-14).
    pub ev: Decimal,
    /// Actual cost (F-COST-15).
    pub ac: Decimal,
}

impl std::ops::AddAssign for Measures {
    fn add_assign(&mut self, o: Self) {
        self.bac += o.bac;
        self.pv += o.pv;
        self.ev += o.ev;
        self.ac += o.ac;
    }
}

/// Variances, indices and forecasts derived from [`Measures`]. A ratio whose divisor is zero
/// is `None`, never infinite.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Indices {
    /// SV = EV − PV (F-COST-16).
    pub sv: Decimal,
    /// SV% = SV / PV.
    pub sv_pct: Option<Decimal>,
    /// CV = EV − AC (F-COST-17).
    pub cv: Decimal,
    /// CV% = CV / EV.
    pub cv_pct: Option<Decimal>,
    /// SPI = EV / PV (F-COST-18).
    pub spi: Option<Decimal>,
    /// CPI = EV / AC (F-COST-19).
    pub cpi: Option<Decimal>,
    /// EAC = AC + (BAC − EV): remaining work at the budgeted rate (F-COST-21).
    pub eac_budget_rate: Decimal,
    /// EAC = BAC / CPI: remaining work at the current cost efficiency (F-COST-22).
    pub eac_cpi: Option<Decimal>,
    /// EAC = AC + (BAC − EV) / (CPI × SPI) (F-COST-23).
    pub eac_cpi_spi: Option<Decimal>,
    /// ETC = EAC − AC, with the CPI forecast (F-COST-25).
    pub etc: Option<Decimal>,
    /// VAC = BAC − EAC, with the CPI forecast (F-COST-26).
    pub vac: Option<Decimal>,
    /// TCPI against the budget = (BAC − EV) / (BAC − AC) (F-COST-27).
    pub tcpi_bac: Option<Decimal>,
    /// TCPI against the CPI forecast = (BAC − EV) / (EAC − AC) (F-COST-28).
    pub tcpi_eac: Option<Decimal>,
    /// Percent complete = EV / BAC (F-COST-30), as a fraction.
    pub complete: Option<Decimal>,
    /// Percent planned = PV / BAC, as a fraction.
    pub planned: Option<Decimal>,
}

fn div(a: Decimal, b: Decimal) -> Option<Decimal> {
    if b.is_zero() { None } else { a.checked_div(b) }
}

impl Measures {
    /// Derives every variance, index and forecast.
    pub fn indices(&self) -> Indices {
        let Measures { bac, pv, ev, ac } = *self;
        let sv = ev - pv;
        let cv = ev - ac;
        let spi = div(ev, pv);
        let cpi = div(ev, ac);
        let eac_cpi = cpi.and_then(|c| div(bac, c));
        let eac_cpi_spi = match (cpi, spi) {
            (Some(c), Some(s)) => div(bac - ev, c * s).map(|x| ac + x),
            _ => None,
        };
        Indices {
            sv,
            sv_pct: div(sv, pv),
            cv,
            cv_pct: div(cv, ev),
            spi,
            cpi,
            eac_budget_rate: ac + (bac - ev),
            eac_cpi,
            eac_cpi_spi,
            etc: eac_cpi.map(|e| e - ac),
            vac: eac_cpi.map(|e| bac - e),
            tcpi_bac: div(bac - ev, bac - ac),
            tcpi_eac: eac_cpi.and_then(|e| div(bac - ev, e - ac)),
            complete: div(ev, bac),
            planned: div(pv, bac),
        }
    }
}

/// Why earned value cannot be measured.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvmError {
    /// The plan has no `status_date` (`L-EVM-04`).
    NoStatusDate,
    /// The snapshot is format 0: it has no per-task budgets (`L-EVM-04`).
    OldSnapshot,
}

/// Earned value of a plan at its status date.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Evm {
    /// The status date.
    pub status: Date,
    /// Per task, rolled up over its descendants; parallel to `plan.tasks`.
    pub rows: Vec<Measures>,
    /// Per task, its own values only; parallel to `plan.tasks`.
    pub own: Vec<Measures>,
    /// The whole project, including baseline tasks the plan no longer has.
    pub project: Measures,
    /// Baseline tasks the plan no longer has: their budget and planned value still count
    /// until a revised baseline removes them.
    pub removed: Vec<String>,
}

/// The fraction of its budget a task has earned (F-COST-14), from 0 to 1.
pub fn earned_fraction(plan: &Plan, schedule: &Schedule, i: usize) -> Decimal {
    let t = &plan.tasks[i];
    match schedule.tasks[i].progress {
        Progress::Complete => Decimal::ONE,
        Progress::NotStarted => Decimal::ZERO,
        Progress::InProgress { .. } => match t.ev_method.unwrap_or(EvMethod::Physical) {
            EvMethod::Physical => (t.physical_pct.unwrap_or_default() / Decimal::from(100))
                .clamp(Decimal::ZERO, Decimal::ONE),
            EvMethod::Fixed { start } => Decimal::from(start) / Decimal::from(100),
        },
    }
}

/// Measures every task and the project at the status date against `snap`.
pub fn evm(plan: &Plan, schedule: &Schedule, snap: &Snapshot) -> Result<Evm, EvmError> {
    let status = plan.project.status_date.ok_or(EvmError::NoStatusDate)?;
    if !snap.per_task {
        return Err(EvmError::OldSnapshot);
    }
    let index = PlanIndex::new(plan);
    let base: HashMap<&str, &alma_core::model::SnapTask> =
        snap.tasks.iter().map(|t| (t.id.as_str(), t)).collect();
    let n = plan.tasks.len();
    let mut own = vec![Measures::default(); n];
    for (i, t) in plan.tasks.iter().enumerate() {
        let m = &mut own[i];
        m.ac = t.actual_cost.unwrap_or_default();
        let Some(b) = base.get(t.id.as_str()) else {
            continue;
        };
        m.bac = b.budget;
        m.pv = b.pv_through(status);
        // Level of effort, and money carried by a summary itself, is earned as planned: it
        // has no deliverable of its own to measure (resources §5.6).
        m.ev = if t.kind == TaskKind::Loe || index.is_summary(i) {
            m.pv.min(m.bac)
        } else {
            m.bac * earned_fraction(plan, schedule, i)
        };
    }
    let mut rows = own.clone();
    for (i, m) in own.iter().enumerate() {
        // Add each task to every ancestor.
        let mut cur = index.parent[i];
        let mut steps = 0;
        while let Some(p) = cur {
            rows[p] += *m;
            cur = index.parent[p];
            steps += 1;
            if steps > n {
                break;
            }
        }
    }
    let mut project = Measures::default();
    for m in &own {
        project += *m;
    }
    let current: HashSet<&str> = plan.tasks.iter().map(|t| t.id.as_str()).collect();
    let mut removed = Vec::new();
    for b in snap
        .tasks
        .iter()
        .filter(|b| !current.contains(b.id.as_str()))
    {
        project.bac += b.budget;
        project.pv += b.pv_through(status);
        if !b.budget.is_zero() {
            removed.push(b.id.clone());
        }
    }
    Ok(Evm {
        status,
        rows,
        own,
        project,
        removed,
    })
}
