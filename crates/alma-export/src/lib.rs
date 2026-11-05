//! Display list → SVG and PNG (SPEC.md §9). The SVG writer is deterministic: same display list,
//! same bytes. PNG rasterizes that SVG with `resvg` using the system's sans-serif fonts
//! (ADR 0009), so PNG pixels may differ between machines; the SVG does not.

use std::fmt::Write;

use alma_layout::{Anchor, Color, DisplayList, Prim, Weight};
use thiserror::Error;

/// Font stack written into the SVG.
pub const FONT_FAMILY: &str = "IBM Plex Sans, Noto Sans, DejaVu Sans, Arial, sans-serif";

/// Why an export failed.
#[derive(Debug, Error)]
pub enum ExportError {
    /// The generated SVG could not be parsed (a bug).
    #[error("internal SVG error: {0}")]
    Svg(String),
    /// The image is too large to rasterize.
    #[error("image of {0}×{1} px is too large to rasterize; use --range or --depth")]
    TooLarge(u32, u32),
    /// PNG encoding failed.
    #[error("PNG encoding failed: {0}")]
    Png(String),
}

/// Formats a number with at most two decimals and no trailing zeros.
fn n(v: f64) -> String {
    let r = (v * 100.0).round() / 100.0;
    let s = format!("{r:.2}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.to_string() }
}

fn c(c: Color) -> String {
    format!("#{:02x}{:02x}{:02x}", c.0, c.1, c.2)
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Writes a display list as a standalone SVG document. Sizes are in points.
pub fn to_svg(list: &DisplayList) -> String {
    let mut s = String::new();
    let _ = writeln!(
        s,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}pt" height="{h}pt" viewBox="0 0 {w} {h}" font-family="{f}">"#,
        w = n(list.width),
        h = n(list.height),
        f = FONT_FAMILY
    );
    let _ = writeln!(
        s,
        r#"<rect width="{}" height="{}" fill="{}"/>"#,
        n(list.width),
        n(list.height),
        c(list.background)
    );
    let mut clip = 0;
    for p in &list.prims {
        match p {
            Prim::Rect {
                x,
                y,
                w,
                h,
                fill,
                stroke,
                dashed,
            } => {
                let fill = fill.map_or("none".to_string(), c);
                let stroke = stroke.map_or(String::new(), |k| {
                    let dash = if *dashed {
                        r#" stroke-dasharray="3 2""#
                    } else {
                        ""
                    };
                    format!(r#" stroke="{}" stroke-width="0.8"{dash}"#, c(k))
                });
                let _ = writeln!(
                    s,
                    r#"<rect x="{}" y="{}" width="{}" height="{}" fill="{fill}"{stroke}/>"#,
                    n(*x),
                    n(*y),
                    n(*w),
                    n(*h)
                );
            }
            Prim::Line {
                x1,
                y1,
                x2,
                y2,
                stroke,
                width,
            } => {
                let _ = writeln!(
                    s,
                    r#"<line x1="{}" y1="{}" x2="{}" y2="{}" stroke="{}" stroke-width="{}"/>"#,
                    n(*x1),
                    n(*y1),
                    n(*x2),
                    n(*y2),
                    c(*stroke),
                    n(*width)
                );
            }
            Prim::Poly {
                points,
                fill,
                stroke,
                width,
            } => {
                let pts: Vec<String> = points
                    .iter()
                    .map(|(x, y)| format!("{},{}", n(*x), n(*y)))
                    .collect();
                let tag = if fill.is_some() {
                    "polygon"
                } else {
                    "polyline"
                };
                let fill = fill.map_or("none".to_string(), c);
                let stroke = stroke.map_or(String::new(), |k| {
                    format!(
                        r#" stroke="{}" stroke-width="{}" stroke-linejoin="round""#,
                        c(k),
                        n(*width)
                    )
                });
                let _ = writeln!(
                    s,
                    r#"<{tag} points="{}" fill="{fill}"{stroke}/>"#,
                    pts.join(" ")
                );
            }
            Prim::Text {
                x,
                y,
                text,
                size,
                weight,
                anchor,
                fill,
            } => {
                let anchor = match anchor {
                    Anchor::Start => "",
                    Anchor::Middle => r#" text-anchor="middle""#,
                    Anchor::End => r#" text-anchor="end""#,
                };
                let weight = match weight {
                    Weight::Normal => "",
                    Weight::Bold => r#" font-weight="bold""#,
                };
                let _ = writeln!(
                    s,
                    r#"<text x="{}" y="{}" font-size="{}" fill="{}"{anchor}{weight}>{}</text>"#,
                    n(*x),
                    n(*y),
                    n(*size),
                    c(*fill),
                    esc(text)
                );
            }
            Prim::ClipStart { x, y, w, h } => {
                clip += 1;
                let _ = writeln!(
                    s,
                    r#"<clipPath id="clip{clip}"><rect x="{}" y="{}" width="{}" height="{}"/></clipPath><g clip-path="url(#clip{clip})">"#,
                    n(*x),
                    n(*y),
                    n(*w),
                    n(*h)
                );
            }
            Prim::ClipEnd => s.push_str("</g>\n"),
        }
    }
    s.push_str("</svg>\n");
    s
}

/// Largest PNG side, in pixels.
const MAX_SIDE: u32 = 16_384;

/// Rasterizes a display list to PNG at `scale` pixels per point (2 is a good default).
pub fn to_png(list: &DisplayList, scale: f32) -> Result<Vec<u8>, ExportError> {
    let svg = to_svg(list);
    let mut opt = resvg::usvg::Options::default();
    opt.fontdb_mut().load_system_fonts();
    opt.font_family = "sans-serif".into();
    let tree =
        resvg::usvg::Tree::from_str(&svg, &opt).map_err(|e| ExportError::Svg(e.to_string()))?;
    // The SVG is sized in points; usvg converts pt to px at 96/72.
    let size = tree.size();
    let (w, h) = (
        (size.width() * scale).ceil() as u32,
        (size.height() * scale).ceil() as u32,
    );
    if w == 0 || h == 0 || w > MAX_SIDE || h > MAX_SIDE {
        return Err(ExportError::TooLarge(w, h));
    }
    let mut pixmap = resvg::tiny_skia::Pixmap::new(w, h).ok_or(ExportError::TooLarge(w, h))?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    pixmap
        .encode_png()
        .map_err(|e| ExportError::Png(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_are_short_and_stable() {
        assert_eq!(n(1.0), "1");
        assert_eq!(n(1.256), "1.26");
        assert_eq!(n(-0.001), "0");
        assert_eq!(n(10.5), "10.5");
    }

    #[test]
    fn text_is_escaped() {
        let list = DisplayList {
            width: 10.0,
            height: 10.0,
            background: Color(255, 255, 255),
            prims: vec![Prim::Text {
                x: 0.0,
                y: 5.0,
                text: "R&D <phase>".into(),
                size: 9.0,
                weight: Weight::Normal,
                anchor: Anchor::Start,
                fill: Color(0, 0, 0),
            }],
        };
        assert!(to_svg(&list).contains("R&amp;D &lt;phase&gt;"));
    }
}
