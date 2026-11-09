//! Risks and funding (knowledge notes, resources §4.4 and §7.1; F-COST-11, F-COST-37;
//! ADR 0015).

use alma_core::model::{Plan, Risk, RiskSign, RiskStatus};
use jiff::civil::Date;
use rust_decimal::Decimal;

use crate::baseline::daily;
use crate::cost::Costs;

/// Expected monetary value of a risk (F-COST-37): probability × cost impact, negative for an
/// opportunity. Zero when it has no cost impact.
pub fn emv(r: &Risk) -> Decimal {
    let v = r.probability * r.impact.unwrap_or(Decimal::ZERO);
    match r.sign {
        RiskSign::Threat => v,
        RiskSign::Opportunity => -v,
    }
}

/// Expected schedule impact in working minutes: probability × time impact (negative for an
/// opportunity), rounded up.
pub fn emv_time(r: &Risk) -> i64 {
    let Some(t) = r.impact_time else { return 0 };
    let v = (r.probability * Decimal::from(t.minutes)).ceil();
    let m = i64::try_from(v).unwrap_or(0);
    match r.sign {
        RiskSign::Threat => m,
        RiskSign::Opportunity => -m,
    }
}

/// Σ EMV of the open risks a reserve covers.
pub fn covered_emv(plan: &Plan, risk_ids: &[String]) -> Decimal {
    plan.risks
        .iter()
        .filter(|r| r.status == RiskStatus::Open && risk_ids.contains(&r.id))
        .map(emv)
        .sum()
}

/// Cumulative funding available at the end of `date` (Σ amounts dated on or before it).
pub fn funded_through(plan: &Plan, date: Date) -> Decimal {
    plan.funding
        .iter()
        .filter(|f| f.date <= date)
        .map(|f| f.amount)
        .sum()
}

/// The first day cumulative planned spend exceeds cumulative funding (F-COST-11).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FundingGap {
    /// The day.
    pub date: Date,
    /// Cumulative spend at the end of that day.
    pub spend: Decimal,
    /// Cumulative funding by then.
    pub funded: Decimal,
}

/// Checks spend against funding; `None` when the plan has no funding entries or never spends
/// more than it has. Spend is the cost baseline of the current schedule (management reserve is
/// not spent until it is released).
pub fn funding_gap(plan: &Plan, costs: &Costs) -> Option<FundingGap> {
    if plan.funding.is_empty() {
        return None;
    }
    daily(plan, costs)
        .into_iter()
        .map(|(date, spend)| FundingGap {
            date,
            spend,
            funded: funded_through(plan, date),
        })
        .find(|g| g.spend > g.funded)
}
