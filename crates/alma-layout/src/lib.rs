//! View model → display list: the geometry of a Gantt chart for a given time range and scale
//! (SPEC.md §9). Pure: no I/O, no clock. `alma-export` turns the display list into SVG or PNG;
//! the desktop UI will draw the same list on a canvas.
//!
//! Units are points (1/72 in). Text widths are estimated from an average glyph width until the
//! renderer ships a bundled font (ADR 0009).

use std::collections::HashMap;

use alma_core::model::{DepType, Plan, TaskKind};
use alma_core::time::{DAY, Instant, date_of_day, day_number, day_of, start_of, weekday};
use alma_sched::cpm::Calendars;
use alma_sched::{PlanIndex, Schedule};
use jiff::civil::Date;

/// An sRGB color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color(pub u8, pub u8, pub u8);

impl Color {
    const fn hex(v: u32) -> Self {
        Color((v >> 16) as u8, (v >> 8) as u8, v as u8)
    }
}

/// Semantic colors (same tokens as the prototype).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    /// Page background.
    pub bg: Color,
    /// Text and summary bars.
    pub ink: Color,
    /// Secondary text.
    pub muted: Color,
    /// Grid lines.
    pub line: Color,
    /// Task bars.
    pub bar: Color,
    /// Light bars (level of effort).
    pub bar_soft: Color,
    /// Critical bars and arrows.
    pub crit: Color,
    /// Sprint bands.
    pub sprint: Color,
    /// Non-working days.
    pub weekend: Color,
}

impl Theme {
    /// Light theme.
    pub const LIGHT: Theme = Theme {
        bg: Color::hex(0xffffff),
        ink: Color::hex(0x1c2220),
        muted: Color::hex(0x66706b),
        line: Color::hex(0xdde1dc),
        bar: Color::hex(0x3d6b6e),
        bar_soft: Color::hex(0xd5e3e2),
        crit: Color::hex(0xc0432f),
        sprint: Color::hex(0xe8eee0),
        weekend: Color::hex(0xf4f5f2),
    };
    /// Dark theme.
    pub const DARK: Theme = Theme {
        bg: Color::hex(0x1c2120),
        ink: Color::hex(0xe4e8e5),
        muted: Color::hex(0x95a09a),
        line: Color::hex(0x2e3533),
        bar: Color::hex(0x6fa3a6),
        bar_soft: Color::hex(0x27403f),
        crit: Color::hex(0xf07a62),
        sprint: Color::hex(0x1f2a22),
        weekend: Color::hex(0x151918),
    };
}

/// Font weight.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Weight {
    /// Regular.
    Normal,
    /// Bold.
    Bold,
}

/// Horizontal text anchor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Anchor {
    /// `x` is the left edge.
    Start,
    /// `x` is the center.
    Middle,
    /// `x` is the right edge.
    End,
}

/// One drawing primitive.
#[derive(Debug, Clone, PartialEq)]
pub enum Prim {
    /// A rectangle.
    Rect {
        /// Left.
        x: f64,
        /// Top.
        y: f64,
        /// Width.
        w: f64,
        /// Height.
        h: f64,
        /// Fill.
        fill: Option<Color>,
        /// Stroke.
        stroke: Option<Color>,
        /// Dashed stroke.
        dashed: bool,
    },
    /// A straight line.
    Line {
        /// Start x.
        x1: f64,
        /// Start y.
        y1: f64,
        /// End x.
        x2: f64,
        /// End y.
        y2: f64,
        /// Color.
        stroke: Color,
        /// Width.
        width: f64,
    },
    /// A polyline or polygon through `points`.
    Poly {
        /// Points.
        points: Vec<(f64, f64)>,
        /// Closed and filled with this color.
        fill: Option<Color>,
        /// Stroke color.
        stroke: Option<Color>,
        /// Stroke width.
        width: f64,
    },
    /// Text on a baseline.
    Text {
        /// Anchor x.
        x: f64,
        /// Baseline y.
        y: f64,
        /// Content.
        text: String,
        /// Size in points.
        size: f64,
        /// Weight.
        weight: Weight,
        /// Anchor.
        anchor: Anchor,
        /// Color.
        fill: Color,
    },
    /// Start clipping following primitives to a rectangle.
    ClipStart {
        /// Left.
        x: f64,
        /// Top.
        y: f64,
        /// Width.
        w: f64,
        /// Height.
        h: f64,
    },
    /// End the current clip.
    ClipEnd,
}

/// A finished drawing.
#[derive(Debug, Clone, PartialEq)]
pub struct DisplayList {
    /// Width in points.
    pub width: f64,
    /// Height in points.
    pub height: f64,
    /// Background color.
    pub background: Color,
    /// Primitives, back to front.
    pub prims: Vec<Prim>,
}

/// Time scale of the timeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scale {
    /// One column per day.
    Day,
    /// One column per week.
    Week,
    /// One column per month.
    Month,
}

/// Layout options.
#[derive(Debug, Clone)]
pub struct Options {
    /// First and last day shown; `None` fits the project.
    pub range: Option<(Date, Date)>,
    /// Time scale; `None` picks one from the range length.
    pub scale: Option<Scale>,
    /// Only tasks at or above this WBS depth (0 = top level).
    pub max_depth: Option<usize>,
    /// Base font size (points); never below 9.
    pub font_size: f64,
    /// Draw dependency arrows.
    pub arrows: bool,
    /// Colors.
    pub theme: Theme,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            range: None,
            scale: None,
            max_depth: None,
            font_size: 10.0,
            arrows: true,
            theme: Theme::LIGHT,
        }
    }
}

/// Minimum font size for print (SPEC.md §1, §4).
pub const MIN_FONT: f64 = 9.0;

/// Estimated text width: average glyph width of a sans-serif face (ADR 0009).
pub fn text_width(text: &str, size: f64, weight: Weight) -> f64 {
    let factor = match weight {
        Weight::Normal => 0.54,
        Weight::Bold => 0.58,
    };
    text.chars().count() as f64 * size * factor
}

/// Truncates `text` with an ellipsis to fit `width`.
pub fn fit_text(text: &str, size: f64, weight: Weight, width: f64) -> String {
    if text_width(text, size, weight) <= width {
        return text.to_string();
    }
    let mut out: String = String::new();
    for c in text.chars() {
        let candidate = format!("{out}{c}…");
        if text_width(&candidate, size, weight) > width {
            break;
        }
        out.push(c);
    }
    format!("{}…", out.trim_end())
}

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// Lays out a Gantt chart of the scheduled plan.
pub fn gantt(plan: &Plan, schedule: &Schedule, opts: &Options) -> DisplayList {
    let theme = opts.theme;
    let fs = opts.font_size.max(MIN_FONT);
    let small = (fs * 0.9).max(MIN_FONT);
    let row_h = (fs * 2.0).round();
    let pad = fs;
    let index = PlanIndex::new(plan);
    let cals = Calendars::new(plan);

    // Rows: WBS order, cut at max_depth.
    let rows: Vec<usize> = (0..plan.tasks.len())
        .filter(|&i| opts.max_depth.is_none_or(|d| index.depth(i) <= d))
        .collect();
    let row_of: HashMap<usize, usize> = rows.iter().enumerate().map(|(r, &i)| (i, r)).collect();
    let codes = wbs_codes(plan, &index);

    // Time range (whole days).
    let (first_day, last_day) = match opts.range {
        Some((a, b)) => (day_number(a), day_number(b).max(day_number(a))),
        None => {
            let lo = schedule
                .tasks
                .iter()
                .map(|d| d.es)
                .min()
                .unwrap_or(schedule.start);
            let mut hi = schedule
                .tasks
                .iter()
                .map(|d| d.ef)
                .max()
                .unwrap_or(schedule.finish);
            // Include the last sprint, so a cadence is never cut off.
            let cal = cals.project();
            for c in &plan.cadences {
                let mut t = cal.next_start(start_of(c.start));
                for _ in 0..c.count {
                    t = cal.next_start(cal.add(t, c.length.minutes));
                }
                hi = hi.max(t);
            }
            // And every deadline, which is drawn as a marker.
            for t in &plan.tasks {
                if let Some(d) = t.deadline {
                    hi = hi.max(start_of(d) + DAY);
                }
            }
            (day_of(lo) - 2, day_of(hi - 1) + 3)
        }
    };
    let span = (last_day - first_day + 1) as f64;
    let scale = opts.scale.unwrap_or(if span <= 70.0 {
        Scale::Day
    } else if span <= 400.0 {
        Scale::Week
    } else {
        Scale::Month
    });
    let px_day = match scale {
        Scale::Day => (fs * 1.8).round(),
        Scale::Week => (fs * 2.6 / 7.0 * 10.0).round() / 10.0,
        Scale::Month => (fs * 4.0 / 30.0 * 100.0).round() / 100.0,
    };
    // A fitted range leaves room after the last bar for a milestone label ("30 Jul").
    let last_day = match opts.range {
        Some(_) => last_day,
        None => last_day + (text_width("30 Sep", small, Weight::Normal) / px_day).ceil() as i64,
    };
    let days = (last_day - first_day + 1) as f64;

    // Columns.
    let code_w = rows
        .iter()
        .map(|&i| text_width(&codes[i], small, Weight::Normal))
        .fold(text_width("WBS", small, Weight::Bold), f64::max)
        + pad;
    let name_w = (fs * 24.0).round();
    let left = pad + code_w + name_w;
    let chart_w = days * px_day;
    let title_h = fs * 2.4;
    let has_sprints = !plan.cadences.is_empty();
    let header_h = row_h * if has_sprints { 3.0 } else { 2.0 };
    let top = title_h + header_h;
    let width = (left + chart_w + pad).ceil();
    let height = (top + rows.len() as f64 * row_h + pad).ceil();

    let x_of = |t: Instant| left + (t - first_day * DAY) as f64 / DAY as f64 * px_day;
    let x_day = |d: i64| left + (d - first_day) as f64 * px_day;
    let row_y = |r: usize| top + r as f64 * row_h;

    let mut p: Vec<Prim> = Vec::new();
    let text =
        |x: f64, y: f64, s: String, size: f64, weight: Weight, anchor: Anchor, fill: Color| {
            Prim::Text {
                x,
                y,
                text: s,
                size,
                weight,
                anchor,
                fill,
            }
        };

    // Title.
    p.push(text(
        pad,
        title_h * 0.65,
        plan.project.name.clone(),
        fs * 1.3,
        Weight::Bold,
        Anchor::Start,
        theme.ink,
    ));

    // Non-working days (day scale) on the project calendar.
    let cal = cals.project();
    if scale == Scale::Day {
        for d in first_day..=last_day {
            if cal.intervals(d).is_empty() {
                p.push(Prim::Rect {
                    x: x_day(d),
                    y: title_h,
                    w: px_day,
                    h: height - title_h - pad,
                    fill: Some(theme.weekend),
                    stroke: None,
                    dashed: false,
                });
            }
        }
    }

    // Sprint bands.
    let band_y = title_h + row_h * 2.0;
    for c in &plan.cadences {
        let ccal = c
            .calendar
            .as_ref()
            .and_then(|id| plan.calendars.iter().position(|x| &x.id == id))
            .map(|i| alma_sched::WorkCalendar::new(&plan.calendars[i]))
            .unwrap_or_else(|| cal.clone());
        let mut start = ccal.next_start(start_of(c.start));
        for (n, id) in c.iteration_ids().into_iter().enumerate() {
            let end = ccal.add(start, c.length.minutes);
            let (x1, x2) = (x_of(start).max(left), x_of(end).min(left + chart_w));
            if x2 > x1 {
                if n % 2 == 0 {
                    p.push(Prim::Rect {
                        x: x1,
                        y: band_y,
                        w: x2 - x1,
                        h: height - band_y - pad,
                        fill: Some(theme.sprint),
                        stroke: None,
                        dashed: false,
                    });
                }
                let label = fit_text(&id, small, Weight::Normal, x2 - x1 - 2.0);
                if !label.starts_with('…') {
                    p.push(text(
                        (x1 + x2) / 2.0,
                        band_y + row_h * 0.65,
                        label,
                        small,
                        Weight::Normal,
                        Anchor::Middle,
                        theme.muted,
                    ));
                }
            }
            start = ccal.next_start(end);
        }
    }

    // Time header and vertical grid.
    let h1 = title_h + row_h * 0.65;
    let h2 = title_h + row_h * 1.65;
    let mut d = first_day;
    while d <= last_day {
        let date = date_of_day(d);
        let month_start = date.day() == 1 || d == first_day;
        if month_start {
            let next = {
                let mut n = d + 1;
                while n <= last_day && date_of_day(n).day() != 1 {
                    n += 1;
                }
                n
            };
            let w = x_day(next) - x_day(d);
            let label = format!("{} {}", MONTHS[(date.month() - 1) as usize], date.year());
            let label = if text_width(&label, small, Weight::Bold) <= w - 4.0 {
                label
            } else {
                fit_text(
                    MONTHS[(date.month() - 1) as usize],
                    small,
                    Weight::Bold,
                    w - 2.0,
                )
            };
            if !label.starts_with('…') {
                p.push(text(
                    x_day(d) + 3.0,
                    h1,
                    label,
                    small,
                    Weight::Bold,
                    Anchor::Start,
                    theme.ink,
                ));
            }
            p.push(Prim::Line {
                x1: x_day(d),
                y1: title_h,
                x2: x_day(d),
                y2: height - pad,
                stroke: theme.line,
                width: 0.8,
            });
        }
        let tick = match scale {
            Scale::Day => true,
            Scale::Week => weekday(d) == 0,
            Scale::Month => false,
        };
        if tick {
            let label = date.day().to_string();
            p.push(text(
                x_day(d) + 2.0,
                h2,
                label,
                small,
                Weight::Normal,
                Anchor::Start,
                theme.muted,
            ));
            if !month_start {
                p.push(Prim::Line {
                    x1: x_day(d),
                    y1: title_h + row_h,
                    x2: x_day(d),
                    y2: height - pad,
                    stroke: theme.line,
                    width: 0.4,
                });
            }
        }
        d += 1;
    }

    // Column headers and separators.
    p.push(text(
        pad,
        h2,
        "WBS".into(),
        small,
        Weight::Bold,
        Anchor::Start,
        theme.muted,
    ));
    p.push(text(
        pad + code_w,
        h2,
        "Task".into(),
        small,
        Weight::Bold,
        Anchor::Start,
        theme.muted,
    ));
    for y in [title_h, top] {
        p.push(Prim::Line {
            x1: pad,
            y1: y,
            x2: left + chart_w,
            y2: y,
            stroke: theme.line,
            width: 0.8,
        });
    }
    p.push(Prim::Line {
        x1: left,
        y1: title_h,
        x2: left,
        y2: height - pad,
        stroke: theme.line,
        width: 0.8,
    });

    // Rows: labels and row lines.
    for (r, &i) in rows.iter().enumerate() {
        let t = &plan.tasks[i];
        let y = row_y(r);
        let base = y + row_h * 0.66;
        let summary = index.is_summary(i);
        let weight = if summary {
            Weight::Bold
        } else {
            Weight::Normal
        };
        p.push(text(
            pad,
            base,
            codes[i].clone(),
            small,
            Weight::Normal,
            Anchor::Start,
            theme.muted,
        ));
        let indent = index.depth(i) as f64 * fs;
        let avail = name_w - indent - pad * 0.5;
        p.push(text(
            pad + code_w + indent,
            base,
            fit_text(&t.name, fs, weight, avail),
            fs,
            weight,
            Anchor::Start,
            theme.ink,
        ));
        p.push(Prim::Line {
            x1: pad,
            y1: y + row_h,
            x2: left + chart_w,
            y2: y + row_h,
            stroke: theme.line,
            width: 0.3,
        });
    }

    // Bars, clipped to the timeline.
    p.push(Prim::ClipStart {
        x: left,
        y: top,
        w: chart_w,
        h: rows.len() as f64 * row_h,
    });
    let bar_h = row_h * 0.46;
    for (r, &i) in rows.iter().enumerate() {
        let t = &plan.tasks[i];
        let d = &schedule.tasks[i];
        let mid = row_y(r) + row_h / 2.0;
        let color = if d.critical { theme.crit } else { theme.bar };
        let (x1, x2) = (x_of(d.es), x_of(d.ef));
        if index.is_summary(i) {
            let h = bar_h * 0.4;
            p.push(Prim::Rect {
                x: x1,
                y: mid - h,
                w: (x2 - x1).max(1.0),
                h,
                fill: Some(theme.ink),
                stroke: None,
                dashed: false,
            });
            for x in [x1, x2] {
                p.push(Prim::Poly {
                    points: vec![(x - 3.0, mid - h), (x + 3.0, mid - h), (x, mid + h)],
                    fill: Some(theme.ink),
                    stroke: None,
                    width: 0.0,
                });
            }
            continue;
        }
        if let Some(deadline) = t.deadline {
            // A small downward triangle and a line at the end of the deadline day.
            let x = x_of(start_of(deadline) + DAY);
            let top_y = row_y(r) + 2.0;
            p.push(Prim::Line {
                x1: x,
                y1: top_y,
                x2: x,
                y2: row_y(r) + row_h - 2.0,
                stroke: theme.crit,
                width: 1.0,
            });
            p.push(Prim::Poly {
                points: vec![(x - 3.5, top_y), (x + 3.5, top_y), (x, top_y + 4.0)],
                fill: Some(theme.crit),
                stroke: None,
                width: 0.0,
            });
        }
        match t.kind {
            TaskKind::Milestone => {
                let s = bar_h * 0.6;
                p.push(Prim::Poly {
                    points: vec![(x1, mid - s), (x1 + s, mid), (x1, mid + s), (x1 - s, mid)],
                    fill: Some(if d.critical { theme.crit } else { theme.ink }),
                    stroke: None,
                    width: 0.0,
                });
                p.push(text(
                    x1 + s + 3.0,
                    mid + small * 0.35,
                    start_label(d.es),
                    small,
                    Weight::Normal,
                    Anchor::Start,
                    theme.muted,
                ));
            }
            TaskKind::Loe => p.push(Prim::Rect {
                x: x1,
                y: mid - bar_h / 2.0,
                w: (x2 - x1).max(1.0),
                h: bar_h,
                fill: Some(theme.bar_soft),
                stroke: None,
                dashed: false,
            }),
            TaskKind::Buffer => p.push(Prim::Rect {
                x: x1,
                y: mid - bar_h / 2.0,
                w: (x2 - x1).max(1.0),
                h: bar_h,
                fill: None,
                stroke: Some(color),
                dashed: true,
            }),
            TaskKind::Task => p.push(Prim::Rect {
                x: x1,
                y: mid - bar_h / 2.0,
                w: (x2 - x1).max(1.5),
                h: bar_h,
                fill: Some(color),
                stroke: None,
                dashed: false,
            }),
        }
    }

    // Dependency arrows between shown rows.
    if opts.arrows {
        for (si, t) in plan.tasks.iter().enumerate() {
            let Some(&sr) = row_of.get(&si) else { continue };
            for dep in &t.deps {
                let Some(&pi) = index.by_id.get(&dep.pred) else {
                    continue;
                };
                let Some(&pr) = row_of.get(&pi) else { continue };
                let (pd, sd) = (&schedule.tasks[pi], &schedule.tasks[si]);
                let (from_end, to_end) = match dep.kind {
                    DepType::Fs => (true, false),
                    DepType::Ss => (false, false),
                    DepType::Ff => (true, true),
                    DepType::Sf => (false, true),
                };
                let x1 = x_of(if from_end { pd.ef } else { pd.es });
                let x2 = x_of(if to_end { sd.ef } else { sd.es });
                let (y1, y2) = (row_y(pr) + row_h / 2.0, row_y(sr) + row_h / 2.0);
                let color = if pd.critical && sd.critical {
                    theme.crit
                } else {
                    theme.muted
                };
                p.push(arrow(x1, y1, x2, y2, to_end, row_h, color));
            }
        }
    }
    p.push(Prim::ClipEnd);

    DisplayList {
        width,
        height,
        background: theme.bg,
        prims: p,
    }
}

/// An orthogonal arrow from `(x1, y1)` to `(x2, y2)`; `into_end` points it left.
fn arrow(x1: f64, y1: f64, x2: f64, y2: f64, into_end: bool, row_h: f64, color: Color) -> Prim {
    let gap = 5.0;
    let mut pts = vec![(x1, y1)];
    let out = if into_end { x2.max(x1) + gap } else { x1 + gap };
    let enter = if into_end { x2 + gap } else { x2 - gap };
    if into_end || enter >= out {
        let xm = if into_end {
            out.max(enter)
        } else {
            out.max(x1 + gap).min(enter)
        };
        pts.push((xm, y1));
        pts.push((xm, y2));
    } else {
        let ymid = if y2 > y1 {
            y2 - row_h / 2.0
        } else {
            y2 + row_h / 2.0
        };
        pts.push((out, y1));
        pts.push((out, ymid));
        pts.push((enter, ymid));
        pts.push((enter, y2));
    }
    pts.push((x2, y2));
    let head = 3.0;
    let dir = if into_end { 1.0 } else { -1.0 };
    pts.extend([
        (x2 + dir * head, y2 - head),
        (x2, y2),
        (x2 + dir * head, y2 + head),
    ]);
    Prim::Poly {
        points: pts,
        fill: None,
        stroke: Some(color),
        width: 0.7,
    }
}

/// Day-and-month label of a milestone; an instant at 00:00 belongs to the previous day.
fn start_label(t: Instant) -> String {
    let day = if alma_core::time::minute_of_day(t) == 0 {
        day_of(t) - 1
    } else {
        day_of(t)
    };
    let d = date_of_day(day);
    format!("{} {}", d.day(), MONTHS[(d.month() - 1) as usize])
}

/// WBS codes (1.2.3) in source order.
pub fn wbs_codes(plan: &Plan, index: &PlanIndex) -> Vec<String> {
    let mut codes = vec![String::new(); plan.tasks.len()];
    let mut counters = vec![0usize; plan.tasks.len()];
    let mut roots = 0;
    for i in 0..plan.tasks.len() {
        codes[i] = match index.parent[i] {
            None => {
                roots += 1;
                roots.to_string()
            }
            Some(p) => {
                counters[p] += 1;
                if codes[p].is_empty() {
                    counters[p].to_string()
                } else {
                    format!("{}.{}", codes[p], counters[p])
                }
            }
        };
    }
    codes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_text_truncates_with_ellipsis() {
        assert_eq!(fit_text("short", 10.0, Weight::Normal, 100.0), "short");
        let t = fit_text("a very long task name indeed", 10.0, Weight::Normal, 60.0);
        assert!(t.ends_with('…'));
        assert!(text_width(&t, 10.0, Weight::Normal) <= 60.0);
    }

    #[test]
    fn arrow_ends_at_target() {
        let Prim::Poly { points, .. } = arrow(10.0, 5.0, 40.0, 25.0, false, 20.0, Color(0, 0, 0))
        else {
            panic!()
        };
        assert!(points.contains(&(40.0, 25.0)));
    }
}
