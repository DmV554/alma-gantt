//! Scheduling with progress at a status date (ADR 0018; knowledge notes, scheduling §4.16).
//!
//! Every case is worked out by hand on the standard calendar (Mon–Fri, 09:00–13:00 and
//! 14:00–18:00, 8 h a day). The project starts on Monday 2027-01-04.

use alma_core::Duration;
use alma_core::model::*;
use alma_core::time::format_instant;
use alma_sched::cpm::Progress;
use alma_sched::{Driver, lint, schedule};
use jiff::civil::Date;
use proptest::prelude::*;
use rust_decimal::Decimal;

const DAY: i64 = 480;

fn date(m: i8, d: i8) -> Date {
    Date::constant(2027, m, d)
}

fn plan(tasks: Vec<Task>) -> Plan {
    Plan {
        project: Project {
            name: "test".into(),
            start: date(1, 4),
            calendar: None,
            deadline: None,
            currency: None,
            description: None,
            status_date: None,
        },
        settings: ScheduleSettings::default(),
        rules: RuleConfig::default(),
        calendars: Vec::new(),
        resources: Vec::new(),
        cadences: Vec::new(),
        tasks,
        baselines: Vec::new(),
        reserves: Vec::new(),
        risks: Vec::new(),
        funding: Vec::new(),
    }
}

fn task(id: &str, days: i64, deps: &[&str]) -> Task {
    let mut t = Task::new(id, id);
    t.duration = Some(Duration::working(days * DAY));
    t.deps = deps.iter().map(|p| Dep::fs(*p)).collect();
    t
}

fn started(mut t: Task, start: Date) -> Task {
    t.actual_start = Some(start);
    t
}

fn at(s: &alma_sched::Schedule, i: usize) -> (String, String) {
    (format_instant(s.tasks[i].es), format_instant(s.tasks[i].ef))
}

fn pair(a: &str, b: &str) -> (String, String) {
    (a.to_string(), b.to_string())
}

/// `a` (5d) started on Monday; the status date is Wednesday. Three days are done, so two are
/// left by default: they run Thursday and Friday, and `b` starts the next Monday, as planned.
#[test]
fn in_progress_task_keeps_its_start_and_runs_the_rest_after_the_status_date() {
    let mut p = plan(vec![
        started(task("a", 5, &[]), date(1, 4)),
        task("b", 5, &["a"]),
    ]);
    p.project.status_date = Some(date(1, 6));
    let s = schedule(&p).unwrap();
    assert_eq!(at(&s, 0), pair("2027-01-04T09:00", "2027-01-08T18:00"));
    assert_eq!(s.tasks[0].driver, Driver::Actual);
    assert!(matches!(
        s.tasks[0].progress,
        Progress::InProgress { remaining, .. } if remaining == 2 * DAY
    ));
    assert_eq!(at(&s, 1), pair("2027-01-11T09:00", "2027-01-15T18:00"));
}

/// The same task with four days left: the rest runs Thursday to Tuesday, so `b` slips two days.
#[test]
fn remaining_work_overrides_the_planned_duration() {
    let mut a = started(task("a", 5, &[]), date(1, 4));
    a.remaining = Some(Duration::working(4 * DAY));
    let mut p = plan(vec![a, task("b", 5, &["a"])]);
    p.project.status_date = Some(date(1, 6));
    let s = schedule(&p).unwrap();
    assert_eq!(at(&s, 0), pair("2027-01-04T09:00", "2027-01-12T18:00"));
    assert_eq!(at(&s, 1), pair("2027-01-13T09:00", "2027-01-19T18:00"));
}

/// `a` finished early, on Tuesday. Logic lets `b` start Wednesday, but nothing that has not
/// started can start before the data date: Thursday, the day after the status date.
#[test]
fn work_not_started_waits_for_the_status_date() {
    let mut a = started(task("a", 5, &[]), date(1, 4));
    a.actual_finish = Some(date(1, 5));
    let mut p = plan(vec![a, task("b", 5, &["a"])]);
    p.project.status_date = Some(date(1, 6));
    let s = schedule(&p).unwrap();
    assert_eq!(at(&s, 0), pair("2027-01-04T09:00", "2027-01-05T18:00"));
    assert_eq!(s.tasks[0].progress, Progress::Complete);
    assert_eq!(s.tasks[0].total_float, None, "done work has no float");
    assert!(!s.tasks[0].critical);
    assert_eq!(at(&s, 1), pair("2027-01-07T09:00", "2027-01-13T18:00"));
    assert_eq!(s.tasks[1].driver, Driver::StatusDate);
}

/// `b` started on Tuesday although its predecessor `a` is still running (out of sequence).
/// `a` has two days left (Thursday, Friday). `b` has 5 − 2 = 3 days left.
/// - Retained logic: the rest of `b` waits for `a`: Monday to Wednesday.
/// - Progress override: the rest of `b` runs from the data date: Thursday to Monday.
#[test]
fn out_of_sequence_progress_follows_progress_logic() {
    let tasks = vec![
        started(task("a", 5, &[]), date(1, 4)),
        started(task("b", 5, &["a"]), date(1, 5)),
    ];
    let mut p = plan(tasks);
    p.project.status_date = Some(date(1, 6));
    let s = schedule(&p).unwrap();
    assert_eq!(at(&s, 1), pair("2027-01-05T09:00", "2027-01-13T18:00"));
    assert!(matches!(
        s.tasks[1].progress,
        Progress::InProgress { resume, remaining, .. }
            if format_instant(resume) == "2027-01-11T09:00" && remaining == 3 * DAY
    ));
    p.settings.progress_logic = ProgressLogic::ProgressOverride;
    let s = schedule(&p).unwrap();
    assert_eq!(at(&s, 1), pair("2027-01-05T09:00", "2027-01-11T18:00"));
}

/// A milestone marked as reached sits at the first working minute of that day, even while the
/// work before it is still running.
#[test]
fn reached_milestone_is_fixed_on_its_day() {
    let mut m = task("m", 0, &["a"]);
    m.kind = TaskKind::Milestone;
    m.actual_finish = Some(date(1, 5));
    let mut p = plan(vec![started(task("a", 5, &[]), date(1, 4)), m]);
    p.project.status_date = Some(date(1, 6));
    let s = schedule(&p).unwrap();
    assert_eq!(at(&s, 1), pair("2027-01-05T09:00", "2027-01-05T09:00"));
    assert_eq!(s.tasks[1].progress, Progress::Complete);
}

/// Float of a task in progress is measured on its finish only.
///
/// `x` (20d, not started) runs from the data date, Thursday 01-07, to Wednesday 02-03.
/// `a` (in progress, 2d left) finishes Friday 01-08; `b` (5d) runs 01-11 to 01-15.
/// Late dates: `b` must finish by 02-03, so it may start 01-28; `a` must finish by 01-27.
/// Both have 13 days of total float; the late start of `a` is that of its remaining work.
#[test]
fn f_sch_08_float_of_a_task_in_progress() {
    let mut p = plan(vec![
        started(task("a", 5, &[]), date(1, 4)),
        task("b", 5, &["a"]),
        task("x", 20, &[]),
    ]);
    p.project.status_date = Some(date(1, 6));
    let s = schedule(&p).unwrap();
    assert_eq!(format_instant(s.tasks[2].ef), "2027-02-03T18:00");
    assert_eq!(s.tasks[0].total_float, Some(13 * DAY));
    assert_eq!(s.tasks[1].total_float, Some(13 * DAY));
    assert_eq!(format_instant(s.tasks[0].lf), "2027-01-27T18:00");
    assert_eq!(format_instant(s.tasks[0].ls), "2027-01-26T09:00");
    assert!(s.tasks[2].critical);
    assert!(!s.tasks[0].critical);
}

/// Leveling never moves started work, and places it first: `c` comes first in the file and has
/// less float, but `a` already holds the only developer, so `c` waits until `a` is done.
#[test]
fn leveling_keeps_started_work_and_books_it_first() {
    let assign = |mut t: Task| {
        t.assign = vec![Assignment {
            resource: "dev".into(),
            units: Decimal::ONE,
        }];
        t
    };
    let mut p = plan(vec![
        assign(task("c", 10, &[])),
        assign(started(task("a", 5, &[]), date(1, 4))),
    ]);
    p.resources.push(Resource {
        id: "dev".into(),
        name: "dev".into(),
        kind: ResourceKind::Work,
        rate: None,
        max_units: Decimal::ONE,
        calendar: None,
    });
    p.project.status_date = Some(date(1, 6));
    p.settings.leveling = Leveling::Leveling;
    let s = schedule(&p).unwrap();
    assert_eq!(at(&s, 1), pair("2027-01-04T09:00", "2027-01-08T18:00"));
    assert_eq!(format_instant(s.tasks[0].es), "2027-01-11T09:00");
    assert!(matches!(s.tasks[0].driver, Driver::Leveling { by: 1, .. }));
    assert!(alma_sched::resources::overloads(&p, &s).is_empty());
}

/// A task waiting out of sequence does no work while it waits: its cost covers the two days
/// done and the three left, not the idle days between (F-COST-07 over the work spans).
#[test]
fn f_cost_07_no_cost_while_waiting() {
    let mut b = started(task("b", 5, &["a"]), date(1, 5));
    b.assign = vec![Assignment {
        resource: "dev".into(),
        units: Decimal::ONE,
    }];
    let mut p = plan(vec![started(task("a", 5, &[]), date(1, 4)), b]);
    p.resources.push(Resource {
        id: "dev".into(),
        name: "dev".into(),
        kind: ResourceKind::Work,
        rate: Some(Rate {
            amount: Decimal::ONE,
            per: RatePer::Hour,
        }),
        max_units: Decimal::ONE,
        calendar: None,
    });
    p.project.status_date = Some(date(1, 6));
    let s = schedule(&p).unwrap();
    assert_eq!(s.tasks[1].work_spans().len(), 2);
    let c = alma_sched::cost::costs(&p, &s);
    assert_eq!(c.task_total[1], Decimal::from(40));
    // The weekly curve puts 16 h in the first week (Tue, Wed) and 24 h in the second.
    let usage = alma_sched::resources::bookings(&p, &s);
    let hours: Vec<i64> = usage[0]
        .iter()
        .map(|b| alma_sched::WorkCalendar::new(&Calendar::standard()).between(b.start, b.end) / 60)
        .collect();
    assert_eq!(hours, [16, 24]);
}

fn codes(p: &Plan) -> Vec<&'static str> {
    let s = schedule(p).ok();
    lint(p, s.as_ref())
        .into_iter()
        .map(|d| d.code)
        .filter(|c| c.starts_with("L-BAS"))
        .collect()
}

#[test]
fn l_bas_01_progress_without_a_baseline() {
    let mut p = plan(vec![started(task("a", 5, &[]), date(1, 4))]);
    p.project.status_date = Some(date(1, 6));
    assert_eq!(codes(&p), ["L-BAS-01"]);
    p.baselines.push(Baseline {
        id: "b1".into(),
        name: "b1".into(),
        date: None,
        active: true,
        parent: None,
        change_ref: None,
        reason: None,
        approved_by: None,
    });
    assert!(codes(&p).is_empty());
}

#[test]
fn l_bas_02_progress_without_a_status_date() {
    let mut p = plan(vec![started(task("a", 5, &[]), date(1, 4))]);
    p.rules.severity.insert("L-BAS-01".into(), None);
    assert_eq!(codes(&p), ["L-BAS-02"]);
    p.project.status_date = Some(date(1, 6));
    assert!(codes(&p).is_empty());
}

#[test]
fn l_bas_04_actuals_after_the_status_date_or_against_logic() {
    let mut p = plan(vec![
        task("a", 5, &[]),
        started(task("b", 5, &["a"]), date(1, 5)),
    ]);
    p.rules.severity.insert("L-BAS-01".into(), None);
    p.project.status_date = Some(date(1, 6));
    // `b` started but its mandatory FS predecessor `a` has not.
    assert_eq!(codes(&p), ["L-BAS-04"]);
    // A discretionary link may be overlapped.
    p.tasks[1].deps[0].nature = DepNature::Discretionary;
    assert!(codes(&p).is_empty());
    // A start after the status date is in the future.
    p.tasks[1].actual_start = Some(date(1, 7));
    assert_eq!(codes(&p), ["L-BAS-04"]);
}

fn arb_progress_plan() -> impl Strategy<Value = Plan> {
    (2usize..10).prop_flat_map(|n| {
        (
            proptest::collection::vec(1i64..6, n),
            proptest::collection::vec(proptest::option::of(0usize..10), n),
            proptest::collection::vec(0u8..4, n),
            proptest::collection::vec(proptest::option::of(0i64..5), n),
            0i64..20,
            any::<bool>(),
        )
            .prop_map(|(durs, preds, state, rem, status, retained)| {
                let start = date(1, 4);
                let day = |k: i64| start.checked_add(jiff::Span::new().days(k)).unwrap();
                let status_day = day(status);
                let mut tasks = Vec::new();
                for (i, d) in durs.iter().enumerate() {
                    let deps: Vec<&str> = Vec::new();
                    let mut t = task(&format!("t{i}"), *d, &deps);
                    if let Some(p) = preds[i].filter(|&p| p < i) {
                        t.deps.push(Dep::fs(format!("t{p}")));
                    }
                    // 0: not started, 1: in progress, 2: done, 3: in progress with remaining.
                    if state[i] >= 1 && status > 0 {
                        let s = day(i as i64 % status);
                        t.actual_start = Some(s);
                        if state[i] == 2 {
                            t.actual_finish = Some(status_day.min(day(i as i64 % status + d)));
                        }
                        if state[i] == 3
                            && let Some(r) = rem[i]
                        {
                            t.remaining = Some(Duration::working(r * DAY));
                        }
                    }
                    tasks.push(t);
                }
                let mut p = plan(tasks);
                p.project.status_date = Some(status_day);
                if !retained {
                    p.settings.progress_logic = ProgressLogic::ProgressOverride;
                }
                p
            })
    })
}

proptest! {
    /// Invariants of scheduling with progress: started work keeps its actual start, nothing
    /// not started begins before the data date, remaining work never runs before it, every
    /// finish follows its start, and an incomplete task never has negative remaining work.
    #[test]
    fn progress_invariants(p in arb_progress_plan()) {
        let s = schedule(&p).unwrap();
        let status = p.project.status_date.unwrap();
        let data = alma_core::time::start_of(status) + alma_core::time::DAY;
        // An actual date on a non-working day means its next working minute.
        let cal = alma_sched::WorkCalendar::new(&Calendar::standard());
        for (i, t) in p.tasks.iter().enumerate() {
            let d = &s.tasks[i];
            prop_assert!(d.ef >= d.es, "{}", t.id);
            match d.progress {
                Progress::NotStarted => prop_assert!(d.es >= data, "{} starts before the data date", t.id),
                Progress::InProgress { resume, remaining, .. } => {
                    prop_assert!(remaining >= 0);
                    prop_assert!(remaining == 0 || resume >= data, "{} resumes before the data date", t.id);
                    prop_assert_eq!(d.es, cal.next_start(alma_core::time::start_of(t.actual_start.unwrap())));
                }
                Progress::Complete => {
                    prop_assert_eq!(d.es, cal.next_start(alma_core::time::start_of(t.actual_start.unwrap())));
                    prop_assert_eq!(d.total_float, None);
                }
            }
            if let Some(tf) = d.total_float {
                prop_assert!(d.free_float.unwrap_or(0) <= tf.max(0));
            }
        }
    }
}
