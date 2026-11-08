//! CPM tests: hand-worked networks, the four link types and property tests (SPEC.md §12).

use alma_core::model::*;
use alma_core::time::{Instant, start_of};
use alma_core::{Duration, WorkUnits, parse_duration};
use alma_sched::{Driver, WorkCalendar, schedule};
use jiff::civil::Date;
use proptest::prelude::*;

const DAY: i64 = 480;

fn plan(tasks: Vec<Task>) -> Plan {
    Plan {
        project: Project {
            name: "test".into(),
            // A Monday.
            start: Date::constant(2027, 1, 4),
            calendar: None,
            deadline: None,
            currency: None,
            description: None,
        },
        settings: ScheduleSettings::default(),
        rules: RuleConfig::default(),
        calendars: Vec::new(),
        resources: Vec::new(),
        cadences: Vec::new(),
        tasks,
        baselines: Vec::new(),
    }
}

fn task(id: &str, days: i64, deps: &[&str]) -> Task {
    let mut t = Task::new(id, id);
    t.duration = Some(Duration::working(days * DAY));
    t.deps = deps.iter().map(|p| Dep::fs(*p)).collect();
    t
}

fn link(pred: &str, kind: DepType, lag: &str) -> Dep {
    Dep {
        kind,
        lag: parse_duration(lag, WorkUnits::default(), true).unwrap(),
        ..Dep::fs(pred)
    }
}

/// Working days from the project start to `t`, on the standard calendar.
fn days(p: &Plan, t: Instant) -> f64 {
    let cal = WorkCalendar::new(&Calendar::standard());
    cal.between(cal.next_start(start_of(p.project.start)), t) as f64 / DAY as f64
}

/// A textbook network worked out by hand in the "elapsed periods" convention (ES of the first
/// activity = 0, EF = ES + d), so working days from the project start match it directly.
///
/// ```text
/// A(3) ─┬─ B(2) ── D(2) ──┐
///       ├─ C(4) ── F(4) ──┼─ H(4)
///       └─ G(2) ──────────┘
/// E(6) ────────── F
/// ```
/// Critical path A→C→F→H, length 15. Floats: B 4, D 4, E 1, G 6.
#[test]
fn f_sch_06_07_08_09_textbook_network() {
    let p = plan(vec![
        task("a", 3, &[]),
        task("b", 2, &["a"]),
        task("c", 4, &["a"]),
        task("d", 2, &["b"]),
        task("e", 6, &[]),
        task("f", 4, &["c", "e"]),
        task("g", 2, &["a"]),
        task("h", 4, &["f", "d", "g"]),
    ]);
    let s = schedule(&p).unwrap();
    // id, ES, EF, LS, LF, TF, FF
    let expected = [
        ("a", 0, 3, 0, 3, 0, 0),
        ("b", 3, 5, 7, 9, 4, 0),
        ("c", 3, 7, 3, 7, 0, 0),
        ("d", 5, 7, 9, 11, 4, 4),
        ("e", 0, 6, 1, 7, 1, 1),
        ("f", 7, 11, 7, 11, 0, 0),
        ("g", 3, 5, 9, 11, 6, 6),
        ("h", 11, 15, 11, 15, 0, 0),
    ];
    for (i, (id, es, ef, ls, lf, tf, ff)) in expected.into_iter().enumerate() {
        let d = &s.tasks[i];
        assert_eq!(p.tasks[i].id, id);
        let got = (
            days(&p, d.es),
            days(&p, d.ef),
            days(&p, d.ls),
            days(&p, d.lf),
            d.total_float.unwrap() as f64 / DAY as f64,
            d.free_float.unwrap() as f64 / DAY as f64,
        );
        let want = (
            es as f64, ef as f64, ls as f64, lf as f64, tf as f64, ff as f64,
        );
        assert_eq!(got, want, "task {id}");
        assert_eq!(d.critical, tf == 0, "task {id}");
    }
    assert_eq!(days(&p, s.finish), 15.0);
}

#[test]
fn f_sch_06_link_types_with_lags() {
    let mut ss = task("ss", 3, &[]);
    ss.deps = vec![link("a", DepType::Ss, "2d")];
    let mut ff = task("ff", 3, &[]);
    ff.deps = vec![link("a", DepType::Ff, "1d")];
    let mut sf = task("sf", 3, &[]);
    sf.deps = vec![link("a", DepType::Sf, "4d")];
    let mut lead = task("lead", 2, &[]);
    lead.deps = vec![link("a", DepType::Fs, "-2d")];
    let p = plan(vec![task("a", 5, &[]), ss, ff, sf, lead]);
    let s = schedule(&p).unwrap();
    let es = |i: usize| days(&p, s.tasks[i].es);
    let ef = |i: usize| days(&p, s.tasks[i].ef);
    assert_eq!((es(1), ef(1)), (2.0, 5.0), "SS +2d");
    assert_eq!((es(2), ef(2)), (3.0, 6.0), "FF +1d");
    assert_eq!((es(3), ef(3)), (1.0, 4.0), "SF +4d");
    assert_eq!((es(4), ef(4)), (3.0, 5.0), "FS -2d (lead)");
    assert!(matches!(s.tasks[1].driver, Driver::Dep { pred: 0, .. }));
}

#[test]
fn f_sch_07_backward_pass_through_ss_and_ff() {
    // a(5) SS+2d → b(3); b FF+0 → c(1). Anchor = max EF.
    let mut b = task("b", 3, &[]);
    b.deps = vec![link("a", DepType::Ss, "2d")];
    let mut c = task("c", 1, &[]);
    c.deps = vec![link("b", DepType::Ff, "0d")];
    let p = plan(vec![task("a", 5, &[]), b, c]);
    let s = schedule(&p).unwrap();
    // a: 0–5, b: 2–5, c: 4–5. Everything ends at 5, so nothing has float.
    for d in &s.tasks {
        assert_eq!(d.total_float, Some(0));
    }
    assert_eq!(days(&p, s.tasks[2].es), 4.0);
}

#[test]
fn elapsed_lag_counts_weekends() {
    // a finishes Friday 18:00; +2ed lands Sunday 18:00, so b starts Monday 09:00.
    let mut b = task("b", 1, &[]);
    b.deps = vec![link("a", DepType::Fs, "2ed")];
    let mut c = task("c", 1, &[]);
    c.deps = vec![link("a", DepType::Fs, "3ed")];
    let p = plan(vec![task("a", 5, &[]), b, c]);
    let s = schedule(&p).unwrap();
    assert_eq!(days(&p, s.tasks[1].es), 5.0);
    assert_eq!(days(&p, s.tasks[2].es), 6.0);
}

#[test]
fn milestones_sit_at_the_predecessor_finish() {
    let mut m = Task::new("m", "done");
    m.kind = TaskKind::Milestone;
    m.deps = vec![Dep::fs("a")];
    let p = plan(vec![task("a", 5, &[]), m, task("b", 1, &["m"])]);
    let s = schedule(&p).unwrap();
    assert_eq!(s.tasks[1].es, s.tasks[0].ef);
    assert_eq!(s.tasks[1].ef, s.tasks[1].es);
    assert_eq!(days(&p, s.tasks[2].es), 5.0);
}

#[test]
fn summary_links_apply_to_leaves_and_roll_up() {
    let mut x = task("x", 2, &[]);
    x.parent = Some("phase".into());
    let mut y = task("y", 4, &[]);
    y.parent = Some("phase".into());
    let p = plan(vec![
        Task::new("phase", "Phase"),
        x,
        y,
        task("after", 1, &["phase"]),
    ]);
    let s = schedule(&p).unwrap();
    assert_eq!(days(&p, s.tasks[0].es), 0.0);
    assert_eq!(days(&p, s.tasks[0].ef), 4.0);
    assert_eq!(s.tasks[0].driver, Driver::Rollup);
    assert_eq!(days(&p, s.tasks[3].es), 4.0);
}

#[test]
fn constraints_and_deadline_create_negative_float() {
    let mut b = task("b", 2, &["a"]);
    b.deadline = Some(Date::constant(2027, 1, 6)); // Wednesday: needs to finish by day 3.
    let mut c = task("c", 1, &[]);
    c.constraint = Some(Constraint {
        kind: ConstraintKind::Snet,
        date: Some(Date::constant(2027, 1, 11)),
    });
    let p = plan(vec![task("a", 3, &[]), b, c]);
    let s = schedule(&p).unwrap();
    assert_eq!(s.tasks[1].total_float, Some(-2 * DAY));
    assert_eq!(s.tasks[0].total_float, Some(-2 * DAY));
    assert_eq!(days(&p, s.tasks[2].es), 5.0);
    assert_eq!(s.tasks[2].driver, Driver::Constraint(ConstraintKind::Snet));
}

#[test]
fn e_dep_01_cycle_reports_path() {
    let p = plan(vec![
        task("a", 1, &["c"]),
        task("b", 1, &["a"]),
        task("c", 1, &["b"]),
    ]);
    let err = schedule(&p).unwrap_err();
    assert_eq!(err[0].code, "E-DEP-01");
    assert_eq!(err[0].ids.len(), 4, "{:?}", err[0].ids);
    assert_eq!(err[0].ids.first(), err[0].ids.last());
}

#[test]
fn e_dep_02_link_inside_own_branch() {
    let mut child = task("child", 1, &["phase"]);
    child.parent = Some("phase".into());
    let p = plan(vec![Task::new("phase", "Phase"), child]);
    assert_eq!(schedule(&p).unwrap_err()[0].code, "E-DEP-02");
}

#[test]
fn f_sch_04_three_point_estimate_drives_duration() {
    let mut t = Task::new("t", "t");
    let d = |n| Duration::working(n * DAY);
    t.estimate = Some(Estimate {
        o: d(4),
        m: d(7),
        p: d(16),
        dist: None,
        basis: None,
        confidence: None,
    });
    let p = plan(vec![t]);
    let s = schedule(&p).unwrap();
    // (4 + 28 + 16) / 6 = 8 days.
    assert_eq!(s.tasks[0].duration, d(8));
}

#[test]
fn f_sch_01_effort_to_duration() {
    let mut t = Task::new("t", "t");
    t.sched_type = SchedType::FixedEffort;
    t.effort = Some(Duration::working(160 * 60));
    t.assign = vec![Assignment {
        resource: "dev".into(),
        units: 2.into(),
    }];
    let p = plan(vec![t]);
    // 160 h / (2 × 8 h) = 10 working days.
    assert_eq!(
        schedule(&p).unwrap().tasks[0].duration,
        Duration::working(10 * DAY)
    );
}

/// A random DAG: links only go from lower to higher index, so it is always acyclic.
fn arb_plan() -> impl Strategy<Value = Plan> {
    (2usize..25).prop_flat_map(|n| {
        let durations = proptest::collection::vec(0i64..8, n);
        let links = proptest::collection::vec((0usize..n, 0usize..n, 0u8..4, 0i64..3), 0..n * 2);
        (durations, links).prop_map(move |(durs, links)| {
            let mut tasks: Vec<Task> = durs
                .iter()
                .enumerate()
                .map(|(i, &d)| {
                    let mut t = task(&format!("t{i}"), d, &[]);
                    if d == 0 {
                        t.kind = TaskKind::Milestone;
                        t.duration = None;
                    }
                    t
                })
                .collect();
            for (a, b, kind, lag) in links {
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
            plan(tasks)
        })
    })
}

proptest! {
    #[test]
    fn cpm_invariants(p in arb_plan()) {
        let s = schedule(&p).unwrap();
        let cal = WorkCalendar::new(&Calendar::standard());
        for (i, d) in s.tasks.iter().enumerate() {
            let tf = d.total_float.unwrap();
            let ff = d.free_float.unwrap();
            prop_assert!(cal.between(d.es, d.ls) >= 0, "ES <= LS for {}", i);
            prop_assert!(cal.between(d.ef, d.lf) >= 0, "EF <= LF for {}", i);
            prop_assert!(tf >= 0, "TF >= 0 for {}", i);
            prop_assert!(ff >= 0 && ff <= tf, "0 <= FF <= TF for {}: {} {}", i, ff, tf);
            prop_assert!(d.es >= s.start);
            // Every link is satisfied, in working time.
            for dep in &p.tasks[i].deps {
                let j: usize = dep.pred[1..].parse().unwrap();
                let pd = &s.tasks[j];
                let (from, to) = match dep.kind {
                    DepType::Fs => (pd.ef, d.es),
                    DepType::Ss => (pd.es, d.es),
                    DepType::Ff => (pd.ef, d.ef),
                    DepType::Sf => (pd.es, d.ef),
                };
                prop_assert!(cal.between(cal.add(from, dep.lag.minutes), to) >= 0,
                    "link t{} -> t{} {:?}", j, i, dep.kind);
            }
        }
        prop_assert!(s.tasks.iter().any(|d| d.critical), "some task is critical");
    }

    #[test]
    fn schedule_is_invariant_to_task_order(p in arb_plan(), seed in any::<u64>()) {
        let s1 = schedule(&p).unwrap();
        let mut q = p.clone();
        // Deterministic shuffle from the seed.
        let mut state = seed | 1;
        for i in (1..q.tasks.len()).rev() {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            q.tasks.swap(i, (state % (i as u64 + 1)) as usize);
        }
        let s2 = schedule(&q).unwrap();
        for (i, t) in q.tasks.iter().enumerate() {
            let k = p.tasks.iter().position(|x| x.id == t.id).unwrap();
            let (a, b) = (&s1.tasks[k], &s2.tasks[i]);
            prop_assert_eq!((a.es, a.ef, a.ls, a.lf, a.total_float, a.free_float),
                (b.es, b.ef, b.ls, b.lf, b.total_float, b.free_float), "task {}", t.id);
        }
    }
}
