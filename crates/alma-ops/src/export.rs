//! Exports: the Gantt chart, the resource histogram and the S-curve as SVG or PNG, and the task
//! table as CSV (SPEC.md §5 M0, M2).

use alma_core::model::Plan;
use alma_layout::{Options, Scale, Theme};
use alma_sched::Schedule;
use jiff::civil::Date;

use crate::query::task_views;

pub use alma_export::ExportError;

/// What to render.
#[derive(Debug, Clone, Default)]
pub struct RenderRequest {
    /// `"2027-01..2027-06"` (months) or `"2027-01-04..2027-03-31"` (days).
    pub range: Option<String>,
    /// Deepest WBS level shown (1 = top level only).
    pub depth: Option<usize>,
    /// `day`, `week` or `month`.
    pub scale: Option<String>,
    /// Dark theme.
    pub dark: bool,
    /// Hide dependency arrows.
    pub no_arrows: bool,
    /// Base font size in points (minimum 9).
    pub font_size: Option<f64>,
    /// `gantt` (default), `histogram` or `scurve`.
    pub chart: Option<String>,
    /// Resource id, for `histogram`.
    pub resource: Option<String>,
    /// `day`, `week` or `month`, for `histogram` (default week) and `scurve` (default month).
    pub period: Option<String>,
    /// Baseline snapshot for the planned value line of `scurve`.
    pub baseline: Option<alma_core::model::Snapshot>,
}

/// Parses `A..B` where each end is `YYYY-MM` or `YYYY-MM-DD`.
pub fn parse_range(text: &str) -> Result<(Date, Date), String> {
    let (a, b) = text
        .split_once("..")
        .ok_or_else(|| format!("range `{text}` must look like 2027-01..2027-06"))?;
    let end_of = |s: &str, start: bool| -> Result<Date, String> {
        if let Ok(d) = s.parse::<Date>() {
            return Ok(d);
        }
        let (y, m) = s
            .split_once('-')
            .and_then(|(y, m)| Some((y.parse::<i16>().ok()?, m.parse::<i8>().ok()?)))
            .ok_or_else(|| format!("`{s}` is not YYYY-MM or YYYY-MM-DD"))?;
        let first = Date::new(y, m, 1).map_err(|_| format!("`{s}` is not a valid month"))?;
        Ok(if start { first } else { first.last_of_month() })
    };
    let (from, to) = (end_of(a, true)?, end_of(b, false)?);
    if to < from {
        return Err(format!("range `{text}` ends before it starts"));
    }
    Ok((from, to))
}

fn options(req: &RenderRequest) -> Result<Options, String> {
    let scale = match req.scale.as_deref() {
        None => None,
        Some("day") => Some(Scale::Day),
        Some("week") => Some(Scale::Week),
        Some("month") => Some(Scale::Month),
        Some(s) => return Err(format!("unknown scale `{s}`; use day, week or month")),
    };
    Ok(Options {
        range: req.range.as_deref().map(parse_range).transpose()?,
        scale,
        max_depth: req.depth.map(|d| d.saturating_sub(1)),
        font_size: req.font_size.unwrap_or(10.0),
        arrows: !req.no_arrows,
        theme: if req.dark { Theme::DARK } else { Theme::LIGHT },
    })
}

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// A short period label: `Mar 27` for months, `08 Mar` for days and weeks.
fn period_label(start: &str, period: &str) -> String {
    let Ok(d) = start.parse::<Date>() else {
        return start.to_string();
    };
    let mon = MONTHS[(d.month() - 1) as usize];
    if period == "month" {
        format!("{mon} {:02}", d.year() % 100)
    } else {
        format!("{:02} {mon}", d.day())
    }
}

/// The display list of the requested chart.
fn display(
    plan: &Plan,
    schedule: &Schedule,
    req: &RenderRequest,
) -> Result<alma_layout::DisplayList, String> {
    let opts = options(req)?;
    match req.chart.as_deref().unwrap_or("gantt") {
        "gantt" => Ok(alma_layout::gantt(plan, schedule, &opts)),
        "histogram" => {
            let id = req.resource.as_deref().ok_or_else(|| {
                let ids: Vec<&str> = plan.resources.iter().map(|r| r.id.as_str()).collect();
                format!(
                    "the histogram needs a resource (--resource ID); resources: {}",
                    ids.join(", ")
                )
            })?;
            let period = req.period.as_deref().unwrap_or("week");
            let u = crate::resources::usage(plan, schedule, id, period)?;
            let name = plan
                .resources
                .iter()
                .find(|r| r.id == id)
                .map(|r| r.name.clone())
                .unwrap_or_default();
            let data = alma_layout::charts::HistogramData {
                title: format!(
                    "{id} ({name}): demand vs capacity per {period}, max {}",
                    u.max_units
                ),
                labels: u
                    .periods
                    .iter()
                    .map(|b| period_label(&b.start, period))
                    .collect(),
                demand: u.periods.iter().map(|b| b.demand_h).collect(),
                capacity: u.periods.iter().map(|b| b.capacity_h).collect(),
            };
            Ok(alma_layout::charts::histogram(
                &data,
                opts.theme,
                opts.font_size,
            ))
        }
        "scurve" => {
            let period = req.period.as_deref().unwrap_or("month");
            let r = crate::cost::scurve(plan, schedule, period, req.baseline.as_ref())?;
            let mut series = vec![alma_layout::charts::Series {
                name: "forecast".into(),
                values: r.points.iter().map(|p| p.cumulative).collect(),
                main: true,
                step: false,
            }];
            if r.points.iter().any(|p| p.funded.is_some()) {
                series.push(alma_layout::charts::Series {
                    name: "funded".into(),
                    values: r.points.iter().map(|p| p.funded.unwrap_or(0.0)).collect(),
                    main: false,
                    step: true,
                });
            }
            if let Some(b) = &r.baseline {
                series.push(alma_layout::charts::Series {
                    name: format!("planned value ({b})"),
                    values: r.points.iter().map(|p| p.pv.unwrap_or(0.0)).collect(),
                    main: false,
                    step: false,
                });
            }
            let data = alma_layout::charts::CurveData {
                title: format!("S-curve: cumulative cost per {period}"),
                unit: plan.project.currency.clone().unwrap_or_default(),
                labels: r
                    .points
                    .iter()
                    .map(|p| period_label(&p.start, period))
                    .collect(),
                series,
                ceiling: (r.budget > r.total).then(|| ("budget".to_string(), r.budget)),
            };
            Ok(alma_layout::charts::curve(
                &data,
                opts.theme,
                opts.font_size,
            ))
        }
        other => Err(format!(
            "unknown chart `{other}`; use gantt, histogram or scurve"
        )),
    }
}

/// The requested chart (the Gantt chart by default) as SVG text.
pub fn svg(plan: &Plan, schedule: &Schedule, req: &RenderRequest) -> Result<String, String> {
    Ok(alma_export::to_svg(&display(plan, schedule, req)?))
}

/// The requested chart as PNG bytes (2 px per point).
pub fn png(plan: &Plan, schedule: &Schedule, req: &RenderRequest) -> Result<Vec<u8>, String> {
    alma_export::to_png(&display(plan, schedule, req)?, 2.0).map_err(|e| e.to_string())
}

/// Largest comfortable chart side, in points (an A0 sheet is about 3,370 pt wide).
const MAX_COMFORT: f64 = 3400.0;

/// Advice about a render, e.g. a chart too wide to read or print. Empty when all is well.
pub fn warnings(
    plan: &Plan,
    schedule: &Schedule,
    req: &RenderRequest,
) -> Result<Vec<String>, String> {
    let list = display(plan, schedule, req)?;
    let mut out = Vec::new();
    if list.width > MAX_COMFORT {
        out.push(format!(
            "the chart is {:.0} pt wide (wider than A0); use a coarser --scale or a shorter --range",
            list.width
        ));
    }
    if list.height > MAX_COMFORT {
        out.push(format!(
            "the chart is {:.0} pt tall; use --depth to show fewer WBS levels",
            list.height
        ));
    }
    Ok(out)
}

/// The task table as CSV (RFC 4180 quoting), one row per task in WBS order.
pub fn csv(plan: &Plan, schedule: &Schedule) -> String {
    let mut out =
        String::from("wbs,id,name,kind,start,finish,duration,total_float_days,critical\n");
    let q = |s: &str| {
        if s.contains([',', '"', '\n']) {
            format!("\"{}\"", s.replace('"', "\"\""))
        } else {
            s.to_string()
        }
    };
    for v in task_views(plan, schedule) {
        let tf = v.tf.map(|t| t.to_string()).unwrap_or_default();
        out.push_str(&format!(
            "{},{},{},{},{},{},{},{},{}\n",
            q(&v.wbs),
            q(&v.id),
            q(&v.name),
            v.kind,
            v.start,
            v.finish,
            v.duration,
            tf,
            v.critical
        ));
    }
    out
}

/// The cost workbook (ADR 0016): cost per period by resource, fixed-cost category and
/// contingency, with total and cumulative rows (plus planned value and funding when the plan
/// has them); the task table; resources; risks.
pub fn xlsx(
    plan: &Plan,
    schedule: &Schedule,
    period: &str,
    baseline: Option<&alma_core::model::Snapshot>,
) -> Result<Vec<u8>, String> {
    use alma_export::xlsx::{Cell, Sheet};
    use alma_sched::cost::{Source, costs, share};
    use rust_decimal::Decimal;

    let curve = crate::cost::scurve(plan, schedule, period, baseline)?;
    let p = alma_sched::resources::Period::parse(period)
        .ok_or_else(|| format!("period `{period}` must be day, week or month"))?;
    let c = costs(plan, schedule);
    let cals = alma_sched::cpm::Calendars::new(plan);
    let buckets: Vec<(Date, Date)> = curve
        .points
        .iter()
        .map(|pt| {
            let start: Date = pt.start.parse().unwrap_or_default();
            (start, p.next(start))
        })
        .collect();
    // Rows in a stable order: resources (plan order), fixed-cost categories (by name),
    // contingency.
    let mut keys: Vec<(String, String)> = Vec::new();
    let key_of = |source: Source, category: &Option<String>| -> (String, String) {
        match source {
            Source::Resource(r) => (
                plan.resources[r].id.clone(),
                plan.resources[r].kind.as_str().to_string(),
            ),
            Source::Fixed => (
                format!(
                    "fixed: {}",
                    category.clone().unwrap_or_else(|| "uncategorized".into())
                ),
                "fixed".into(),
            ),
            Source::Contingency(_) => ("contingency".into(), "reserve".into()),
        }
    };
    for r in &plan.resources {
        if c.lines
            .iter()
            .any(|l| matches!(l.source, Source::Resource(i) if plan.resources[i].id == r.id))
        {
            keys.push((r.id.clone(), r.kind.as_str().to_string()));
        }
    }
    let mut fixed: Vec<(String, String)> = c
        .lines
        .iter()
        .filter(|l| l.source == Source::Fixed)
        .map(|l| key_of(l.source, &l.category))
        .collect();
    fixed.sort();
    fixed.dedup();
    keys.extend(fixed);
    if c.contingency > Decimal::ZERO {
        keys.push(("contingency".into(), "reserve".into()));
    }
    let mut table = vec![vec![Decimal::ZERO; buckets.len()]; keys.len()];
    for l in &c.lines {
        let k = key_of(l.source, &l.category);
        let Some(row) = keys.iter().position(|x| *x == k) else {
            continue;
        };
        for (i, (a, b)) in buckets.iter().enumerate() {
            table[row][i] += share(
                plan,
                &cals,
                l,
                alma_core::time::start_of(*a),
                alma_core::time::start_of(*b),
            );
        }
    }
    let label = |i: usize| curve.points[i].start.clone();
    let mut header = vec!["Item".to_string(), "Kind".to_string()];
    header.extend((0..buckets.len()).map(label));
    header.push("Total".into());
    let money = |d: Decimal| Cell::Money(crate::cost::money(d));
    let mut rows: Vec<Vec<Cell>> = keys
        .iter()
        .zip(&table)
        .map(|((name, kind), values)| {
            let mut r = vec![Cell::Text(name.clone()), Cell::Text(kind.clone())];
            r.extend(values.iter().map(|v| money(*v)));
            r.push(money(values.iter().copied().sum()));
            r
        })
        .collect();
    let mut total = vec![Cell::Text("Total".into()), Cell::Empty];
    total.extend(curve.points.iter().map(|pt| Cell::Money(pt.cost)));
    total.push(Cell::Money(curve.total));
    rows.push(total);
    let mut cum = vec![Cell::Text("Cumulative".into()), Cell::Empty];
    cum.extend(curve.points.iter().map(|pt| Cell::Money(pt.cumulative)));
    rows.push(cum);
    if let Some(b) = &curve.baseline {
        let mut r = vec![
            Cell::Text(format!("Planned value ({b}), cumulative")),
            Cell::Empty,
        ];
        r.extend(
            curve
                .points
                .iter()
                .map(|pt| Cell::Money(pt.pv.unwrap_or(0.0))),
        );
        rows.push(r);
    }
    if curve.points.iter().any(|pt| pt.funded.is_some()) {
        let mut r = vec![Cell::Text("Funded, cumulative".into()), Cell::Empty];
        r.extend(
            curve
                .points
                .iter()
                .map(|pt| Cell::Money(pt.funded.unwrap_or(0.0))),
        );
        rows.push(r);
    }
    let currency = plan.project.currency.clone().unwrap_or_default();
    let mut notes = vec![
        format!("Project: {}", plan.project.name),
        format!(
            "Currency: {currency}. Cost per {period} of the current schedule (a forecast), contingency included; columns are the first day of each {period}."
        ),
    ];
    if c.management > Decimal::ZERO {
        notes.push(format!(
            "Management reserve outside the cost baseline: {:.2}; project budget {:.2}.",
            crate::cost::money(c.management),
            curve.budget
        ));
    }
    let per_period = Sheet {
        name: "Cost per period".into(),
        notes,
        header,
        rows,
        bold_rows: vec!["Total".into(), "Cumulative".into()],
    };

    let views = task_views(plan, schedule);
    let tasks = Sheet {
        name: "Tasks".into(),
        notes: Vec::new(),
        header: [
            "WBS",
            "Id",
            "Name",
            "Kind",
            "Start",
            "Finish",
            "Duration",
            "Total float (d)",
            "Critical",
            "Cost",
        ]
        .map(String::from)
        .to_vec(),
        rows: views
            .iter()
            .map(|v| {
                vec![
                    Cell::Text(v.wbs.clone()),
                    Cell::Text(v.id.clone()),
                    Cell::Text(v.name.clone()),
                    Cell::Text(v.kind.clone()),
                    Cell::Date(v.start.clone()),
                    Cell::Date(v.finish.clone()),
                    Cell::Text(v.duration.clone()),
                    v.tf.map_or(Cell::Empty, Cell::Number),
                    Cell::Text(if v.critical { "yes" } else { "" }.into()),
                    Cell::Money(v.cost),
                ]
            })
            .collect(),
        bold_rows: Vec::new(),
    };
    let res = crate::resources::list(plan, schedule);
    let resources = Sheet {
        name: "Resources".into(),
        notes: Vec::new(),
        header: [
            "Id",
            "Name",
            "Kind",
            "Rate",
            "Max units",
            "Work (h)",
            "Cost",
        ]
        .map(String::from)
        .to_vec(),
        rows: res
            .iter()
            .zip(&c.by_resource)
            .map(|(r, cost)| {
                vec![
                    Cell::Text(r.id.clone()),
                    Cell::Text(r.name.clone()),
                    Cell::Text(r.kind.clone()),
                    Cell::Text(r.rate.clone().unwrap_or_default()),
                    Cell::Number(r.max_units),
                    Cell::Number(r.work_h),
                    money(*cost),
                ]
            })
            .collect(),
        bold_rows: Vec::new(),
    };
    let mut sheets = vec![per_period, tasks, resources];
    if !plan.risks.is_empty() {
        let r = crate::finance::risks(plan);
        sheets.push(Sheet {
            name: "Risks".into(),
            notes: vec![format!(
                "EMV of open risks {:.2}; contingency {:.2}.",
                r.total_emv, r.contingency
            )],
            header: [
                "Id",
                "Name",
                "Status",
                "Sign",
                "Probability",
                "Impact",
                "EMV",
                "Reserves",
            ]
            .map(String::from)
            .to_vec(),
            rows: r
                .risks
                .iter()
                .map(|k| {
                    vec![
                        Cell::Text(k.id.clone()),
                        Cell::Text(k.name.clone()),
                        Cell::Text(k.status.clone()),
                        Cell::Text(k.sign.clone()),
                        Cell::Number(k.probability),
                        k.impact.map_or(Cell::Empty, Cell::Money),
                        Cell::Money(k.emv),
                        Cell::Text(k.reserves.join(", ")),
                    ]
                })
                .collect(),
            bold_rows: Vec::new(),
        });
    }
    alma_export::xlsx::to_xlsx(&plan.project.name, &sheets).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges() {
        let d = |y, m, dd| Date::new(y, m, dd).unwrap();
        assert_eq!(
            parse_range("2027-01..2027-06"),
            Ok((d(2027, 1, 1), d(2027, 6, 30)))
        );
        assert_eq!(
            parse_range("2027-01-04..2027-02-12"),
            Ok((d(2027, 1, 4), d(2027, 2, 12)))
        );
        assert!(parse_range("2027-06..2027-01").is_err());
        assert!(parse_range("june").is_err());
    }
}
