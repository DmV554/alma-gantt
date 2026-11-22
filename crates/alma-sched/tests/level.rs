//! Resource usage, over-allocation, leveling and smoothing (SPEC.md §7.4, knowledge notes
//! resources §1.5–§1.7, F-COST-03/F-COST-04, L-RES-01/L-RES-02, ADR 0011).

use alma_core::diag::Severity;
use alma_core::model::*;
use alma_core::time::{Instant, start_of};
use alma_core::{Duration, WorkUnits, parse_duration};
use alma_sched::resources::{Period, histogram, overloads};
use alma_sched::{Driver, WorkCalendar, lint, schedule};
use jiff::civil::Date;
use proptest::prelude::*;
use rust_decimal::Decimal;

const DAY: i64 = 480;

fn plan(tasks: Vec<Task>, resources: &[(&str, i64)]) -> Plan {
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
        resources: resources
            .iter()
            .map(|(id, max)| Resource {
                id: (*id).into(),
                name: (*id).into(),
                kind: ResourceKind::Work,
                rate: None,
                max_units: Decimal::from(*max),
                calendar: None,
            })
            .collect(),
        cadences: Vec::new(),
        tasks,
        baselines: Vec::new(),
        reserves: Vec::new(),
        risks: Vec::new(),
        funding: Vec::new(),
    }
}

fn task(id: &str, days: i64, deps: &[&str], assign: &[(&str, i64)]) -> Task {
    let mut t = Task::new(id, id);
    t.duration = Some(Duration::working(days * DAY));
    t.deps = deps.iter().map(|p| Dep::fs(*p)).collect();
    t.assign = assign
        .iter()
        .map(|(r, u)| Assignment {
            resource: (*r).into(),
            units: Decimal::from(*u),
        })
        .collect();
    t
}

/// Working days from the project start to `t`, on the standard calendar.
fn days(p: &Plan, t: Instant) -> f64 {
    let cal = WorkCalendar::new(&Calendar::standard());
    cal.between(cal.next_start(start_of(p.project.start)), t) as f64 / DAY as f64
}

fn leveled(mut p: Plan, mode: Leveling) -> Plan {
    p.settings.leveling = mode;
    p
}

fn codes(p: &Plan) -> Vec<(&'static str, Severity)> {
    let s = schedule(p).unwrap();
    lint(p, Some(&s))
        .into_iter()
        .filter(|d| matches!(d.code, "L-RES-01" | "L-RES-02"))
        .map(|d| (d.code, d.severity))
        .collect()
}

/// Two parallel tasks need the only developer.
///
/// ```text
/// a(5, dev) ──┐
///             ├── end
/// b(3, dev) ──┘      (b has 2 days of float)
/// ```
/// Unleveled: both start on day 0 and `dev` is booked 2 of 1. Leveling places `a` first (least
/// float), then `b` waits until day 5: the finish moves from 5 to 8.
#[test]
fn l_res_01_parallel_tasks_are_leveled_by_least_float() {
    let p = plan(
        vec![
            task("a", 5, &[], &[("dev", 1)]),
            task("b", 3, &[], &[("dev", 1)]),
            task("end", 0, &["a", "b"], &[]),
        ],
        &[("dev", 1)],
    );
    let s = schedule(&p).unwrap();
    assert_eq!(days(&p, s.finish), 5.0);
    let o = overloads(&p, &s);
    assert_eq!(o.len(), 1);
    assert_eq!(
        (o[0].peak, o[0].tasks.clone()),
        (Decimal::from(2), vec![0, 1])
    );
    assert_eq!(codes(&p), vec![("L-RES-01", Severity::Warn)]);

    let q = leveled(p, Leveling::Leveling);
    let s = schedule(&q).unwrap();
    assert_eq!(days(&q, s.tasks[0].es), 0.0);
    assert_eq!(days(&q, s.tasks[1].es), 5.0);
    assert_eq!(days(&q, s.finish), 8.0);
    assert_eq!(s.tasks[1].driver, Driver::Leveling { resource: 0, by: 0 });
    assert!(overloads(&q, &s).is_empty());
    assert!(codes(&q).is_empty());
    let r = s.leveling.as_ref().unwrap();
    assert_eq!(days(&q, r.unleveled_finish), 5.0);
    assert_eq!(r.delays.len(), 1);
    assert_eq!((r.delays[0].task, r.delays[0].by), (1, 0));
    // The resource link makes `a` critical again: delaying it now delays `b` and the finish.
    assert!(s.tasks[0].critical && s.tasks[1].critical);
}

/// Smoothing delays a task only within its free float, so the finish never moves.
///
/// ```text
/// a(3, dev) ── end
/// b(n, dev) ── end
/// x(5) ─────── end      (x sets the finish at day 5)
/// ```
/// With n = 1, `b` has 4 days of free float: it waits for `a` (day 3) and the finish stays at 5.
/// With n = 3, `b` has 2 days of float but would have to wait 3: smoothing leaves it at day 0
/// and reports L-RES-02.
#[test]
fn l_res_02_smoothing_stays_within_free_float() {
    let mk = |b_days| {
        leveled(
            plan(
                vec![
                    task("a", 3, &[], &[("dev", 1)]),
                    task("b", b_days, &[], &[("dev", 1)]),
                    task("x", 5, &[], &[]),
                    task("end", 0, &["a", "b", "x"], &[]),
                ],
                &[("dev", 1)],
            ),
            Leveling::Smoothing,
        )
    };
    let p = mk(1);
    let s = schedule(&p).unwrap();
    assert_eq!(days(&p, s.tasks[1].es), 3.0);
    assert_eq!(days(&p, s.finish), 5.0);
    assert!(codes(&p).is_empty());

    let p = mk(3);
    let s = schedule(&p).unwrap();
    assert_eq!(days(&p, s.tasks[1].es), 0.0);
    assert_eq!(days(&p, s.finish), 5.0);
    assert_eq!(s.leveling.as_ref().unwrap().unresolved, vec![1]);
    assert_eq!(codes(&p), vec![("L-RES-02", Severity::Warn)]);
}

#[test]
fn l_res_01_is_an_error_when_leveling_cannot_fix_it() {
    // One assignment above capacity: no delay helps.
    let p = leveled(
        plan(vec![task("a", 5, &[], &[("dev", 3)])], &[("dev", 2)]),
        Leveling::Leveling,
    );
    let s = schedule(&p).unwrap();
    assert_eq!(days(&p, s.tasks[0].es), 0.0);
    assert_eq!(s.leveling.as_ref().unwrap().unresolved, vec![0]);
    assert_eq!(codes(&p), vec![("L-RES-01", Severity::Error)]);
}

/// An MSO task keeps its date; it is placed by priority like any other, and the movable task
/// works around it. `a` (2 days, float 1) and `fixed` (3 days, must start day 0) both need the
/// only developer: `fixed` has less float, so it goes first and `a` waits until day 3.
#[test]
fn fixed_dates_are_never_moved_but_still_book_their_resources() {
    let mut fixed = task("fixed", 3, &[], &[("dev", 1)]);
    fixed.constraint = Some(Constraint {
        kind: ConstraintKind::Mso,
        date: Some(Date::constant(2027, 1, 4)),
    });
    let p = leveled(
        plan(vec![task("a", 2, &[], &[("dev", 1)]), fixed], &[("dev", 1)]),
        Leveling::Leveling,
    );
    let s = schedule(&p).unwrap();
    assert_eq!(days(&p, s.tasks[1].es), 0.0, "MSO task stays");
    assert_eq!(days(&p, s.tasks[0].es), 3.0);
    assert!(overloads(&p, &s).is_empty());
}

/// A lag is waiting time: it demands no resource (knowledge notes, resources §1.7).
#[test]
fn lag_generates_no_resource_demand() {
    let mut b = task("b", 2, &[], &[("dev", 1)]);
    b.deps = vec![Dep {
        lag: parse_duration("5d", WorkUnits::default(), true).unwrap(),
        ..Dep::fs("a")
    }];
    let p = plan(vec![task("a", 2, &[], &[("dev", 1)]), b], &[("dev", 1)]);
    let s = schedule(&p).unwrap();
    let bars = histogram(&p, &s, 0, Period::Day);
    let demand: Vec<i64> = bars
        .iter()
        .map(|b| (b.demand / Decimal::from(60)).try_into().unwrap())
        .collect();
    // Mon–Tue a (8 h each), then a 5-working-day gap (Wed–Tue incl. a weekend), then b.
    let working: Vec<i64> = demand.iter().copied().filter(|&h| h > 0).collect();
    assert_eq!(working, vec![8, 8, 8, 8]);
    let first_b = bars.iter().position(|x| x.tasks == vec![1]).unwrap();
    let gap: i64 = demand[2..first_b].iter().sum();
    assert_eq!(gap, 0, "no demand during the lag");
}

#[test]
fn f_cost_03_04_weekly_histogram() {
    // dev max 1; a and b overlap for a whole week: demand 80 h vs capacity 40 h.
    let p = plan(
        vec![
            task("a", 5, &[], &[("dev", 1)]),
            task("b", 5, &[], &[("dev", 1)]),
        ],
        &[("dev", 1)],
    );
    let s = schedule(&p).unwrap();
    let bars = histogram(&p, &s, 0, Period::Week);
    assert_eq!(bars.len(), 1);
    assert_eq!(bars[0].start, Date::constant(2027, 1, 4));
    assert_eq!(bars[0].demand, Decimal::from(80 * 60));
    assert_eq!(bars[0].capacity, Decimal::from(40 * 60));
}

#[test]
fn material_and_cost_resources_never_over_allocate() {
    let mut p = plan(
        vec![
            task("a", 5, &[], &[("cable", 10)]),
            task("b", 5, &[], &[("cable", 10)]),
        ],
        &[("cable", 1)],
    );
    p.resources[0].kind = ResourceKind::Material;
    let s = schedule(&p).unwrap();
    assert!(overloads(&p, &s).is_empty());
    let q = leveled(p, Leveling::Leveling);
    assert_eq!(schedule(&q).unwrap().leveling, None);
}

/// Random networks with random assignments to two resources.
fn arb_plan() -> impl Strategy<Value = Plan> {
    (2usize..18).prop_flat_map(|n| {
        let durations = proptest::collection::vec(1i64..6, n);
        let links = proptest::collection::vec((0usize..n, 0usize..n), 0..n);
        let assigns = proptest::collection::vec((0usize..3, 1i64..3), n);
        (durations, links, assigns).prop_map(move |(durs, links, assigns)| {
            let names = ["dev", "qa"];
            let mut tasks: Vec<Task> = durs
                .iter()
                .zip(&assigns)
                .enumerate()
                .map(|(i, (&d, &(r, u)))| {
                    let a: Vec<(&str, i64)> = if r < 2 {
                        vec![(names[r], u)]
                    } else {
                        Vec::new()
                    };
                    task(&format!("t{i}"), d, &[], &a)
                })
                .collect();
            for (a, b) in links {
                let (a, b) = (a.min(b), a.max(b));
                if a != b && !tasks[b].deps.iter().any(|d| d.pred == format!("t{a}")) {
                    tasks[b].deps.push(Dep::fs(format!("t{a}")));
                }
            }
            plan(tasks, &[("dev", 2), ("qa", 2)])
        })
    })
}

proptest! {
    #[test]
    fn leveling_resolves_every_conflict_and_keeps_logic(p in arb_plan()) {
        let before = schedule(&p).unwrap();
        let q = leveled(p.clone(), Leveling::Leveling);
        let s = schedule(&q).unwrap();
        let cal = WorkCalendar::new(&Calendar::standard());
        // No over-allocation is left: every assignment fits its resource on its own.
        prop_assert!(overloads(&q, &s).is_empty(), "{:?}", overloads(&q, &s));
        for (i, d) in s.tasks.iter().enumerate() {
            prop_assert!(d.es >= before.tasks[i].es, "leveling never pulls a task earlier");
            prop_assert!(d.total_float.unwrap() >= 0, "TF >= 0 for t{}", i);
            for dep in &q.tasks[i].deps {
                let j: usize = dep.pred[1..].parse().unwrap();
                prop_assert!(cal.between(s.tasks[j].ef, d.es) >= 0, "t{} -> t{}", j, i);
            }
        }
        prop_assert!(s.finish >= before.finish);
        prop_assert!(s.tasks.iter().any(|d| d.critical));
    }

    #[test]
    fn smoothing_never_moves_the_finish(p in arb_plan()) {
        let before = schedule(&p).unwrap();
        let q = leveled(p, Leveling::Smoothing);
        let s = schedule(&q).unwrap();
        prop_assert_eq!(s.finish, before.finish);
        for (i, d) in s.tasks.iter().enumerate() {
            prop_assert!(d.es >= before.tasks[i].es);
        }
        // Whatever is left over-allocated is reported as L-RES-02, never silently.
        let left = !overloads(&q, &s).is_empty();
        let reported = lint(&q, Some(&s)).iter().any(|d| d.code == "L-RES-02");
        prop_assert_eq!(left, reported);
    }

    #[test]
    fn leveling_is_deterministic(p in arb_plan()) {
        let q = leveled(p, Leveling::Leveling);
        prop_assert_eq!(schedule(&q).unwrap(), schedule(&q).unwrap());
    }
}

/// Leveling is not in SPEC §12's 50 ms budget (that is for logic-only scheduling), but it must
/// stay interactive on big plans: 5,000 tasks over 5 resources in well under a second in release.
#[test]
fn leveling_5000_tasks_stays_interactive() {
    let names = ["a", "b", "c", "d", "e"];
    let mut tasks = Vec::with_capacity(5000);
    for i in 0..5000 {
        let r = names[i % 5];
        let deps: Vec<String> = if i >= 25 {
            vec![format!("t{}", i - 25)]
        } else {
            Vec::new()
        };
        let deps: Vec<&str> = deps.iter().map(String::as_str).collect();
        tasks.push(task(&format!("t{i}"), (i % 4 + 1) as i64, &deps, &[(r, 1)]));
    }
    let p = leveled(
        plan(tasks, &[("a", 3), ("b", 3), ("c", 3), ("d", 3), ("e", 3)]),
        Leveling::Leveling,
    );
    let start = std::time::Instant::now();
    let s = schedule(&p).unwrap();
    let elapsed = start.elapsed();
    assert!(overloads(&p, &s).is_empty());
    eprintln!("leveled 5000 tasks in {elapsed:?}");
    if !cfg!(debug_assertions) {
        assert!(elapsed.as_millis() < 1000, "took {elapsed:?}");
    }
}

/// A resource with its own calendar narrows the working time of the tasks it works on
/// (knowledge notes, resources §1.3, ADR 0017). Worked by someone available Monday to
/// Wednesday, a 3-day task fits in one week and a 4-day task needs the next Monday, unless
/// `resource_calendars = "ignore"`.
#[test]
fn l_res_03_resource_calendars_narrow_tasks() {
    let mut p = plan(vec![task("a", 3, &[], &[("pt", 1)])], &[("pt", 1)]);
    let mut part = Calendar::standard();
    part.id = "mon-wed".into();
    part.workdays = [true, true, true, false, false, false, false];
    p.calendars.push(part);
    p.resources[0].calendar = Some("mon-wed".into());
    let s = schedule(&p).unwrap();
    // 3 days of work on Mon–Wed only: Monday 4 to Wednesday 6 January.
    assert_eq!(days(&p, s.tasks[0].ef), 3.0);
    let mut q = p.clone();
    q.tasks[0].duration = Some(Duration::working(4 * DAY));
    let s = schedule(&q).unwrap();
    // The fourth day is the next Monday: 7 standard working days after the start.
    assert_eq!(days(&q, s.tasks[0].ef), 6.0);
    q.settings.resource_calendars = ResourceCalendars::Ignore;
    assert_eq!(days(&q, schedule(&q).unwrap().tasks[0].ef), 4.0);

    // No shared time: reported, and the task keeps its own calendar.
    let mut r = p.clone();
    r.calendars[0].workdays = [false, false, false, false, false, true, true];
    let s = schedule(&r).unwrap();
    assert_eq!(days(&r, s.tasks[0].ef), 3.0);
    let codes: Vec<_> = lint(&r, Some(&s))
        .into_iter()
        .filter(|d| d.code == "L-RES-03")
        .map(|d| d.severity)
        .collect();
    assert_eq!(codes, vec![Severity::Error]);
}
