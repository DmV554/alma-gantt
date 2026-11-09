//! Lint rule tests, one per rule id (knowledge notes, Annex A), plus configuration.

use alma_core::diag::Severity;
use alma_core::model::*;
use alma_core::{Duration, WorkUnits, parse_duration};
use alma_sched::{lint, schedule};
use jiff::civil::Date;

const DAY: i64 = 480;

fn plan(tasks: Vec<Task>) -> Plan {
    Plan {
        project: Project {
            name: "t".into(),
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
        reserves: Vec::new(),
        risks: Vec::new(),
        funding: Vec::new(),
    }
}

fn task(id: &str, days: i64, deps: &[&str]) -> Task {
    let mut t = Task::new(id, id);
    t.duration = Some(Duration::working(days * DAY));
    t.owner = Some("pm".into());
    t.deps = deps.iter().map(|p| Dep::fs(*p)).collect();
    t
}

fn milestone(id: &str, deps: &[&str]) -> Task {
    let mut t = Task::new(id, id);
    t.kind = TaskKind::Milestone;
    t.owner = Some("pm".into());
    t.acceptance = Some("done".into());
    t.deps = deps.iter().map(|p| Dep::fs(*p)).collect();
    t
}

/// A clean three-node chain under one phase.
fn base() -> Vec<Task> {
    let mut v = vec![
        Task::new("phase", "Phase"),
        milestone("start", &[]),
        task("work", 5, &["start"]),
        milestone("end", &["work"]),
    ];
    for t in &mut v[1..] {
        t.parent = Some("phase".into());
    }
    v
}

fn codes(p: &Plan) -> Vec<(&'static str, Severity)> {
    let s = schedule(p).ok();
    lint(p, s.as_ref())
        .into_iter()
        .map(|d| (d.code, d.severity))
        .collect()
}

fn has(p: &Plan, code: &str) -> bool {
    codes(p).iter().any(|(c, _)| *c == code)
}

#[test]
fn clean_plan_has_no_findings() {
    assert_eq!(codes(&plan(base())), vec![]);
}

#[test]
fn l_dep_01_open_start_and_l_dep_02_open_finish() {
    let mut t = base();
    t.push(task("orphan", 2, &[]));
    let c = codes(&plan(t));
    assert!(c.contains(&("L-DEP-01", Severity::Warn)));
    assert!(c.contains(&("L-DEP-02", Severity::Warn)));
}

#[test]
fn l_dep_01_suppressed_inside_iterative_branch() {
    let mut t = base();
    let mut b = Task::new("sprints", "Sprints");
    b.approach = Some(Approach::Iterative);
    let mut s = task("story-ish", 2, &[]);
    s.parent = Some("sprints".into());
    t.push(b);
    t.push(s);
    let p = plan(t);
    assert!(!has(&p, "L-DEP-01"));
    assert!(!has(&p, "L-DEP-02"));
}

#[test]
fn l_dep_04_duplicate_link_and_l_dep_05_sf() {
    let mut t = base();
    t[2].deps.push(Dep::fs("start"));
    t[3].deps[0].kind = DepType::Sf;
    let p = plan(t);
    assert!(has(&p, "L-DEP-04"));
    assert!(has(&p, "L-DEP-05"));
}

#[test]
fn l_dep_06_lead_l_dep_11_long_lag_l_dep_12_no_rationale() {
    let mut t = base();
    let lag = |s| parse_duration(s, WorkUnits::default(), true).unwrap();
    t[3].deps[0].lag = lag("-1d");
    let p = plan(t.clone());
    assert!(has(&p, "L-DEP-06"));
    assert!(has(&p, "L-DEP-12"));
    t[3].deps[0].lag = lag("20d");
    t[3].deps[0].rationale = Some("supplier lead time".into());
    let p = plan(t);
    assert!(has(&p, "L-DEP-11"));
    assert!(!has(&p, "L-DEP-12"));
}

#[test]
fn l_dep_07_external_and_l_dep_08_discretionary_without_rationale() {
    let mut t = base();
    t[2].deps[0].scope = DepScope::External;
    t[3].deps[0].nature = DepNature::Discretionary;
    let p = plan(t);
    assert!(has(&p, "L-DEP-07"));
    assert!(has(&p, "L-DEP-08"));
}

#[test]
fn l_dep_09_link_on_summary() {
    let mut t = base();
    t.push(task("after", 1, &["phase"]));
    t.push(milestone("fin", &["after"]));
    assert!(has(&plan(t), "L-DEP-09"));
}

#[test]
fn l_sch_01_milestone_with_duration() {
    let mut t = base();
    t[3].duration = Some(Duration::working(DAY));
    assert!(codes(&plan(t)).contains(&("L-SCH-01", Severity::Error)));
}

#[test]
fn l_sch_02_milestone_without_criterion() {
    let mut t = base();
    t[3].acceptance = None;
    assert!(codes(&plan(t.clone())).contains(&("L-SCH-02", Severity::Info)));
    t[3].class = Some(MilestoneClass::Contractual);
    assert!(codes(&plan(t)).contains(&("L-SCH-02", Severity::Warn)));
}

#[test]
fn l_sch_04_activity_without_owner() {
    let mut t = base();
    t[2].owner = None;
    assert!(has(&plan(t), "L-SCH-04"));
}

#[test]
fn l_sch_05_negative_float_depends_on_phase() {
    let mut t = base();
    t[3].deadline = Some(Date::constant(2027, 1, 5));
    let mut p = plan(t);
    assert!(codes(&p).contains(&("L-SCH-05", Severity::Warn)), "draft");
    p.settings.phase = Phase::Baselined;
    assert!(
        codes(&p).contains(&("L-SCH-05", Severity::Error)),
        "baselined"
    );
}

#[test]
fn l_sch_06_hard_constraint_with_predecessors_and_l_sch_11_snet_only() {
    let mut t = base();
    t[2].constraint = Some(Constraint {
        kind: ConstraintKind::Mso,
        date: Some(Date::constant(2027, 1, 11)),
    });
    let mut lone = task("lone", 1, &[]);
    lone.constraint = Some(Constraint {
        kind: ConstraintKind::Snet,
        date: Some(Date::constant(2027, 2, 1)),
    });
    lone.parent = Some("phase".into());
    t.push(lone);
    t[3].deps.push(Dep::fs("lone"));
    let p = plan(t);
    assert!(has(&p, "L-SCH-06"));
    assert!(has(&p, "L-SCH-11"));
}

#[test]
fn l_sch_09_loe_with_links() {
    let mut t = base();
    let mut loe = Task::new("pm", "PM");
    loe.kind = TaskKind::Loe;
    loe.parent = Some("phase".into());
    loe.spans = Some(("start".into(), "end".into()));
    loe.deps = vec![Dep::fs("start")];
    t.push(loe);
    assert!(codes(&plan(t)).contains(&("L-SCH-09", Severity::Error)));
}

#[test]
fn l_wbs_01_single_child_and_l_wbs_02_flat_plan() {
    let mut t = base();
    let mut only = Task::new("solo", "Solo");
    only.parent = None;
    let mut c = task("solo-child", 1, &["start"]);
    c.parent = Some("solo".into());
    t.push(only);
    t.push(c);
    t[3].deps.push(Dep::fs("solo-child"));
    assert!(has(&plan(t), "L-WBS-01"));
    let flat = plan(vec![
        milestone("a", &[]),
        task("b", 1, &["a"]),
        milestone("c", &["b"]),
    ]);
    assert!(has(&flat, "L-WBS-02"));
}

#[test]
fn l_wbs_04_l_wbs_07_l_wbs_12_and_l_cost_02_work_packages() {
    let mut t = base();
    t[0].wbs_role = Some(WbsRole::ControlAccount);
    t[2].wbs_role = Some(WbsRole::WorkPackage);
    t[2].owner = None;
    let mut stray = task("stray", 1, &["start"]);
    stray.wbs_role = Some(WbsRole::WorkPackage);
    t.push(stray);
    t[3].deps.push(Dep::fs("stray"));
    let mut empty = Task::new("empty-ca", "Empty");
    empty.wbs_role = Some(WbsRole::ControlAccount);
    t.push(empty);
    let c = codes(&plan(t));
    for code in ["L-WBS-04", "L-WBS-07", "L-WBS-12", "L-COST-02"] {
        assert!(c.iter().any(|(x, _)| *x == code), "{code}: {c:?}");
    }
}

#[test]
fn l_wbs_05_effort_out_of_range() {
    let mut t = base();
    t[2].wbs_role = Some(WbsRole::WorkPackage);
    t[2].description = Some("d".into());
    t[2].deliverable = Some("d".into());
    t[2].acceptance = Some("d".into());
    t[2].assign = vec![Assignment {
        resource: "dev".into(),
        units: 3.into(),
    }];
    // 5 d × 8 h × 3 = 120 h > 80 h.
    assert!(has(&plan(t), "L-WBS-05"));
}

#[test]
fn l_wbs_10_no_final_milestone() {
    let mut t = base();
    t.pop();
    assert!(has(&plan(t), "L-WBS-10"));
}

#[test]
fn l_est_01_no_basis_l_est_02_unordered_l_est_09_outside() {
    let mut t = base();
    let d = |n| Duration::working(n * DAY);
    t[2].estimate = Some(Estimate {
        o: d(6),
        m: d(4),
        p: d(9),
        dist: None,
        basis: None,
        confidence: None,
    });
    let c = codes(&plan(t));
    assert!(c.contains(&("L-EST-01", Severity::Info)));
    assert!(c.contains(&("L-EST-02", Severity::Error)));
    assert!(c.contains(&("L-EST-09", Severity::Info)));
}

#[test]
fn l_cal_01_day_length_mismatch() {
    let mut p = plan(base());
    let mut cal = Calendar::standard();
    cal.id = "long".into();
    cal.hours = vec![(9 * 60, 18 * 60)];
    p.calendars.push(cal);
    assert!(has(&p, "L-CAL-01"));
}

#[test]
fn rule_configuration_and_allow() {
    let mut t = base();
    t.push(task("orphan", 2, &[]));
    let mut p = plan(t);
    p.rules.severity.insert("L-DEP-01".into(), None);
    p.rules
        .severity
        .insert("L-DEP-02".into(), Some(Severity::Error));
    let c = codes(&p);
    assert!(!c.iter().any(|(x, _)| *x == "L-DEP-01"));
    assert!(c.contains(&("L-DEP-02", Severity::Error)));
    p.tasks.last_mut().unwrap().allow = vec!["L-DEP-02".into()];
    assert!(!has(&p, "L-DEP-02"));
}

#[test]
fn lint_runs_without_a_schedule() {
    let mut t = base();
    t[1].deps.push(Dep::fs("end"));
    let p = plan(t);
    assert!(schedule(&p).is_err());
    let _ = lint(&p, None);
}

#[test]
fn budget_5000_tasks_under_50ms() {
    // SPEC.md §12. Measured in release builds only; debug builds are an order of magnitude slower.
    let mut tasks = Vec::with_capacity(5000);
    for i in 0..5000 {
        let mut t = task(&format!("t{i}"), (i % 7 + 1) as i64, &[]);
        if i > 0 {
            t.deps.push(Dep::fs(format!("t{}", i - 1)));
        }
        if i > 10 {
            t.deps.push(Dep {
                kind: DepType::Ss,
                lag: Duration::working(DAY),
                ..Dep::fs(format!("t{}", i - 10))
            });
        }
        tasks.push(t);
    }
    let p = plan(tasks);
    let start = std::time::Instant::now();
    let s = schedule(&p).unwrap();
    let elapsed = start.elapsed();
    assert!(s.finish > s.start);
    if !cfg!(debug_assertions) {
        assert!(elapsed.as_millis() < 50, "took {elapsed:?}");
    }
}

#[test]
fn rules_are_listed_in_code_order() {
    let codes: Vec<&str> = alma_sched::RULES.iter().map(|r| r.code).collect();
    let mut sorted = codes.clone();
    sorted.sort();
    assert_eq!(codes, sorted);
}
