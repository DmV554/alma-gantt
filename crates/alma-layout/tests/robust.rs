//! Property tests: the layout never panics and always yields finite, print-grade geometry.

use alma_core::Duration;
use alma_core::model::*;
use alma_layout::{MIN_FONT, Options, Prim, Scale, Theme, gantt};
use jiff::civil::Date;
use proptest::prelude::*;

const DAY: i64 = 480;

#[derive(Debug, Clone)]
struct Case {
    plan: Plan,
    opts: Options,
}

fn arb_case() -> impl Strategy<Value = Case> {
    let tasks = proptest::collection::vec(
        (
            0i64..30,
            0u8..6,
            any::<bool>(),
            proptest::option::of(0i64..400),
        ),
        1..30,
    );
    let range = proptest::option::of((0i64..300, 0i64..300));
    (
        tasks,
        proptest::collection::vec((0usize..30, 0usize..30, 0u8..4, -3i64..5), 0..40),
        range,
        proptest::option::of(0u8..3),
        proptest::option::of(0usize..3),
        any::<bool>(),
        proptest::option::of((0i64..200, 1i64..15, 1u32..30)),
        6.0f64..14.0,
    )
        .prop_map(|(specs, links, range, scale, depth, dark, cadence, font)| {
            let start = Date::constant(2027, 1, 4);
            let n = specs.len();
            let mut tasks: Vec<Task> = Vec::new();
            for (i, (days, shape, milestone, deadline)) in specs.into_iter().enumerate() {
                let mut t = Task::new(format!("t{i}"), format!("Task number {i} with a long name"));
                // Shapes: 0-1 top level, 2-5 child of an earlier summary (i/3).
                if shape >= 2 && i > 0 {
                    t.parent = Some(format!("t{}", (i - 1) / 3));
                }
                if milestone {
                    t.kind = TaskKind::Milestone;
                } else {
                    t.duration = Some(Duration::working(days * DAY));
                }
                t.deadline =
                    deadline.map(|d| start.checked_add(jiff::Span::new().days(d)).unwrap());
                tasks.push(t);
            }
            // Only leaves keep a kind; parents become summaries.
            let parents: Vec<String> = tasks.iter().filter_map(|t| t.parent.clone()).collect();
            for t in &mut tasks {
                if parents.contains(&t.id) {
                    t.kind = TaskKind::Task;
                    t.duration = None;
                }
            }
            for (a, b, kind, lag) in links {
                let (a, b) = (a % n, b % n);
                let (a, b) = (a.min(b), a.max(b));
                if a == b {
                    continue;
                }
                let kind = [DepType::Fs, DepType::Ss, DepType::Ff, DepType::Sf][kind as usize];
                tasks[b].deps.push(Dep {
                    kind,
                    lag: Duration::working(lag * DAY),
                    ..Dep::fs(format!("t{a}"))
                });
            }
            let cadences = cadence
                .map(|(off, len, count)| {
                    vec![Cadence {
                        id: "sprints".into(),
                        name: "Sprint".into(),
                        prefix: "s".into(),
                        start: start.checked_add(jiff::Span::new().days(off)).unwrap(),
                        length: Duration::working(len * DAY),
                        count,
                        calendar: None,
                    }]
                })
                .unwrap_or_default();
            let plan = Plan {
                project: Project {
                    name: "Random & <plan>".into(),
                    start,
                    calendar: None,
                    deadline: None,
                    currency: None,
                    description: None,
                },
                settings: ScheduleSettings::default(),
                rules: RuleConfig::default(),
                calendars: Vec::new(),
                resources: Vec::new(),
                cadences,
                tasks,
                baselines: Vec::new(),
                reserves: Vec::new(),
            };
            let day = |d: i64| start.checked_add(jiff::Span::new().days(d)).unwrap();
            let opts = Options {
                range: range.map(|(a, b)| (day(a.min(b)), day(a.max(b)))),
                scale: scale.map(|s| [Scale::Day, Scale::Week, Scale::Month][s as usize]),
                max_depth: depth,
                font_size: font,
                arrows: true,
                theme: if dark { Theme::DARK } else { Theme::LIGHT },
            };
            Case { plan, opts }
        })
}

fn finite(v: f64) -> bool {
    v.is_finite()
}

proptest! {
    #[test]
    fn layout_is_total_and_print_grade(case in arb_case()) {
        // Plans with links into their own branch are rejected by the engine; skip them.
        let Ok(schedule) = alma_sched::schedule(&case.plan) else { return Ok(()); };
        let list = gantt(&case.plan, &schedule, &case.opts);
        prop_assert!(finite(list.width) && list.width > 0.0);
        prop_assert!(finite(list.height) && list.height > 0.0);
        let mut depth = 0i32;
        for p in &list.prims {
            match p {
                Prim::Rect { x, y, w, h, .. } => {
                    prop_assert!(finite(*x) && finite(*y) && finite(*w) && finite(*h));
                    prop_assert!(*w >= 0.0 && *h >= 0.0);
                }
                Prim::Line { x1, y1, x2, y2, width, .. } => {
                    prop_assert!([x1, y1, x2, y2, width].iter().all(|v| finite(**v)));
                }
                Prim::Poly { points, .. } => {
                    prop_assert!(points.iter().all(|(x, y)| finite(*x) && finite(*y)));
                }
                Prim::Text { x, y, size, .. } => {
                    prop_assert!(finite(*x) && finite(*y));
                    prop_assert!(*size >= MIN_FONT, "font {} below 9 pt", size);
                }
                Prim::ClipStart { .. } => depth += 1,
                Prim::ClipEnd => depth -= 1,
            }
            prop_assert!(depth >= 0);
        }
        prop_assert_eq!(depth, 0, "clips are balanced");
        // Deterministic.
        prop_assert_eq!(&gantt(&case.plan, &schedule, &case.opts), &list);
    }
}
