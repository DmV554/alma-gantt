//! Report charts as display lists: the resource histogram (knowledge notes, resources §1.5) and
//! the S-curve (§5.2). The input is plain per-period numbers already computed by the engine, so
//! these functions only do geometry. Pure, like the Gantt layout.

use crate::{Anchor, Color, DisplayList, Prim, Theme, Weight, text_width};

/// Per-period demand and capacity of one resource, in hours.
#[derive(Debug, Clone, PartialEq)]
pub struct HistogramData {
    /// Title, e.g. "dev — Developer".
    pub title: String,
    /// Period labels, one per bar.
    pub labels: Vec<String>,
    /// Demand per period.
    pub demand: Vec<f64>,
    /// Capacity per period.
    pub capacity: Vec<f64>,
}

/// One line of a line chart.
#[derive(Debug, Clone, PartialEq)]
pub struct Series {
    /// Legend label.
    pub name: String,
    /// One value per period (cumulative for an S-curve).
    pub values: Vec<f64>,
    /// Drawn as the main series (bar color) or the reference one (ink, thinner, with markers).
    pub main: bool,
}

/// Cumulative curves per period, with an optional horizontal ceiling.
#[derive(Debug, Clone, PartialEq)]
pub struct CurveData {
    /// Title.
    pub title: String,
    /// Unit shown on the axis, e.g. "UF".
    pub unit: String,
    /// Period labels.
    pub labels: Vec<String>,
    /// Series, drawn in order.
    pub series: Vec<Series>,
    /// A labelled horizontal line, e.g. the project budget.
    pub ceiling: Option<(String, f64)>,
}

const LEFT: f64 = 64.0;
const RIGHT: f64 = 24.0;
const TOP: f64 = 52.0;
const BOTTOM: f64 = 44.0;
const PLOT_H: f64 = 260.0;

/// A round axis step giving about five ticks up to `max`.
fn nice_step(max: f64) -> f64 {
    if max <= 0.0 || !max.is_finite() {
        return 1.0;
    }
    let raw = max / 5.0;
    let mag = 10f64.powf(raw.log10().floor());
    let n = raw / mag;
    let step = if n <= 1.0 {
        1.0
    } else if n <= 2.0 {
        2.0
    } else if n <= 5.0 {
        5.0
    } else {
        10.0
    };
    step * mag
}

fn fmt_tick(v: f64) -> String {
    if v >= 10_000.0 {
        format!("{:.0}k", v / 1000.0)
    } else if v.fract() == 0.0 {
        format!("{v:.0}")
    } else {
        format!("{v:.1}")
    }
}

struct Frame {
    width: f64,
    col: f64,
    top: f64,
    ymax: f64,
}

impl Frame {
    fn new(n: usize, ymax: f64) -> Self {
        let col = if n == 0 {
            40.0
        } else {
            (1100.0 / n as f64).clamp(6.0, 56.0)
        };
        let step = nice_step(ymax);
        let ymax = ((ymax / step).ceil() * step).max(step);
        Self {
            width: LEFT + col * n.max(1) as f64 + RIGHT,
            col,
            top: TOP,
            ymax,
        }
    }

    fn y(&self, v: f64) -> f64 {
        self.top + PLOT_H - (v / self.ymax).clamp(0.0, 1.05) * PLOT_H
    }

    fn x(&self, i: usize) -> f64 {
        LEFT + self.col * i as f64
    }
}

/// Title, y grid with ticks, x labels (thinned so they never overlap) and the frame.
fn axes(
    prims: &mut Vec<Prim>,
    f: &Frame,
    title: &str,
    unit: &str,
    labels: &[String],
    theme: &Theme,
    font: f64,
) {
    prims.push(Prim::Text {
        x: LEFT,
        y: 24.0,
        text: title.to_string(),
        size: font + 2.0,
        weight: Weight::Bold,
        anchor: Anchor::Start,
        fill: theme.ink,
    });
    let step = nice_step(f.ymax);
    let mut v = 0.0;
    while v <= f.ymax + step * 0.01 {
        let y = f.y(v);
        prims.push(Prim::Line {
            x1: LEFT,
            y1: y,
            x2: f.width - RIGHT,
            y2: y,
            stroke: theme.line,
            width: 0.5,
        });
        prims.push(Prim::Text {
            x: LEFT - 6.0,
            y: y + font * 0.35,
            text: fmt_tick(v),
            size: font,
            weight: Weight::Normal,
            anchor: Anchor::End,
            fill: theme.muted,
        });
        v += step;
    }
    prims.push(Prim::Text {
        x: LEFT - 6.0,
        y: TOP - 10.0,
        text: unit.to_string(),
        size: font,
        weight: Weight::Normal,
        anchor: Anchor::End,
        fill: theme.muted,
    });
    let widest = labels
        .iter()
        .map(|l| text_width(l, font, Weight::Normal))
        .fold(0.0, f64::max);
    let every = ((widest + 8.0) / f.col).ceil().max(1.0) as usize;
    for (i, l) in labels.iter().enumerate().step_by(every) {
        prims.push(Prim::Text {
            x: f.x(i) + f.col / 2.0,
            y: TOP + PLOT_H + 16.0,
            text: l.clone(),
            size: font,
            weight: Weight::Normal,
            anchor: Anchor::Middle,
            fill: theme.muted,
        });
    }
    prims.push(Prim::Line {
        x1: LEFT,
        y1: TOP + PLOT_H,
        x2: f.width - RIGHT,
        y2: TOP + PLOT_H,
        stroke: theme.muted,
        width: 1.0,
    });
}

fn legend(prims: &mut Vec<Prim>, right: f64, items: &[(String, Color)], theme: &Theme, font: f64) {
    let mut x = right;
    for (name, color) in items.iter().rev() {
        let w = text_width(name, font, Weight::Normal);
        x -= w;
        prims.push(Prim::Text {
            x,
            y: 24.0,
            text: name.clone(),
            size: font,
            weight: Weight::Normal,
            anchor: Anchor::Start,
            fill: theme.ink,
        });
        x -= 14.0;
        prims.push(Prim::Rect {
            x,
            y: 24.0 - font * 0.8,
            w: 10.0,
            h: font * 0.8,
            fill: Some(*color),
            stroke: None,
            dashed: false,
        });
        x -= 14.0;
    }
}

/// The resource histogram: demand bars per period, the part above capacity in the critical
/// color, and capacity as a step line.
pub fn histogram(data: &HistogramData, theme: Theme, font: f64) -> DisplayList {
    let font = font.max(crate::MIN_FONT);
    let n = data.labels.len();
    let top = data
        .demand
        .iter()
        .chain(&data.capacity)
        .copied()
        .fold(0.0, f64::max);
    let f = Frame::new(n, top);
    let mut prims = Vec::new();
    axes(&mut prims, &f, &data.title, "h", &data.labels, &theme, font);
    let gap = (f.col * 0.15).min(4.0);
    for i in 0..n {
        let d = data.demand.get(i).copied().unwrap_or(0.0);
        let c = data.capacity.get(i).copied().unwrap_or(0.0);
        let x = f.x(i) + gap / 2.0;
        let w = f.col - gap;
        let ok = d.min(c);
        if ok > 0.0 {
            prims.push(Prim::Rect {
                x,
                y: f.y(ok),
                w,
                h: f.y(0.0) - f.y(ok),
                fill: Some(theme.bar),
                stroke: None,
                dashed: false,
            });
        }
        if d > c {
            prims.push(Prim::Rect {
                x,
                y: f.y(d),
                w,
                h: f.y(ok) - f.y(d),
                fill: Some(theme.crit),
                stroke: None,
                dashed: false,
            });
        }
    }
    // Capacity as a step line across the periods.
    let mut points = Vec::with_capacity(n * 2);
    for i in 0..n {
        let c = data.capacity.get(i).copied().unwrap_or(0.0);
        points.push((f.x(i), f.y(c)));
        points.push((f.x(i + 1), f.y(c)));
    }
    if !points.is_empty() {
        prims.push(Prim::Poly {
            points,
            fill: None,
            stroke: Some(theme.ink),
            width: 1.5,
        });
    }
    legend(
        &mut prims,
        f.width - RIGHT,
        &[
            ("demand".into(), theme.bar),
            ("over capacity".into(), theme.crit),
            ("capacity".into(), theme.ink),
        ],
        &theme,
        font,
    );
    DisplayList {
        width: f.width,
        height: TOP + PLOT_H + BOTTOM,
        background: theme.bg,
        prims,
    }
}

/// A line chart of cumulative values (the S-curve), with an optional ceiling.
pub fn curve(data: &CurveData, theme: Theme, font: f64) -> DisplayList {
    let font = font.max(crate::MIN_FONT);
    let n = data.labels.len();
    let top = data
        .series
        .iter()
        .flat_map(|s| s.values.iter().copied())
        .chain(data.ceiling.as_ref().map(|c| c.1))
        .fold(0.0, f64::max);
    let f = Frame::new(n, top);
    let mut prims = Vec::new();
    axes(
        &mut prims,
        &f,
        &data.title,
        &data.unit,
        &data.labels,
        &theme,
        font,
    );
    if let Some((label, v)) = &data.ceiling {
        let y = f.y(*v);
        prims.push(Prim::Line {
            x1: LEFT,
            y1: y,
            x2: f.width - RIGHT,
            y2: y,
            stroke: theme.crit,
            width: 1.0,
        });
        prims.push(Prim::Text {
            x: LEFT + 4.0,
            y: y - 4.0,
            text: label.clone(),
            size: font,
            weight: Weight::Normal,
            anchor: Anchor::Start,
            fill: theme.crit,
        });
    }
    let mut items = Vec::new();
    for s in &data.series {
        let color = if s.main { theme.bar } else { theme.ink };
        // Values are at the end of each period: start from zero at the first period's start.
        let mut points = vec![(f.x(0), f.y(0.0))];
        points.extend(
            s.values
                .iter()
                .enumerate()
                .map(|(i, v)| (f.x(i + 1), f.y(*v))),
        );
        prims.push(Prim::Poly {
            points: points.clone(),
            fill: None,
            stroke: Some(color),
            width: if s.main { 2.5 } else { 1.5 },
        });
        if !s.main {
            for (x, y) in points.iter().skip(1) {
                prims.push(Prim::Rect {
                    x: x - 2.0,
                    y: y - 2.0,
                    w: 4.0,
                    h: 4.0,
                    fill: Some(color),
                    stroke: None,
                    dashed: false,
                });
            }
        }
        items.push((s.name.clone(), color));
    }
    if let Some((label, _)) = &data.ceiling {
        items.push((label.clone(), theme.crit));
    }
    legend(&mut prims, f.width - RIGHT, &items, &theme, font);
    DisplayList {
        width: f.width,
        height: TOP + PLOT_H + BOTTOM,
        background: theme.bg,
        prims,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nice_steps() {
        assert_eq!(nice_step(100.0), 20.0);
        assert_eq!(nice_step(4714.5), 1000.0);
        assert_eq!(nice_step(7.0), 2.0);
        assert_eq!(nice_step(0.0), 1.0);
    }

    #[test]
    fn over_capacity_is_drawn_in_the_critical_color() {
        let d = HistogramData {
            title: "dev".into(),
            labels: vec!["W1".into(), "W2".into()],
            demand: vec![80.0, 20.0],
            capacity: vec![40.0, 40.0],
        };
        let l = histogram(&d, Theme::LIGHT, 10.0);
        let crit = l
            .prims
            .iter()
            .filter(|p| matches!(p, Prim::Rect { fill: Some(c), w, .. } if *c == Theme::LIGHT.crit && *w > 10.0))
            .count();
        assert_eq!(crit, 1, "only the first week is over capacity");
        assert!(l.width >= LEFT + RIGHT);
    }

    #[test]
    fn empty_data_still_draws_axes() {
        let d = CurveData {
            title: "S".into(),
            unit: "UF".into(),
            labels: Vec::new(),
            series: Vec::new(),
            ceiling: None,
        };
        let l = curve(&d, Theme::DARK, 10.0);
        assert!(!l.prims.is_empty());
        assert!(l.width.is_finite() && l.height.is_finite());
    }
}
