//! Export tests: deterministic SVG, print-grade font sizes, PNG, depth and range, CSV.

use alma_ops::export::{RenderRequest, csv, png, svg};

fn fixture() -> alma_ops::Loaded {
    alma_ops::load(include_str!("../../../examples/hybrid-terminal.alma.toml"))
}

fn render(req: &RenderRequest) -> String {
    let l = fixture();
    svg(l.plan.as_ref().unwrap(), l.schedule.as_ref().unwrap(), req).unwrap()
}

#[test]
fn svg_is_deterministic_and_complete() {
    let req = RenderRequest::default();
    let a = render(&req);
    assert_eq!(a, render(&req));
    assert!(a.starts_with("<svg "));
    assert!(a.trim_end().ends_with("</svg>"));
    for name in [
        "Discovery and solution design",
        "Stage 1 closed",
        "s01",
        "Jan 2027",
    ] {
        assert!(a.contains(name), "missing {name}");
    }
}

#[test]
fn no_text_below_9pt() {
    for font in [None, Some(6.0), Some(12.0)] {
        let s = render(&RenderRequest {
            font_size: font,
            ..Default::default()
        });
        for part in s.split("font-size=\"").skip(1) {
            let size: f64 = part[..part.find('"').unwrap()].parse().unwrap();
            assert!(size >= 9.0, "font-size {size} with base {font:?}");
        }
    }
}

#[test]
fn depth_and_range_cut_the_chart() {
    let full = render(&RenderRequest::default());
    let top = render(&RenderRequest {
        depth: Some(1),
        ..Default::default()
    });
    assert!(full.contains(">Environments<"));
    assert!(!top.contains(">Environments<"));
    assert!(top.contains(">Platform<"));
    let narrow = render(&RenderRequest {
        range: Some("2027-02..2027-03".into()),
        // Same scale as the full chart; a two-month range alone would pick the day scale.
        scale: Some("week".into()),
        ..Default::default()
    });
    let width = |s: &str| -> f64 {
        let w = s.split("width=\"").nth(1).unwrap();
        w[..w.find("pt").unwrap()].parse().unwrap()
    };
    assert!(width(&narrow) < width(&full));
    assert!(narrow.contains("Feb 2027") && !narrow.contains("Jul 2027"));
}

#[test]
fn png_is_a_png() {
    let l = fixture();
    let bytes = png(
        l.plan.as_ref().unwrap(),
        l.schedule.as_ref().unwrap(),
        &RenderRequest::default(),
    )
    .unwrap();
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
}

#[test]
fn csv_has_one_row_per_task_and_quotes() {
    let l = fixture();
    let text = csv(l.plan.as_ref().unwrap(), l.schedule.as_ref().unwrap());
    assert_eq!(text.lines().count(), 26);
    assert!(text.starts_with("wbs,id,name,kind,start,finish"));
    let mut plan = l.plan.clone().unwrap();
    plan.tasks[0].name = "Management, \"PMO\"".into();
    let quoted = csv(&plan, l.schedule.as_ref().unwrap());
    assert!(quoted.contains("\"Management, \"\"PMO\"\"\""));
}
