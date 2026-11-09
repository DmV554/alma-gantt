//! `alma evm`: earned value at the status date against a baseline (ADR 0019).

// A failure is one `Diagnostic` per command, as in `mutate`; boxing it buys nothing.
#![allow(clippy::result_large_err)]

use alma_core::diag::Diagnostic;
use alma_core::model::{Plan, Snapshot, WbsRole};
use alma_sched::Schedule;
use alma_sched::cpm::PlanIndex;
use alma_sched::evm::{EvmError, Indices, Measures, evm};
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use serde::Serialize;

use crate::OUTPUT_SCHEMA;
use crate::cost::money;
use crate::query::{Page, wbs_codes};

fn ratio(d: Option<Decimal>) -> Option<f64> {
    d.and_then(|x| x.round_dp(3).to_f64())
}

fn pct(d: Option<Decimal>) -> Option<f64> {
    d.and_then(|x| (x * Decimal::from(100)).round_dp(1).to_f64())
}

/// One WBS node, or the project, in `alma evm`. Money in project currency, rounded to cents;
/// indices to three decimals; percentages to one. A ratio whose divisor is zero is absent.
#[derive(Debug, Clone, Serialize)]
pub struct EvmRow {
    /// Task id; absent on the project row.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// WBS code.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wbs: Option<String>,
    /// Name.
    pub name: String,
    /// Budget at completion.
    pub bac: f64,
    /// Planned value.
    pub pv: f64,
    /// Earned value.
    pub ev: f64,
    /// Actual cost.
    pub ac: f64,
    /// Schedule variance, EV − PV.
    pub sv: f64,
    /// Cost variance, EV − AC.
    pub cv: f64,
    /// SV as a percentage of PV.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sv_pct: Option<f64>,
    /// CV as a percentage of EV.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cv_pct: Option<f64>,
    /// Schedule performance index.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spi: Option<f64>,
    /// Cost performance index.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpi: Option<f64>,
    /// Estimate at completion, BAC / CPI.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eac: Option<f64>,
    /// Estimate to complete, EAC − AC.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub etc: Option<f64>,
    /// Variance at completion, BAC − EAC.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vac: Option<f64>,
    /// Efficiency needed on the remaining work to finish within BAC.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tcpi: Option<f64>,
    /// EV / BAC, percent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub complete_pct: Option<f64>,
    /// PV / BAC, percent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub planned_pct: Option<f64>,
}

impl EvmRow {
    fn new(id: Option<String>, wbs: Option<String>, name: String, m: Measures) -> Self {
        let x = m.indices();
        Self {
            id,
            wbs,
            name,
            bac: money(m.bac),
            pv: money(m.pv),
            ev: money(m.ev),
            ac: money(m.ac),
            sv: money(x.sv),
            cv: money(x.cv),
            sv_pct: pct(x.sv_pct),
            cv_pct: pct(x.cv_pct),
            spi: ratio(x.spi),
            cpi: ratio(x.cpi),
            eac: x.eac_cpi.map(money),
            etc: x.etc.map(money),
            vac: x.vac.map(money),
            tcpi: ratio(x.tcpi_bac),
            complete_pct: pct(x.complete),
            planned_pct: pct(x.planned),
        }
    }
}

/// The statistical forecasts side by side (knowledge notes, resources §5.5): no single one is
/// the answer.
#[derive(Debug, Clone, Serialize)]
pub struct Forecasts {
    /// AC + (BAC − EV): the remaining work goes as budgeted.
    pub eac_budget_rate: f64,
    /// BAC / CPI: cost efficiency so far continues.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eac_cpi: Option<f64>,
    /// AC + (BAC − EV) / (CPI × SPI): schedule pressure also weighs on cost.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eac_cpi_spi: Option<f64>,
    /// TCPI against the CPI forecast.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tcpi_eac: Option<f64>,
}

impl Forecasts {
    fn of(x: &Indices) -> Self {
        Self {
            eac_budget_rate: money(x.eac_budget_rate),
            eac_cpi: x.eac_cpi.map(money),
            eac_cpi_spi: x.eac_cpi_spi.map(money),
            tcpi_eac: ratio(x.tcpi_eac),
        }
    }
}

/// Result of `alma evm`.
#[derive(Debug, Clone, Serialize)]
pub struct EvmReport {
    /// Output schema version.
    pub schema: u32,
    /// Status date measured at.
    pub status_date: String,
    /// Baseline measured against.
    pub baseline: String,
    /// Currency label.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub currency: Option<String>,
    /// The whole project.
    pub project: EvmRow,
    /// Every statistical EAC for the project.
    pub forecasts: Forecasts,
    /// Matching WBS nodes.
    pub total: usize,
    /// Nodes on this page, WBS order.
    pub rows: Vec<EvmRow>,
    /// Baseline tasks the plan no longer has; their budget still counts.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub removed: Vec<String>,
    /// Cursor for the next page.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

/// Which WBS nodes `alma evm` lists.
#[derive(Debug, Clone, Copy)]
pub enum Rows {
    /// Nodes down to this WBS level (1 = top level).
    Depth(usize),
    /// Control accounts.
    Accounts,
}

/// `alma evm`. Fails with `L-EVM-04` without a status date or with a format 0 snapshot.
pub fn report(
    plan: &Plan,
    schedule: &Schedule,
    snap: &Snapshot,
    which: Rows,
    page: Page,
) -> Result<EvmReport, Diagnostic> {
    let e = evm(plan, schedule, snap).map_err(|err| match err {
        EvmError::NoStatusDate => Diagnostic::error(
            "L-EVM-04",
            "earned value is measured at a status date, and the plan has none",
        )
        .with_hint("set it: `alma progress date YYYY-MM-DD`"),
        EvmError::OldSnapshot => Diagnostic::error(
            "L-EVM-04",
            format!(
                "baseline `{}` was saved before per-task budgets existed (snapshot format 0)",
                snap.baseline
            ),
        )
        .with_ids([snap.baseline.clone()])
        .with_hint("save a new baseline: `alma baseline save ID --change CR-… --reason \"...\"`"),
    })?;
    let index = PlanIndex::new(plan);
    let codes = wbs_codes(plan, &index);
    let all: Vec<EvmRow> = plan
        .tasks
        .iter()
        .enumerate()
        .filter(|(i, t)| match which {
            Rows::Depth(d) => index.depth(*i) < d.max(1),
            Rows::Accounts => t.wbs_role == Some(WbsRole::ControlAccount),
        })
        .map(|(i, t)| {
            EvmRow::new(
                Some(t.id.clone()),
                Some(codes[i].clone()),
                t.name.clone(),
                e.rows[i],
            )
        })
        .collect();
    let total = all.len();
    let (rows, next_cursor) = page.apply(&all);
    Ok(EvmReport {
        schema: OUTPUT_SCHEMA,
        status_date: e.status.to_string(),
        baseline: snap.baseline.clone(),
        currency: plan.project.currency.clone(),
        project: EvmRow::new(None, None, plan.project.name.clone(), e.project),
        forecasts: Forecasts::of(&e.project.indices()),
        total,
        rows,
        removed: e.removed,
        next_cursor,
    })
}
