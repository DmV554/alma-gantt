//! Golden test: the M0 fixture schedules to hand-checked dates and lints cleanly (no error, no
//! warn; decision D16 in docs/format.md).

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
    let views = task_views(
        loaded.plan.as_ref().unwrap(),
        loaded.schedule.as_ref().unwrap(),
    );
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
    assert_eq!(get("golive").kind, "summary");
    assert_eq!(get("golive").wbs, "6");
    assert_eq!(get("h4").wbs, "6.2");
}
