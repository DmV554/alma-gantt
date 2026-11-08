//! Golden test: the fixture schedules to hand-checked dates and lints cleanly (no error, no
//! warn; decision D16 in docs/format.md). It is checked twice: with logic alone (CPM), and
//! leveled as the file asks (ADR 0011).

use alma_core::diag::Severity;
use alma_ops::query::task_views;

#[test]
fn hybrid_terminal_fixture() {
    let src = include_str!("../../../examples/hybrid-terminal.alma.toml");
    let loaded = alma_ops::load(src);
    let bad: Vec<_> = loaded
        .diagnostics
        .iter()
        .filter(|d| d.severity >= Severity::Warn)
        .collect();
    assert!(bad.is_empty(), "{bad:#?}");
    // Logic alone: the file asks for leveling; turn it off to check the CPM pass by itself.
    let mut logic = loaded.plan.clone().unwrap();
    logic.settings.leveling = alma_core::model::Leveling::Off;
    let logic_schedule = alma_sched::schedule(&logic).unwrap();
    let views = task_views(&logic, &logic_schedule);
    let get = |id: &str| views.iter().find(|v| v.id == id).unwrap();
    // Worked out by hand on the Mon–Fri, 8 h calendar from 2027-01-04.
    let expected = [
        ("kickoff", "2027-01-04", "2027-01-04", 0.0),
        ("discovery", "2027-01-04", "2027-02-12", 0.0),
        ("platform-env", "2027-02-15", "2027-02-26", 0.0),
        ("platform-cicd", "2027-03-01", "2027-03-05", 0.0),
        ("gate-sw", "2027-03-08", "2027-04-30", 10.0),
        ("gate-ocr-procure", "2027-02-15", "2027-03-12", 55.0),
        ("yard-sw", "2027-03-08", "2027-05-14", 0.0),
        ("reefer-sw", "2027-03-01", "2027-04-09", 25.0),
        ("integration-tests", "2027-05-17", "2027-06-04", 0.0),
        ("h4", "2027-06-04", "2027-06-04", 0.0),
        ("pilot", "2027-06-21", "2027-07-09", 0.0),
        ("finish", "2027-07-30", "2027-07-30", 0.0),
    ];
    for (id, start, finish, tf) in expected {
        let v = get(id);
        assert_eq!(
            (v.start.as_str(), v.finish.as_str(), v.tf),
            (start, finish, Some(tf)),
            "{id}"
        );
    }
    // Leveled, as the file asks. `dev` has 6 people; gate software (3) and yard software (4)
    // cannot overlap. Yard has no float, so it keeps its dates and gate software waits until
    // 2027-05-17. Reefer software (2) fits next to yard (4). Everything after gate software
    // moves 40 working days. The resource link yard-sw → gate-sw puts both on the critical
    // path. `fld` (4 technicians): the LTE network (3) and the OCR portals (2) overlapped one
    // week; the portals, with more float, wait until the LTE crew is done.
    let views = task_views(
        loaded.plan.as_ref().unwrap(),
        loaded.schedule.as_ref().unwrap(),
    );
    let get = |id: &str| views.iter().find(|v| v.id == id).unwrap();
    let expected = [
        ("kickoff", "2027-01-04", "2027-01-04", 0.0),
        ("platform-cicd", "2027-03-01", "2027-03-05", 0.0),
        ("yard-sw", "2027-03-08", "2027-05-14", 0.0),
        ("gate-sw", "2027-05-17", "2027-07-09", 0.0),
        ("reefer-sw", "2027-03-01", "2027-04-09", 65.0),
        ("yard-lte", "2027-02-15", "2027-03-19", 85.0),
        ("gate-ocr", "2027-03-22", "2027-04-09", 90.0),
        ("integration-tests", "2027-07-12", "2027-07-30", 0.0),
        ("finish", "2027-09-24", "2027-09-24", 0.0),
    ];
    for (id, start, finish, tf) in expected {
        let v = get(id);
        assert_eq!(
            (v.start.as_str(), v.finish.as_str(), v.tf),
            (start, finish, Some(tf)),
            "leveled {id}"
        );
    }
    // Costs, by hand: days of work × units × day rate, plus fixed costs (180 + 420 + 260).
    // pm 40 d × 6 = 240; dev 560 d × 4.2 = 2,352; qa 200 d × 3.6 = 720; fld 175 d × 3.1 =
    // 542.5. Total 4,714.5 UF. Gate software: 40 d × (3 × 4.2 + 3.6) = 648.
    assert_eq!(get("gate-sw").cost, 648.0);
    let c = alma_ops::cost::report(
        loaded.plan.as_ref().unwrap(),
        loaded.schedule.as_ref().unwrap(),
        "resource",
        1,
    )
    .unwrap();
    let by: Vec<(String, f64, Option<f64>)> = c
        .rows
        .iter()
        .map(|r| (r.id.clone(), r.cost, r.work_h))
        .collect();
    assert_eq!(
        by,
        [
            ("pm".to_string(), 240.0, Some(320.0)),
            ("dev".to_string(), 2352.0, Some(4480.0)),
            ("qa".to_string(), 720.0, Some(1600.0)),
            ("fld".to_string(), 542.5, Some(1400.0)),
        ]
    );
    assert_eq!(c.total, 4714.5);
    let curve = alma_ops::cost::scurve(
        loaded.plan.as_ref().unwrap(),
        loaded.schedule.as_ref().unwrap(),
        "month",
        None,
    )
    .unwrap();
    assert_eq!(curve.points.last().unwrap().cumulative, 4714.5);
    assert_eq!(curve.points.last().unwrap().end, "2027-09-30");
    assert_eq!(get("golive").kind, "summary");
    assert_eq!(get("golive").wbs, "6");
    assert_eq!(get("h4").wbs, "6.2");
}
