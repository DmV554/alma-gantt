//! Risk and funding queries: `alma risk ls`, and the funding lines of `alma cost` and
//! `alma scurve` (ADR 0015).

use alma_core::model::{Plan, ReserveKind};
use alma_sched::Schedule;
use alma_sched::finance::{covered_emv, emv, emv_time, funding_gap};
use rust_decimal::Decimal;
use serde::Serialize;

use crate::OUTPUT_SCHEMA;
use crate::cost::money;

/// One risk in `alma risk ls`.
#[derive(Debug, Clone, Serialize)]
pub struct RiskView {
    /// Id.
    pub id: String,
    /// Name.
    pub name: String,
    /// `threat` or `opportunity`.
    pub sign: String,
    /// `open` or `closed`.
    pub status: String,
    /// Probability.
    pub probability: f64,
    /// Cost impact.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub impact: Option<f64>,
    /// Expected monetary value (F-COST-37), negative for opportunities.
    pub emv: f64,
    /// Expected schedule impact in working days.
    pub emv_days: f64,
    /// Reserves covering it.
    pub reserves: Vec<String>,
    /// Owner.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
}

/// A contingency compared with the EMV of the risks it covers.
#[derive(Debug, Clone, Serialize)]
pub struct CoverageView {
    /// Reserve id.
    pub reserve: String,
    /// Its amount.
    pub amount: f64,
    /// Σ EMV of its open risks.
    pub emv: f64,
}

/// Result of `alma risk ls`.
#[derive(Debug, Clone, Serialize)]
pub struct RiskReport {
    /// Output schema version.
    pub schema: u32,
    /// Risks, plan order.
    pub risks: Vec<RiskView>,
    /// Σ EMV of open risks: the objective basis of contingency.
    pub total_emv: f64,
    /// Total contingency in the plan.
    pub contingency: f64,
    /// Each contingency that names its risks, against their EMV.
    pub coverage: Vec<CoverageView>,
    /// Open risks no reserve covers.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub uncovered: Vec<String>,
}

/// `alma risk ls`.
pub fn risks(plan: &Plan) -> RiskReport {
    let day = Decimal::from(plan.settings.units.minutes_per_day);
    let views: Vec<RiskView> = plan
        .risks
        .iter()
        .map(|r| RiskView {
            id: r.id.clone(),
            name: r.name.clone(),
            sign: r.sign.as_str().into(),
            status: r.status.as_str().into(),
            probability: money(r.probability),
            impact: r.impact.map(money),
            emv: money(emv(r)),
            emv_days: money(Decimal::from(emv_time(r)) / day),
            reserves: plan
                .reserves
                .iter()
                .filter(|x| x.risks.contains(&r.id))
                .map(|x| x.id.clone())
                .collect(),
            owner: r.owner.clone(),
        })
        .collect();
    let open: Decimal = plan
        .risks
        .iter()
        .filter(|r| r.status == alma_core::model::RiskStatus::Open)
        .map(emv)
        .sum();
    let contingency: Decimal = plan
        .reserves
        .iter()
        .filter(|r| r.kind == ReserveKind::Contingency)
        .map(|r| r.amount)
        .sum();
    RiskReport {
        schema: OUTPUT_SCHEMA,
        uncovered: views
            .iter()
            .filter(|v| v.status == "open" && v.reserves.is_empty())
            .map(|v| v.id.clone())
            .collect(),
        risks: views,
        total_emv: money(open),
        contingency: money(contingency),
        coverage: plan
            .reserves
            .iter()
            .filter(|r| r.kind == ReserveKind::Contingency && !r.risks.is_empty())
            .map(|r| CoverageView {
                reserve: r.id.clone(),
                amount: money(r.amount),
                emv: money(covered_emv(plan, &r.risks)),
            })
            .collect(),
    }
}

/// Funding summary for `alma cost`.
#[derive(Debug, Clone, Serialize)]
pub struct FundingView {
    /// Total funding.
    pub total: f64,
    /// The first day spend exceeds funding, when it does (`L-COST-04`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub short_on: Option<String>,
    /// How much is missing that day.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub short_by: Option<f64>,
}

/// The funding summary, `None` when the plan has no funding entries.
pub fn funding(plan: &Plan, schedule: &Schedule) -> Option<FundingView> {
    if plan.funding.is_empty() {
        return None;
    }
    let gap = funding_gap(plan, &alma_sched::cost::costs(plan, schedule));
    Some(FundingView {
        total: money(plan.funding.iter().map(|f| f.amount).sum()),
        short_on: gap.as_ref().map(|g| g.date.to_string()),
        short_by: gap.map(|g| money(g.spend - g.funded)),
    })
}
