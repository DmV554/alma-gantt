//! Exports: Gantt chart as SVG or PNG, task table as CSV (SPEC.md §5 M0).

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

/// The Gantt chart as SVG text.
pub fn svg(plan: &Plan, schedule: &Schedule, req: &RenderRequest) -> Result<String, String> {
    let list = alma_layout::gantt(plan, schedule, &options(req)?);
    Ok(alma_export::to_svg(&list))
}

/// The Gantt chart as PNG bytes (2 px per point).
pub fn png(plan: &Plan, schedule: &Schedule, req: &RenderRequest) -> Result<Vec<u8>, String> {
    let list = alma_layout::gantt(plan, schedule, &options(req)?);
    alma_export::to_png(&list, 2.0).map_err(|e| e.to_string())
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
