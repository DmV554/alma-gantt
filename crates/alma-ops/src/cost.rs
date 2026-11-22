//! Cost queries: `alma cost` and `alma scurve` (SPEC.md §7.5, knowledge notes resources §3–§5,
//! ADR 0012).

use alma_core::model::Plan;
use alma_core::model::Snapshot;
use alma_sched::Schedule;
use alma_sched::cost::{Source, costs, curve_over};
use alma_sched::resources::Period;
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use serde::Serialize;

use crate::OUTPUT_SCHEMA;
use crate::query::wbs_codes;

/// Money for display: rounded to cents (engine sums keep full precision).
pub fn money(d: Decimal) -> f64 {
    d.round_dp(2).to_f64().unwrap_or(0.0)
}

/// One row of `alma cost`.
#[derive(Debug, Clone, Serialize)]
pub struct CostRow {
    /// Task id, resource id or category.
    pub id: String,
    /// Name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// WBS code (`--by wbs`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wbs: Option<String>,
    /// Booked work in hours (`--by resource`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub work_h: Option<f64>,
    /// Cost.
    pub cost: f64,
    /// Share of the total, in percent.
    pub pct: f64,
}

/// Result of `alma cost`.
#[derive(Debug, Clone, Serialize)]
pub struct CostReport {
    /// Output schema version.
    pub schema: u32,
    /// Currency label from `[project]`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub currency: Option<String>,
    /// Total cost of the current schedule, contingency included: the cost baseline. It becomes
    /// the budget at completion (BAC) once a baseline is saved.
    pub total: f64,
    /// How the total and the budget are built (F-COST-09, F-COST-10).
    pub stack: CostStack,
    /// `wbs`, `resource` or `category`.
    pub by: String,
    /// Rows, in plan order (`wbs`, `resource`) or by category name.
    pub rows: Vec<CostRow>,
    /// Resources assigned but without a rate: their cost counts as zero (`L-RES-05`).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub unpriced: Vec<String>,
}

/// The cost stack: work and fixed costs, plus contingency, is the cost baseline; plus
/// management reserve, the project budget.
#[derive(Debug, Clone, Serialize)]
pub struct CostStack {
    /// Work (resources) and fixed costs.
    pub work: f64,
    /// Contingency reserves (inside the baseline).
    pub contingency: f64,
    /// Cost baseline: work + contingency.
    pub cost_baseline: f64,
    /// Management reserve (outside the baseline).
    pub management_reserve: f64,
    /// Project budget: cost baseline + management reserve.
    pub project_budget: f64,
}

fn stack(c: &alma_sched::cost::Costs) -> CostStack {
    CostStack {
        work: money(c.total - c.contingency),
        contingency: money(c.contingency),
        cost_baseline: money(c.total),
        management_reserve: money(c.management),
        project_budget: money(c.total + c.management),
    }
}

/// `alma cost`: the cost of the current schedule, grouped by WBS (down to `depth`), resource or
/// category (F-COST-07). Assignments are grouped under their resource kind (`work`,
/// `equipment`, `material`, `cost`); fixed costs under their category or `uncategorized`.
pub fn report(
    plan: &Plan,
    schedule: &Schedule,
    by: &str,
    depth: usize,
) -> Result<CostReport, String> {
    let c = costs(plan, schedule);
    let pct = |x: Decimal| {
        if c.total.is_zero() {
            0.0
        } else {
            money(x * Decimal::from(100) / c.total)
        }
    };
    let rows = match by {
        "wbs" => {
            let index = alma_sched::PlanIndex::new(plan);
            let codes = wbs_codes(plan, &index);
            plan.tasks
                .iter()
                .enumerate()
                .filter(|(i, _)| index.depth(*i) < depth.max(1))
                .map(|(i, t)| CostRow {
                    id: t.id.clone(),
                    name: Some(t.name.clone()),
                    wbs: Some(codes[i].clone()),
                    work_h: None,
                    cost: money(c.task_total[i]),
                    pct: pct(c.task_total[i]),
                })
                .collect()
        }
        "resource" => plan
            .resources
            .iter()
            .enumerate()
            .map(|(r, res)| {
                let work: Decimal = c
                    .lines
                    .iter()
                    .filter(|l| l.source == Source::Resource(r))
                    .map(|l| l.work_h)
                    .sum();
                CostRow {
                    id: res.id.clone(),
                    name: Some(res.name.clone()),
                    wbs: None,
                    work_h: Some(money(work)),
                    cost: money(c.by_resource[r]),
                    pct: pct(c.by_resource[r]),
                }
            })
            .collect(),
        "category" => {
            let mut groups: std::collections::BTreeMap<String, Decimal> = Default::default();
            for l in &c.lines {
                let key = match (l.source, &l.category) {
                    (Source::Resource(r), _) => plan.resources[r].kind.as_str().to_string(),
                    (Source::Fixed, Some(cat)) => cat.clone(),
                    (Source::Fixed, None) => "uncategorized".to_string(),
                    (Source::Contingency(_), _) => "contingency".to_string(),
                };
                *groups.entry(key).or_default() += l.amount;
            }
            groups
                .into_iter()
                .map(|(k, v)| CostRow {
                    id: k,
                    name: None,
                    wbs: None,
                    work_h: None,
                    cost: money(v),
                    pct: pct(v),
                })
                .collect()
        }
        other => return Err(format!("`--by {other}`: use wbs, resource or category")),
    };
    Ok(CostReport {
        schema: OUTPUT_SCHEMA,
        currency: plan.project.currency.clone(),
        total: money(c.total),
        stack: stack(&c),
        by: by.into(),
        rows,
        unpriced: c
            .unpriced
            .iter()
            .map(|&r| plan.resources[r].id.clone())
            .collect(),
    })
}

/// One period of the S-curve.
#[derive(Debug, Clone, Serialize)]
pub struct CurvePoint {
    /// First day.
    pub start: String,
    /// Last day.
    pub end: String,
    /// Cost accrued in the period.
    pub cost: f64,
    /// Cumulative cost at the end of the period.
    pub cumulative: f64,
    /// Cumulative planned value at the end of the period, from the baseline (F-COST-13).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pv: Option<f64>,
}

/// Result of `alma scurve`.
#[derive(Debug, Clone, Serialize)]
pub struct CurveReport {
    /// Output schema version.
    pub schema: u32,
    /// What `cost` and `cumulative` are: `forecast` (the current schedule). Planned value
    /// (`pv`) comes from `baseline`.
    pub basis: String,
    /// Baseline the `pv` column comes from (the active one), when the plan has one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub baseline: Option<String>,
    /// Budget at completion of that baseline.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bac: Option<f64>,
    /// Currency label.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub currency: Option<String>,
    /// `day`, `week` or `month`.
    pub period: String,
    /// Total (the cost baseline of the current schedule).
    pub total: f64,
    /// Project budget (total + management reserve): a ceiling to draw over the curve.
    pub budget: f64,
    /// Periods from the first cost to the finish.
    pub points: Vec<CurvePoint>,
}

/// `alma scurve`: cost per period and cumulative (F-COST-08, F-COST-12), with planned value
/// from `baseline` when given.
pub fn scurve(
    plan: &Plan,
    schedule: &Schedule,
    period: &str,
    baseline: Option<&Snapshot>,
) -> Result<CurveReport, String> {
    let p = Period::parse(period)
        .ok_or_else(|| format!("period `{period}` must be day, week or month"))?;
    let c = costs(plan, schedule);
    let widen = baseline.map(|b| (b.start, b.finish + 1));
    let pts = curve_over(plan, schedule, &c, p, widen);
    let buckets: Vec<(jiff::civil::Date, jiff::civil::Date)> =
        pts.iter().map(|x| (x.start, x.end)).collect();
    let pv = baseline.map(|b| alma_sched::baseline::pv_curve(b, &buckets));
    let points = pts
        .into_iter()
        .enumerate()
        .map(|(i, pt)| CurvePoint {
            start: pt.start.to_string(),
            end: pt
                .end
                .checked_sub(jiff::Span::new().days(1))
                .unwrap_or(pt.end)
                .to_string(),
            cost: money(pt.cost),
            cumulative: money(pt.cumulative),
            pv: pv.as_ref().map(|v| money(v[i])),
        })
        .collect();
    Ok(CurveReport {
        schema: OUTPUT_SCHEMA,
        basis: "forecast".into(),
        baseline: baseline.map(|b| b.baseline.clone()),
        bac: baseline.map(|b| money(b.bac)),
        currency: plan.project.currency.clone(),
        period: period.into(),
        total: money(c.total),
        budget: money(c.total + c.management),
        points,
    })
}

/// The S-curve as CSV: `start,end,cost,cumulative`, and `pv` when there is a baseline.
pub fn scurve_csv(r: &CurveReport) -> String {
    let mut out = String::from(if r.baseline.is_some() {
        "start,end,cost,cumulative,pv\n"
    } else {
        "start,end,cost,cumulative\n"
    });
    for p in &r.points {
        out.push_str(&format!(
            "{},{},{:.2},{:.2}",
            p.start, p.end, p.cost, p.cumulative
        ));
        if let Some(pv) = p.pv {
            out.push_str(&format!(",{pv:.2}"));
        }
        out.push('\n');
    }
    out
}
