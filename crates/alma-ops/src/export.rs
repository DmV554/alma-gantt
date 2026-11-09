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
