//! Costs and their distribution over time (SPEC.md §7.5, F-COST-07, F-COST-08, F-COST-12,
//! L-RES-05, L-COST-01, ADR 0012).

use std::str::FromStr;

use alma_core::model::*;
use alma_core::time::start_of;
use alma_core::{Duration, WorkUnits, parse_duration};
use alma_sched::baseline::{snapshot, variance};
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
        baselines: Vec::new(),
        reserves: Vec::new(),
        risks: Vec::new(),
        funding: Vec::new(),
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

/// A baseline freezes dates, costs and planned value; variance compares the plan with it.
#[test]
fn f_cost_13_baseline_planned_value_and_variance() {
    let p = plan(
        vec![
            task("a", 5, &[], &[("dev", "1")]),
            task("b", 5, &["a"], &[("dev", "1")]),
        ],
        vec![resource(
            "dev",
            Some(("8", RatePer::Day)),
            ResourceKind::Work,
        )],
    );
    let s = schedule(&p).unwrap();
    let snap = snapshot("b1", &p, &s);
    assert_eq!(snap.bac, dec("80"));
    // 8 per working day, cumulative, no entries on the weekend.
    assert_eq!(snap.pv.len(), 10);
    assert_eq!(snap.pv_through(Date::constant(2027, 1, 8)), dec("40"));
    assert_eq!(
        snap.pv_through(Date::constant(2027, 1, 9)),
        dec("40"),
        "Saturday"
    );
    assert_eq!(snap.pv_through(Date::constant(2027, 1, 3)), Decimal::ZERO);
    assert_eq!(snap.pv_through(Date::constant(2027, 2, 1)), dec("80"));

    // `a` slips two days and gets more expensive; `c` is new.
    let mut q = p.clone();
    q.tasks[0].duration = Some(Duration::working(7 * DAY));
    q.tasks.push(task("c", 1, &["b"], &[]));
    let s2 = schedule(&q).unwrap();
    let (rows, removed) = variance(&q, &s2, &snap);
    assert!(removed.is_empty());
    assert_eq!((rows[0].start_var, rows[0].finish_var), (0, 2 * DAY));
    assert_eq!(rows[0].cost - rows[0].baseline_cost, dec("16"));
    assert_eq!((rows[1].start_var, rows[1].finish_var), (2 * DAY, 2 * DAY));
    assert!(rows[2].baseline.is_none());
    // Removing a task shows up as removed.
    let mut r = p.clone();
    r.tasks.pop();
    let (_, removed) = variance(&r, &schedule(&r).unwrap(), &snap);
    assert_eq!(removed, vec!["b".to_string()]);
}

/// F-COST-09/F-COST-10: contingency is inside the cost baseline and accrues at the finish of
/// its scope; management reserve is outside it, in the budget only.
#[test]
fn f_cost_09_10_reserves_and_the_cost_stack() {
    let mut p = plan(
        vec![
            task("a", 5, &[], &[("dev", "1")]),
            task("b", 5, &["a"], &[("dev", "1")]),
        ],
        vec![resource(
            "dev",
            Some(("8", RatePer::Day)),
            ResourceKind::Work,
        )],
    );
    let reserve = |id: &str, kind, amount: &str, scope: Option<&str>| CostReserve {
        id: id.into(),
        kind,
        amount: dec(amount),
        scope: scope.map(str::to_string),
        basis: Some("EMV".into()),
        risks: Vec::new(),
    };
    p.reserves = vec![
        reserve("c-a", ReserveKind::Contingency, "10", Some("a")),
        reserve("c-p", ReserveKind::Contingency, "5", None),
        reserve("mr", ReserveKind::Management, "30", None),
    ];
    let s = schedule(&p).unwrap();
    let c = costs(&p, &s);
    assert_eq!(c.total, dec("95"), "80 of work + 15 of contingency");
    assert_eq!((c.contingency, c.management), (dec("15"), dec("30")));
    assert_eq!(c.task_total[0], dec("50"), "a's contingency rolls into a");
    // Contingency accrues at the finish of its scope: a's on Friday of week 1, the project's
    // at the finish (Friday of week 2).
    let pts = curve(&p, &s, &c, Period::Week);
    assert_eq!((pts[0].cost, pts[1].cost), (dec("50"), dec("45")));
    let snap = snapshot("b", &p, &s);
    assert_eq!(
        snap.bac,
        dec("95"),
        "BAC includes contingency, not management reserve"
    );
    let lint_codes: Vec<&str> = lint(&p, Some(&s)).iter().map(|d| d.code).collect();
    assert!(!lint_codes.contains(&"L-RISK-01"));
    p.reserves[0].basis = None;
    p.reserves[2].basis = None;
    let flagged: Vec<String> = lint(&p, Some(&s))
        .iter()
        .filter(|d| d.code == "L-RISK-01")
        .map(|d| d.ids[0].clone())
        .collect();
    assert_eq!(flagged, vec!["c-a"], "only contingency needs a basis");
}

fn risk(id: &str, p: &str, impact: &str, sign: RiskSign, group: Option<&str>) -> Risk {
    Risk {
        id: id.into(),
        name: id.into(),
        probability: dec(p),
        impact: Some(dec(impact)),
        impact_time: Some(Duration::working(10 * DAY)),
        sign,
        status: RiskStatus::Open,
        owner: None,
        affects: Vec::new(),
        correlation_group: group.map(str::to_string),
    }
}

/// F-COST-37: EMV = p × impact, negative for opportunities; contingency is checked against the
/// EMV of the risks it covers (L-RISK-02), correlated risks are flagged (L-RISK-03), and a
/// contingency whose risks are all closed should be released (L-RISK-05).
#[test]
fn f_cost_37_emv_and_risk_rules() {
    use alma_sched::finance::{covered_emv, emv, emv_time};
    let threat = risk("r1", "0.3", "200", RiskSign::Threat, Some("supplier"));
    let opp = risk("r2", "0.5", "40", RiskSign::Opportunity, None);
    assert_eq!(emv(&threat), dec("60"));
    assert_eq!(emv(&opp), dec("-20"));
    assert_eq!(emv_time(&threat), 3 * DAY);
    let mut p = plan(
        vec![task("a", 5, &[], &[])],
        vec![resource(
            "dev",
            Some(("8", RatePer::Day)),
            ResourceKind::Work,
        )],
    );
    p.risks = vec![
        threat,
        risk("r3", "0.2", "150", RiskSign::Threat, Some("supplier")),
        opp,
    ];
    assert_eq!(covered_emv(&p, &["r1".into(), "r3".into()]), dec("90"));
    p.reserves = vec![CostReserve {
        id: "c".into(),
        kind: ReserveKind::Contingency,
        amount: dec("95"),
        scope: None,
        basis: None,
        risks: vec!["r1".into(), "r3".into()],
    }];
    let codes = |p: &Plan| -> Vec<&'static str> {
        let s = schedule(p).unwrap();
        lint(p, Some(&s))
            .into_iter()
            .map(|d| d.code)
            .filter(|c| c.starts_with("L-RISK"))
            .collect()
    };
    // 95 vs 90 is within 10%; linked risks count as a basis; the group is shared.
    assert_eq!(codes(&p), vec!["L-RISK-03"]);
    p.reserves[0].amount = dec("150");
    assert_eq!(codes(&p), vec!["L-RISK-02", "L-RISK-03"]);
    for r in &mut p.risks {
        r.status = RiskStatus::Closed;
    }
    assert_eq!(codes(&p), vec!["L-RISK-05"]);
}

/// F-COST-11: cumulative spend must stay within cumulative funding (L-COST-04, a warning in
/// draft plans and an error once preliminary or baselined).
#[test]
fn f_cost_11_funding_gap() {
    use alma_sched::finance::funding_gap;
    // 8 per working day for 10 days: 40 by Friday of week 1, 80 by Friday of week 2.
    let mut p = plan(
        vec![task("a", 10, &[], &[("dev", "1")])],
        vec![resource(
            "dev",
            Some(("8", RatePer::Day)),
            ResourceKind::Work,
        )],
    );
    let fund = |id: &str, d: Date, amount: &str| Funding {
        id: id.into(),
        date: d,
        amount: dec(amount),
        source: None,
    };
    p.funding = vec![
        fund("f1", Date::constant(2027, 1, 4), "40"),
        fund("f2", Date::constant(2027, 1, 13), "40"),
    ];
    let s = schedule(&p).unwrap();
    let g = funding_gap(&p, &costs(&p, &s)).unwrap();
    assert_eq!(
        (g.date, g.spend, g.funded),
        (Date::constant(2027, 1, 11), dec("48"), dec("40"))
    );
    let sev = |p: &Plan| {
        lint(p, Some(&schedule(p).unwrap()))
            .into_iter()
            .find(|d| d.code == "L-COST-04")
            .map(|d| d.severity)
    };
    assert_eq!(sev(&p), Some(alma_core::diag::Severity::Warn));
    p.settings.phase = Phase::Preliminary;
    assert_eq!(sev(&p), Some(alma_core::diag::Severity::Error));
    p.funding[1].date = Date::constant(2027, 1, 11);
    assert_eq!(funding_gap(&p, &costs(&p, &s)), None);
    assert_eq!(sev(&p), None);
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
        // A snapshot's planned value ends at its budget and never goes down.
        let snap = snapshot("b", &p, &s);
        prop_assert_eq!(snap.pv.last().map(|x| x.1).unwrap_or_default(), c.total);
        prop_assert!(snap.pv.windows(2).all(|w| w[0].0 < w[1].0 && w[0].1 <= w[1].1));
        // Against itself, nothing varies.
        let (rows, removed) = variance(&p, &s, &snap);
        prop_assert!(removed.is_empty());
        prop_assert!(rows.iter().all(|r| r.start_var == 0 && r.finish_var == 0 && r.cost == r.baseline_cost));
        // The file round-trips exactly.
        let text = alma_format::snapshot::to_text(&snap);
        prop_assert_eq!(alma_format::snapshot::from_text(&text).unwrap(), snap);
    }
}
