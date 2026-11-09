//! Earned value (ADR 0019; knowledge notes, resources §5.3–5.6; F-COST-13 to F-COST-30),
//! worked out by hand.
//!
//! The plan: one developer at 100 a day (8 h). `a` (10 days) runs 2027-01-04 to 01-15 and `b`
//! (10 days, after `a`) runs 01-18 to 01-29; each has a budget of 1000, so BAC = 2000. The
//! status date is Friday 01-22, the end of `b`'s first week.
//!
//! - PV = 1000 (all of `a`) + 500 (five days of `b`) = 1500.
//! - `a` is finished: it earns 1000. `b` is 40 % done: it earns 400. EV = 1400.
//! - AC = 1100 on `a` + 450 on `b` = 1550.

use std::str::FromStr;

use alma_core::Duration;
use alma_core::model::*;
use alma_sched::baseline::snapshot;
use alma_sched::evm::{Measures, evm};
use alma_sched::{lint_baseline, schedule};
use jiff::civil::Date;
use rust_decimal::Decimal;

const DAY: i64 = 480;

fn d(s: &str) -> Decimal {
    Decimal::from_str(s).unwrap()
}

fn date(m: i8, day: i8) -> Date {
    Date::constant(2027, m, day)
}

fn task(id: &str, days: i64, deps: &[&str]) -> Task {
    let mut t = Task::new(id, id);
    t.duration = Some(Duration::working(days * DAY));
    t.deps = deps.iter().map(|p| Dep::fs(*p)).collect();
    t.assign = vec![Assignment {
        resource: "dev".into(),
        units: Decimal::ONE,
    }];
    t
}

fn plan(tasks: Vec<Task>) -> Plan {
    Plan {
        project: Project {
            name: "evm".into(),
            start: date(1, 4),
            calendar: None,
            deadline: None,
            currency: Some("USD".into()),
            description: None,
            status_date: None,
        },
        settings: ScheduleSettings::default(),
        rules: RuleConfig::default(),
        calendars: Vec::new(),
        resources: vec![Resource {
            id: "dev".into(),
            name: "dev".into(),
            kind: ResourceKind::Work,
            rate: Some(Rate {
                amount: Decimal::from(100),
                per: RatePer::Day,
            }),
            max_units: Decimal::ONE,
            calendar: None,
        }],
        cadences: Vec::new(),
        tasks,
        baselines: vec![Baseline {
            id: "b1".into(),
            name: "b1".into(),
            date: None,
            active: true,
            parent: None,
            change_ref: None,
            reason: None,
            approved_by: None,
        }],
        reserves: Vec::new(),
        risks: Vec::new(),
        funding: Vec::new(),
    }
}

/// The baseline of `p`, then `p` with the progress of the module example.
fn measured(p: Plan) -> (Plan, Snapshot) {
    let snap = snapshot("b1", &p, &schedule(&p).unwrap());
    let mut p = p;
    p.project.status_date = Some(date(1, 22));
    let a = p.tasks.iter_mut().find(|t| t.id == "a").unwrap();
    a.actual_start = Some(date(1, 4));
    a.actual_finish = Some(date(1, 15));
    a.actual_cost = Some(d("1100"));
    let b = p.tasks.iter_mut().find(|t| t.id == "b").unwrap();
    b.actual_start = Some(date(1, 18));
    b.physical_pct = Some(d("40"));
    b.actual_cost = Some(d("450"));
    (p, snap)
}

fn example() -> (Plan, Snapshot) {
    measured(plan(vec![task("a", 10, &[]), task("b", 10, &["a"])]))
}

#[test]
fn f_cost_13_14_15_pv_ev_ac() {
    let (p, snap) = example();
    assert!(snap.per_task);
    assert_eq!(snap.tasks[0].budget, d("1000"));
    assert_eq!(snap.tasks[1].pv_through(date(1, 22)), d("500"));
    let e = evm(&p, &schedule(&p).unwrap(), &snap).unwrap();
    assert_eq!(
        e.project,
        Measures {
            bac: d("2000"),
            pv: d("1500"),
            ev: d("1400"),
            ac: d("1550"),
        }
    );
    assert_eq!(e.own[1].ev, d("400"));
}

/// SV = −100, SV% = −100 / 1500; CV = −150, CV% = −150 / 1400; SPI = 1400 / 1500;
/// CPI = 1400 / 1550; complete = 70 %, planned = 75 %.
#[test]
fn f_cost_16_17_18_19_30_variances_and_indices() {
    let (p, snap) = example();
    let x = evm(&p, &schedule(&p).unwrap(), &snap)
        .unwrap()
        .project
        .indices();
    let r = |v: Option<Decimal>| v.unwrap().round_dp(4);
    assert_eq!((x.sv, x.cv), (d("-100"), d("-150")));
    assert_eq!(r(x.sv_pct), d("-0.0667"));
    assert_eq!(r(x.cv_pct), d("-0.1071"));
    assert_eq!(r(x.spi), d("0.9333"));
    assert_eq!(r(x.cpi), d("0.9032"));
    assert_eq!(x.complete, Some(d("0.7")));
    assert_eq!(x.planned, Some(d("0.75")));
}

/// EAC by the budgeted rate = 1550 + 600 = 2150; by CPI = 2000 × 1550 / 1400 = 2214.2857;
/// by CPI × SPI = 1550 + 600 / (1400/1550 × 1400/1500) = 2261.7347. ETC = 664.2857,
/// VAC = −214.2857. TCPI(BAC) = 600 / 450 = 1.3333; TCPI(EAC) = 600 / 664.2857 = CPI.
#[test]
fn f_cost_21_to_28_forecasts() {
    let (p, snap) = example();
    let x = evm(&p, &schedule(&p).unwrap(), &snap)
        .unwrap()
        .project
        .indices();
    let r = |v: Option<Decimal>| v.unwrap().round_dp(4);
    assert_eq!(x.eac_budget_rate, d("2150"));
    assert_eq!(r(x.eac_cpi), d("2214.2857"));
    assert_eq!(r(x.eac_cpi_spi), d("2261.7347"));
    assert_eq!(r(x.etc), d("664.2857"));
    assert_eq!(r(x.vac), d("-214.2857"));
    assert_eq!(r(x.tcpi_bac), d("1.3333"));
    assert_eq!(r(x.tcpi_eac), r(x.cpi));
}

/// Nothing spent and nothing planned yet: the ratios are absent, never infinite.
#[test]
fn f_cost_18_19_division_by_zero_is_none() {
    let x = Measures {
        bac: d("100"),
        ..Measures::default()
    }
    .indices();
    assert_eq!(
        (x.spi, x.cpi, x.eac_cpi, x.tcpi_eac),
        (None, None, None, None)
    );
    assert_eq!(x.tcpi_bac, Some(Decimal::ONE));
}

/// With `ev_method = "50/50"`, `b` earns half its budget when it starts, whatever its
/// physical percent says.
#[test]
fn f_cost_14_fixed_formula() {
    let (mut p, snap) = example();
    p.tasks[1].ev_method = Some(EvMethod::Fixed { start: 50 });
    let e = evm(&p, &schedule(&p).unwrap(), &snap).unwrap();
    assert_eq!(e.own[1].ev, d("500"));
    p.tasks[1].ev_method = Some(EvMethod::parse("0/100").unwrap());
    let e = evm(&p, &schedule(&p).unwrap(), &snap).unwrap();
    assert_eq!(e.own[1].ev, Decimal::ZERO);
    assert_eq!(EvMethod::parse("30/60"), None);
}

/// A level-of-effort task earns what was planned: `pm` (half a developer over `a` and `b`,
/// 20 days × 50 = 1000) has 15 days × 50 = 750 planned by the status date, and earns 750.
#[test]
fn f_cost_14_level_of_effort_earns_as_planned() {
    let mut pm = Task::new("pm", "pm");
    pm.kind = TaskKind::Loe;
    pm.spans = Some(("a".into(), "b".into()));
    pm.assign = vec![Assignment {
        resource: "dev".into(),
        units: d("0.5"),
    }];
    let mut p = plan(vec![task("a", 10, &[]), task("b", 10, &["a"]), pm]);
    p.resources[0].max_units = Decimal::from(2);
    let (p, snap) = measured(p);
    let e = evm(&p, &schedule(&p).unwrap(), &snap).unwrap();
    assert_eq!(e.own[2].bac, d("1000"));
    assert_eq!((e.own[2].pv, e.own[2].ev), (d("750"), d("750")));
}

/// Rows roll up: a summary sums its children and its own actual cost, which is how cost is
/// recorded per control account.
#[test]
fn evm_rolls_up_and_keeps_removed_scope() {
    let mut s = Task::new("s", "s");
    s.wbs_role = Some(WbsRole::ControlAccount);
    let mut a = task("a", 10, &[]);
    a.parent = Some("s".into());
    let mut b = task("b", 10, &["a"]);
    b.parent = Some("s".into());
    let (mut p, snap) = measured(plan(vec![s, a, b, task("c", 5, &[])]));
    for t in &mut p.tasks {
        t.actual_cost = None;
    }
    p.tasks[0].actual_cost = Some(d("1550"));
    // `c` was dropped from the plan without revising the baseline.
    p.tasks.retain(|t| t.id != "c");
    let e = evm(&p, &schedule(&p).unwrap(), &snap).unwrap();
    assert_eq!(
        e.rows[0],
        Measures {
            bac: d("2000"),
            pv: d("1500"),
            ev: d("1400"),
            ac: d("1550"),
        }
    );
    assert_eq!(e.project.bac, d("2500"), "removed scope stays in BAC");
    assert_eq!(e.removed, ["c"]);
}

fn codes(p: &Plan, snap: &Snapshot) -> Vec<&'static str> {
    lint_baseline(p, &schedule(p).unwrap(), snap)
        .into_iter()
        .map(|d| d.code)
        .collect()
}

/// CV% = −10.7 % is beyond the default 10 % threshold; TCPI = 1.33 is above 1.1.
#[test]
fn l_evm_01_05_thresholds() {
    let (mut p, snap) = example();
    assert_eq!(codes(&p, &snap), ["L-EVM-01", "L-EVM-05"]);
    p.rules
        .params
        .insert("cv_threshold".into(), ParamValue::Number(d("15")));
    p.rules
        .params
        .insert("tcpi_max".into(), ParamValue::Number(d("1.5")));
    assert!(codes(&p, &snap).is_empty());
}

#[test]
fn l_evm_02_progress_without_a_budget() {
    let (mut p, snap) = example();
    p.rules.severity.insert("L-EVM-01".into(), None);
    p.rules.severity.insert("L-EVM-05".into(), None);
    let mut c = task("c", 5, &[]);
    c.actual_start = Some(date(1, 18));
    p.tasks.push(c);
    assert_eq!(codes(&p, &snap), ["L-EVM-02"]);
}

#[test]
fn l_evm_03_cost_and_progress_measured_differently() {
    let (mut p, snap) = example();
    p.rules.severity.insert("L-EVM-01".into(), None);
    p.rules.severity.insert("L-EVM-05".into(), None);
    // No actual cost at all while value is earned.
    for t in &mut p.tasks {
        t.actual_cost = None;
    }
    assert_eq!(codes(&p, &snap), ["L-EVM-03"]);
    // Actual cost on work that has not started.
    p.tasks[1].actual_start = None;
    p.tasks[1].physical_pct = None;
    p.tasks[1].actual_cost = Some(d("10"));
    assert_eq!(codes(&p, &snap), ["L-EVM-03"]);
}

/// `b` slips 10 days: its finish milestone `m` is late by more than `finish_slip` (5 days), and
/// so is the project.
#[test]
fn l_bas_03_finish_slip() {
    let mut m = Task::new("m", "m");
    m.kind = TaskKind::Milestone;
    m.deps = vec![Dep::fs("b")];
    let p = plan(vec![task("a", 10, &[]), task("b", 10, &["a"]), m]);
    let snap = snapshot("b1", &p, &schedule(&p).unwrap());
    let mut late = p.clone();
    late.tasks[1].duration = Some(Duration::working(20 * DAY));
    assert_eq!(codes(&late, &snap), ["L-BAS-03", "L-BAS-03"]);
    late.tasks[1].duration = Some(Duration::working(14 * DAY));
    assert!(codes(&late, &snap).is_empty(), "4 days is within 5");
}

#[test]
fn l_evm_08_long_task_measured_zero_one_hundred() {
    let mut p = plan(vec![task("a", 60, &[]), task("b", 10, &["a"])]);
    p.baselines.clear();
    for t in &mut p.tasks {
        t.ev_method = Some(EvMethod::Fixed { start: 0 });
    }
    let found: Vec<&str> = alma_sched::lint(&p, Some(&schedule(&p).unwrap()))
        .into_iter()
        .filter(|d| d.code == "L-EVM-08")
        .flat_map(|d| d.ids)
        .map(|id| if id == "a" { "a" } else { "other" })
        .collect();
    assert_eq!(found, ["a"]);
}

proptest::proptest! {
    /// EV never exceeds the budget of any task, PV never exceeds BAC, and nothing is negative
    /// (F-COST-14: earned_pct ≤ 1).
    #[test]
    fn ev_is_capped_by_bac(
        days in proptest::collection::vec(1i64..15, 2..8),
        state in proptest::collection::vec(0u8..4, 8),
        pct in proptest::collection::vec(0u32..=100, 8),
        status in 0i64..60,
    ) {
        let tasks: Vec<Task> = days
            .iter()
            .enumerate()
            .map(|(i, &n)| {
                let deps: Vec<String> = if i > 0 { vec![format!("t{}", i - 1)] } else { Vec::new() };
                let refs: Vec<&str> = deps.iter().map(String::as_str).collect();
                task(&format!("t{i}"), n, &refs)
            })
            .collect();
        let p = plan(tasks);
        let snap = snapshot("b1", &p, &schedule(&p).unwrap());
        let mut p = p;
        let day = |k: i64| date(1, 4).checked_add(jiff::Span::new().days(k)).unwrap();
        p.project.status_date = Some(day(status));
        for (i, t) in p.tasks.iter_mut().enumerate() {
            if state[i] >= 1 && status > 0 {
                t.actual_start = Some(day(0));
                t.physical_pct = Some(Decimal::from(pct[i]));
                if state[i] == 2 {
                    t.actual_finish = Some(day(status));
                    t.physical_pct = None;
                }
                if state[i] == 3 {
                    t.ev_method = Some(EvMethod::Fixed { start: 20 });
                }
            }
        }
        let e = evm(&p, &schedule(&p).unwrap(), &snap).unwrap();
        for m in &e.own {
            proptest::prop_assert!(m.ev >= Decimal::ZERO && m.ev <= m.bac, "{m:?}");
            proptest::prop_assert!(m.pv >= Decimal::ZERO && m.pv <= m.bac, "{m:?}");
        }
        proptest::prop_assert!(e.project.ev <= e.project.bac);
    }
}
