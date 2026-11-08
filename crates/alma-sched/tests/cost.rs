//! Costs and their distribution over time (SPEC.md §7.5, F-COST-07, F-COST-08, F-COST-12,
//! L-RES-05, L-COST-01, ADR 0012).

use std::str::FromStr;

use alma_core::model::*;
use alma_core::time::start_of;
use alma_core::{Duration, WorkUnits, parse_duration};
use alma_sched::cost::{Accrual, costs, curve, hourly, share};
use alma_sched::cpm::Calendars;
use alma_sched::resources::Period;
use alma_sched::{lint, schedule};
use jiff::civil::Date;
use proptest::prelude::*;
use rust_decimal::Decimal;

const DAY: i64 = 480;

fn dec(s: &str) -> Decimal {
    Decimal::from_str(s).unwrap()
}

fn resource(id: &str, rate: Option<(&str, RatePer)>, kind: ResourceKind) -> Resource {
    Resource {
        id: id.into(),
        name: id.into(),
        kind,
        rate: rate.map(|(a, per)| Rate {
            amount: dec(a),
            per,
        }),
        max_units: Decimal::from(10),
        calendar: None,
    }
}

fn plan(tasks: Vec<Task>, resources: Vec<Resource>) -> Plan {
    Plan {
        project: Project {
            name: "test".into(),
            // A Monday.
            start: Date::constant(2027, 1, 4),
            calendar: None,
            deadline: None,
            currency: Some("UF".into()),
            description: None,
        },
        settings: ScheduleSettings::default(),
        rules: RuleConfig::default(),
        calendars: Vec::new(),
        resources,
        cadences: Vec::new(),
        tasks,
    }
}

fn task(id: &str, days: i64, deps: &[&str], assign: &[(&str, &str)]) -> Task {
    let mut t = Task::new(id, id);
    t.duration = Some(Duration::working(days * DAY));
    t.deps = deps.iter().map(|p| Dep::fs(*p)).collect();
    t.assign = assign
        .iter()
        .map(|(r, u)| Assignment {
            resource: (*r).into(),
            units: dec(u),
        })
        .collect();
    t
}

#[test]
fn f_cost_07_rates_per_unit_of_time() {
    let u = WorkUnits::default();
    let r = |a: &str, per| Rate {
        amount: dec(a),
        per,
    };
    assert_eq!(hourly(&r("10", RatePer::Hour), u), Some(dec("10")));
    assert_eq!(hourly(&r("80", RatePer::Day), u), Some(dec("10")));
    assert_eq!(hourly(&r("400", RatePer::Week), u), Some(dec("10")));
    // A month is 52/12 weeks: 40 h × 52 / 12 = 173.33… h.
    let m = hourly(&r("1733.3333333333333333333333", RatePer::Month), u).unwrap();
    assert_eq!(m.round_dp(6), dec("10"));
    assert_eq!(hourly(&r("5", RatePer::Use), u), None);
}

/// Worked by hand:
/// - `a`: 5 days, 2 developers at 4.2/day → 5 × 2 × 4.2 = 42, plus a fixed cost of 100 at the
///   start;
/// - `b`: 2 days after `a`, one crane at 0.5/hour → 16 h × 0.5 = 8, plus a mobilisation fee of 3
///   per use; 20 m of cable at 1.5 per use → 30;
/// - a summary `s` holds both: 42 + 100 + 8 + 3 + 30 = 183.
#[test]
fn f_cost_07_task_cost_and_roll_up() {
    let mut a = task("a", 5, &[], &[("dev", "2")]);
    a.parent = Some("s".into());
    a.fixed_cost = Some(FixedCost {
        amount: dec("100"),
        accrue: Accrue::Start,
        category: Some("investment".into()),
    });
    let mut b = task(
        "b",
        2,
        &["a"],
        &[("crane", "1"), ("mob", "1"), ("cable", "20")],
    );
    b.parent = Some("s".into());
    let p = plan(
        vec![Task::new("s", "Summary"), a, b],
        vec![
            resource("dev", Some(("4.2", RatePer::Day)), ResourceKind::Work),
            resource(
                "crane",
                Some(("0.5", RatePer::Hour)),
                ResourceKind::Equipment,
            ),
            resource("mob", Some(("3", RatePer::Use)), ResourceKind::Cost),
            resource("cable", Some(("1.5", RatePer::Use)), ResourceKind::Material),
        ],
    );
    let s = schedule(&p).unwrap();
    let c = costs(&p, &s);
    assert_eq!(c.task_total[1], dec("142"));
    assert_eq!(c.task_total[2], dec("41"));
    assert_eq!(c.task_total[0], dec("183"));
    assert_eq!(c.total, dec("183"));
    assert_eq!(
        c.by_resource,
        vec![dec("42"), dec("8"), dec("3"), dec("30")]
    );
    assert!(c.unpriced.is_empty());
}

/// F-COST-08: a time-rated cost spreads evenly over working time; a fixed cost accrues at its
/// start or end; per-use costs accrue at the start.
#[test]
fn f_cost_08_accrual_over_time() {
    let mut a = task("a", 10, &[], &[("dev", "1")]);
    a.fixed_cost = Some(FixedCost {
        amount: dec("50"),
        accrue: Accrue::End,
        category: Some("x".into()),
    });
    let p = plan(
        vec![a],
        vec![resource(
            "dev",
            Some(("8", RatePer::Day)),
            ResourceKind::Work,
        )],
    );
    let s = schedule(&p).unwrap();
    let c = costs(&p, &s);
    let cals = Calendars::new(&p);
    let week1 = (
        start_of(Date::constant(2027, 1, 4)),
        start_of(Date::constant(2027, 1, 11)),
    );
    let labor = &c.lines[0];
    assert!(matches!(labor.accrual, Accrual::Spread { .. }));
    assert_eq!(share(&p, &cals, labor, week1.0, week1.1), dec("40"));
    let fixed = &c.lines[1];
    assert_eq!(share(&p, &cals, fixed, week1.0, week1.1), Decimal::ZERO);
    let pts = curve(&p, &s, &c, Period::Week);
    assert_eq!(pts.len(), 2);
    assert_eq!((pts[0].cost, pts[1].cost), (dec("40"), dec("90")));
    assert_eq!(pts[1].cumulative, dec("130"));
}

/// A lag is waiting time: no cost accrues during it (knowledge notes, resources §1.7).
#[test]
fn lag_costs_nothing() {
    let mut b = task("b", 5, &[], &[("dev", "1")]);
    b.deps = vec![Dep {
        lag: parse_duration("5d", WorkUnits::default(), true).unwrap(),
        ..Dep::fs("a")
    }];
    let p = plan(
        vec![task("a", 5, &[], &[("dev", "1")]), b],
        vec![resource(
            "dev",
            Some(("1", RatePer::Day)),
            ResourceKind::Work,
        )],
    );
    let s = schedule(&p).unwrap();
    let c = costs(&p, &s);
    let pts = curve(&p, &s, &c, Period::Week);
    let per_week: Vec<Decimal> = pts.iter().map(|x| x.cost).collect();
    assert_eq!(per_week, vec![dec("5"), Decimal::ZERO, dec("5")]);
}

#[test]
fn l_res_05_unpriced_resource_and_l_cost_01_uncategorized_fixed_cost() {
    let mut a = task("a", 5, &[], &[("dev", "1")]);
    a.fixed_cost = Some(FixedCost {
        amount: dec("10"),
        accrue: Accrue::Start,
        category: None,
    });
    let p = plan(vec![a], vec![resource("dev", None, ResourceKind::Work)]);
    let s = schedule(&p).unwrap();
    assert_eq!(costs(&p, &s).unpriced, vec![0]);
    let codes: Vec<&str> = lint(&p, Some(&s))
        .iter()
        .map(|d| d.code)
        .filter(|c| c.starts_with("L-RES") || c.starts_with("L-COST"))
        .collect();
    assert_eq!(codes, vec!["L-RES-05", "L-COST-01"]);
}

fn arb_plan() -> impl Strategy<Value = Plan> {
    (1usize..12).prop_flat_map(|n| {
        let tasks = proptest::collection::vec((0i64..8, 0u8..4, 0u32..400, 0u8..3), n);
        let links = proptest::collection::vec((0usize..n, 0usize..n), 0..n);
        (tasks, links).prop_map(move |(spec, links)| {
            let rates = ["dev", "eq", "fee"];
            let mut tasks: Vec<Task> = spec
                .iter()
                .enumerate()
                .map(|(i, &(d, r, fixed, accrue))| {
                    let a: Vec<(&str, &str)> = match r {
                        0..=2 => vec![(rates[r as usize], "1.5")],
                        _ => Vec::new(),
                    };
                    let mut t = task(&format!("t{i}"), d, &[], &a);
                    if d == 0 {
                        t.kind = TaskKind::Milestone;
                        t.duration = None;
                        t.assign.clear();
                    }
                    if fixed > 0 {
                        t.fixed_cost = Some(FixedCost {
                            amount: Decimal::new(i64::from(fixed), 1),
                            accrue: [Accrue::Start, Accrue::Prorated, Accrue::End][accrue as usize],
                            category: Some("c".into()),
                        });
                    }
                    t
                })
                .collect();
            for (a, b) in links {
                let (a, b) = (a.min(b), a.max(b));
                if a != b {
                    tasks[b].deps.push(Dep::fs(format!("t{a}")));
                }
            }
            plan(
                tasks,
                vec![
                    resource("dev", Some(("4.2", RatePer::Day)), ResourceKind::Work),
                    resource("eq", Some(("0.7", RatePer::Month)), ResourceKind::Equipment),
                    resource("fee", Some(("3", RatePer::Use)), ResourceKind::Cost),
                ],
            )
        })
    })
}

proptest! {
    /// F-COST-12: the curve ends exactly at the total, in every period size, and never goes down.
    #[test]
    fn f_cost_12_curve_adds_up_to_the_total(p in arb_plan()) {
        let s = schedule(&p).unwrap();
        let c = costs(&p, &s);
        prop_assert_eq!(c.total, c.lines.iter().map(|l| l.amount).sum::<Decimal>());
        for period in [Period::Day, Period::Week, Period::Month] {
            let pts = curve(&p, &s, &c, period);
            let last = pts.last().map(|x| x.cumulative).unwrap_or_default();
            prop_assert_eq!(last, c.total, "{:?}", period);
            prop_assert!(pts.iter().all(|x| x.cost >= Decimal::ZERO));
        }
    }
}
