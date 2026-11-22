//! Iterations, stories, velocity and forecasts (ADR 0021; knowledge notes, scheduling
//! §5.3–5.6; F-COST-29), worked out by hand.
//!
//! Four two-week sprints from Monday 2027-01-04: s01 01-04..01-15, s02 01-18..01-29,
//! s03 02-01..02-12, s04 02-15..02-26 (s05 03-01..03-12, s06 03-15..03-26 and s07
//! 03-29..04-09 if the cadence went on). The status date is 01-29, so s01 and s02 have ended.
//!
//! - s01 commits 8 + 5 + 5 = 18 points and finishes 13 (SPI 13/18 = 0.722).
//! - s02 commits 8 + 5 = 13 and finishes 8 (SPI 0.615).
//! - Velocity: mean of 13 and 8 = 10.5.
//! - Left: 5 (s01) + 5 (s02) + 8 (s03) + 16 in the backlog = 34 points.
//! - Forecast: ceil(34 / 10.5) = 4 more iterations from s03, ending with s06 on 03-26. The
//!   best velocity (13) needs 3 (s05, 03-12) and the worst (8) needs 5 (s07, 04-09).

use alma_ops::agile::report;
use alma_ops::load;
use alma_sched::Driver;

const PLAN: &str = r#"[project]
format = 0
name = "Agile"
start = 2027-01-04
status_date = 2027-01-29

[[cadence]]
id = "sprints"
prefix = "s"
start = 2027-01-04
length = "2w"
count = 4
capacity = 20

[[task]]
id = "kickoff"
name = "Kickoff"
kind = "milestone"
actual_finish = 2027-01-04

[[task]]
id = "app"
name = "App"
approach = "iterative"
deps = [{ pred = "kickoff" }]

[[task]]
id = "st1"
parent = "app"
name = "Login"
kind = "story"
iteration = "s01"
points = 8
actual_start = 2027-01-04
actual_finish = 2027-01-14

[[task]]
id = "st2"
parent = "app"
name = "Search"
kind = "story"
iteration = "s01"
points = 5
actual_start = 2027-01-04
actual_finish = 2027-01-15

[[task]]
id = "st3"
parent = "app"
name = "Export"
kind = "story"
iteration = "s01"
points = 5

[[task]]
id = "st4"
parent = "app"
name = "Profile"
kind = "story"
iteration = "s02"
points = 8
actual_start = 2027-01-18
actual_finish = 2027-01-27

[[task]]
id = "st5"
parent = "app"
name = "Alerts"
kind = "story"
iteration = "s02"
points = 5
actual_start = 2027-01-20

[[task]]
id = "st6"
parent = "app"
name = "Reports"
kind = "story"
iteration = "s03"
points = 8

[[task]]
id = "st7"
parent = "app"
name = "Billing"
kind = "story"
points = 13

[[task]]
id = "st8"
parent = "app"
name = "Audit log"
kind = "story"
points = 3
"#;

#[test]
fn f_cost_29_velocity_and_forecast() {
    let l = load(PLAN);
    let errors: Vec<_> = l
        .diagnostics
        .iter()
        .filter(|d| d.code.starts_with("E-"))
        .collect();
    assert!(errors.is_empty(), "{errors:?}");
    let r = report(l.plan.as_ref().unwrap(), None).unwrap();
    let c = &r.cadences[0];
    let s01 = &c.iterations[0];
    assert_eq!(
        (s01.start.as_str(), s01.finish.as_str(), s01.state),
        ("2027-01-04", "2027-01-15", "done")
    );
    assert_eq!(
        (s01.committed, s01.done, s01.spi),
        (18.0, 13.0, Some(0.722))
    );
    assert_eq!(
        (c.iterations[1].done, c.iterations[1].spi),
        (8.0, Some(0.615))
    );
    assert_eq!(c.iterations[2].state, "planned");
    assert_eq!(c.velocities, [13.0, 8.0]);
    assert_eq!((r.backlog, r.backlog_items), (16.0, 2));
    let f = c.forecast.as_ref().unwrap();
    assert_eq!((f.remaining, f.velocity), (34.0, 10.5));
    assert_eq!((f.iterations, f.finish.as_str()), (4, "2027-03-26"));
    assert_eq!(f.optimistic, Some((3, "2027-03-12".into())));
    assert_eq!(f.pessimistic, Some((5, "2027-04-09".into())));
}

/// A story spans its iteration; the backlog has no dates and stays out of the container,
/// which spans s01 to s03.
#[test]
fn stories_span_their_iteration() {
    let l = load(PLAN);
    let (p, s) = (l.plan.as_ref().unwrap(), l.schedule.as_ref().unwrap());
    let at = |id: &str| p.tasks.iter().position(|t| t.id == id).unwrap();
    let fmt = alma_core::time::format_instant;
    let st6 = &s.tasks[at("st6")];
    assert_eq!(
        (fmt(st6.es), fmt(st6.ef)),
        ("2027-02-01T09:00".into(), "2027-02-12T18:00".into())
    );
    assert_eq!(st6.driver, Driver::Iteration);
    assert_eq!(s.tasks[at("st7")].driver, Driver::Backlog);
    let app = &s.tasks[at("app")];
    assert_eq!(
        (fmt(app.es), fmt(app.ef)),
        ("2027-01-04T09:00".into(), "2027-02-12T18:00".into())
    );
    // A link on the container waits for its last planned iteration.
    let with_release = format!(
        "{PLAN}\n[[task]]\nid = \"release\"\nname = \"Release\"\nkind = \"milestone\"\ndeps = [{{ pred = \"app\" }}]\n"
    );
    let l2 = load(&with_release);
    let p2 = l2.plan.as_ref().unwrap();
    let r = p2.tasks.iter().position(|t| t.id == "release").unwrap();
    assert_eq!(
        fmt(l2.schedule.as_ref().unwrap().tasks[r].es),
        "2027-02-12T18:00"
    );
    // Finished stories count as complete for earned value and progress.
    assert_eq!(
        s.tasks[at("st1")].progress,
        alma_sched::cpm::Progress::Complete
    );
    // No open-end or owner noise for stories.
    let story_lint: Vec<_> = l
        .diagnostics
        .iter()
        .filter(|d| d.code != "L-BAS-01" && d.ids.iter().any(|id| id.starts_with("st")))
        .collect();
    assert!(story_lint.is_empty(), "{story_lint:?}");
}

fn codes(src: &str) -> Vec<&'static str> {
    load(src)
        .diagnostics
        .iter()
        .map(|d| d.code)
        .filter(|c| c.starts_with("L-AGL"))
        .collect()
}

#[test]
fn l_agl_07_capacity() {
    assert!(codes(PLAN).is_empty());
    assert_eq!(
        codes(&PLAN.replace("capacity = 20", "capacity = 15")),
        ["L-AGL-07"]
    );
}

#[test]
fn l_agl_06_timebox_length() {
    assert_eq!(
        codes(&PLAN.replace("length = \"2w\"", "length = \"6w\"")),
        ["L-AGL-06"]
    );
    assert_eq!(
        codes(&PLAN.replace("length = \"2w\"", "length = \"3d\"")),
        ["L-AGL-06"]
    );
}

/// An ordinary task planned in s01 but scheduled after it.
#[test]
fn l_agl_04_work_outside_its_iteration() {
    let late = format!(
        "{PLAN}\n[[task]]\nid = \"spike\"\nname = \"Spike\"\nduration = \"3d\"\niteration = \"s01\"\nconstraint = {{ type = \"SNET\", date = 2027-01-25 }}\ndeps = [{{ pred = \"kickoff\" }}]\n"
    );
    assert_eq!(codes(&late), ["L-AGL-04"]);
}

#[test]
fn stories_take_no_duration_or_links() {
    let l = |extra: &str| {
        load(&PLAN.replace("id = \"st8\"\n", &format!("id = \"st8\"\n{extra}")))
            .diagnostics
            .iter()
            .map(|d| d.code)
            .filter(|c| c.starts_with("E-"))
            .collect::<Vec<_>>()
    };
    assert_eq!(l("duration = \"2d\"\n"), ["E-FMT-03"]);
    assert_eq!(l("deps = [{ pred = \"kickoff\" }]\n"), ["E-FMT-03"]);
    assert_eq!(l("iteration = \"s09\"\n"), ["E-REF-03"]);
    // Nothing may depend on a story.
    let dep = format!(
        "{PLAN}\n[[task]]\nid = \"x\"\nname = \"X\"\nduration = \"1d\"\ndeps = [{{ pred = \"st1\" }}]\n"
    );
    assert!(load(&dep).diagnostics.iter().any(|d| d.code == "E-FMT-03"));
}

/// s02 has ten working days and 13 points. The ideal line loses 1.3 points a day. `st4`
/// (8 points) finished on 01-27, the eighth day, so 5 points are left from then to the status
/// date 01-29, the last day.
#[test]
fn burndown_of_an_iteration() {
    let l = load(PLAN);
    let b = alma_ops::agile::burndown(l.plan.as_ref().unwrap(), "s02").unwrap();
    assert_eq!(b.days.len(), 10);
    assert_eq!(
        (b.days[0].as_str(), b.days[9].as_str()),
        ("2027-01-18", "2027-01-29")
    );
    assert_eq!(b.committed, 13.0);
    assert_eq!((b.ideal[0], b.ideal[9]), (11.7, 0.0));
    assert_eq!(
        b.remaining,
        [13.0, 13.0, 13.0, 13.0, 13.0, 13.0, 13.0, 5.0, 5.0, 5.0]
    );
    // s03 has not started: no actual line.
    let b3 = alma_ops::agile::burndown(l.plan.as_ref().unwrap(), "s03").unwrap();
    assert!(b3.remaining.is_empty());
    assert!(alma_ops::agile::burndown(l.plan.as_ref().unwrap(), "s99").is_err());
}

/// Planned: 18, 31, 39, 39. Done through the started iterations: 13, then 21. Scope: the 39
/// points in sprints plus 16 in the backlog.
#[test]
fn burnup_of_a_cadence() {
    let l = load(PLAN);
    let b = alma_ops::agile::burnup(l.plan.as_ref().unwrap(), None).unwrap();
    assert_eq!(b.planned, [18.0, 31.0, 39.0, 39.0]);
    assert_eq!(b.done, [13.0, 21.0]);
    assert_eq!(b.scope, 55.0);
}
