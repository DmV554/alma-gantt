//! Robustness: corrupted plan files never crash the toolchain, and random sequences of
//! commands keep the file valid, canonical and idempotent (SPEC.md §8a).

use std::collections::BTreeMap;

use alma_ops::mutate::{FieldValue, Op, apply, fmt_text};
use proptest::prelude::*;

const FIXTURE: &str = include_str!("../../../examples/hybrid-terminal.alma.toml");

/// One way of damaging a file, applied at a pseudo-random line.
#[derive(Debug, Clone)]
enum Damage {
    DeleteLine(usize),
    DuplicateLine(usize),
    SwapLines(usize, usize),
    TruncateLine(usize, usize),
    ReplaceChar(usize, usize, char),
    TruncateFile(usize),
}

fn damage(src: &str, d: &Damage) -> String {
    let mut lines: Vec<String> = src.lines().map(str::to_string).collect();
    let n = lines.len().max(1);
    match *d {
        Damage::DeleteLine(i) => {
            lines.remove(i % n);
        }
        Damage::DuplicateLine(i) => {
            let l = lines[i % n].clone();
            lines.insert(i % n, l);
        }
        Damage::SwapLines(a, b) => lines.swap(a % n, b % n),
        Damage::TruncateLine(i, k) => {
            let l = &mut lines[i % n];
            let cut = l
                .char_indices()
                .nth(k % (l.chars().count() + 1))
                .map_or(l.len(), |(p, _)| p);
            l.truncate(cut);
        }
        Damage::ReplaceChar(i, k, c) => {
            let l = &mut lines[i % n];
            let chars: Vec<char> = l.chars().collect();
            if !chars.is_empty() {
                let mut chars = chars;
                let at = k % chars.len();
                chars[at] = c;
                *l = chars.into_iter().collect();
            }
        }
        Damage::TruncateFile(k) => {
            let text = lines.join("\n");
            let cut = text
                .char_indices()
                .nth(k % (text.chars().count() + 1))
                .map_or(text.len(), |(p, _)| p);
            return text[..cut].to_string();
        }
    }
    lines.join("\n") + "\n"
}

fn arb_damage() -> impl Strategy<Value = Damage> {
    let c = prop_oneof![
        Just('"'),
        Just('='),
        Just('['),
        Just(']'),
        Just('{'),
        Just(','),
        Just('-'),
        Just('0'),
        Just('x'),
        Just(' '),
        Just('#'),
        Just('ñ'),
    ];
    prop_oneof![
        any::<usize>().prop_map(Damage::DeleteLine),
        any::<usize>().prop_map(Damage::DuplicateLine),
        (any::<usize>(), any::<usize>()).prop_map(|(a, b)| Damage::SwapLines(a, b)),
        (any::<usize>(), any::<usize>()).prop_map(|(a, b)| Damage::TruncateLine(a, b)),
        (any::<usize>(), any::<usize>(), c).prop_map(|(a, b, c)| Damage::ReplaceChar(a, b, c)),
        any::<usize>().prop_map(Damage::TruncateFile),
    ]
}

const IDS: &[&str] = &[
    "mgmt",
    "kickoff",
    "discovery",
    "decision-gate",
    "platform",
    "platform-env",
    "platform-cicd",
    "gate",
    "gate-sw",
    "gate-ocr-procure",
    "gate-ocr",
    "yard",
    "yard-sw",
    "yard-lte",
    "reefer",
    "reefer-sw",
    "reefer-sensors",
    "golive",
    "integration-tests",
    "h4",
    "uat",
    "pilot",
    "production",
    "stabilization",
    "finish",
    "new-a",
    "new-b",
];

fn id(i: usize) -> String {
    IDS[i % IDS.len()].to_string()
}

fn fields(pairs: &[(&str, &str)]) -> BTreeMap<String, FieldValue> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), FieldValue::Text(v.to_string())))
        .collect()
}

fn arb_op() -> impl Strategy<Value = Op> {
    let dur = prop_oneof![Just("0d"), Just("3d"), Just("2w"), Just("12h"), Just("5ed")];
    let kind = prop_oneof![Just("FS"), Just("SS"), Just("FF"), Just("SF")];
    let lag = prop_oneof![Just("0d"), Just("2d"), Just("-1d"), Just("1ed")];
    prop_oneof![
        (any::<usize>(), dur.clone()).prop_map(|(i, d)| Op::TaskSet {
            id: id(i),
            fields: fields(&[("duration", d)]),
            unset: Vec::new(),
        }),
        (any::<usize>(), any::<usize>(), dur, any::<bool>()).prop_map(|(i, p, d, top)| {
            Op::TaskAdd {
                id: id(i),
                name: "Added".into(),
                parent: (!top).then(|| id(p)),
                after: None,
                fields: fields(&[("duration", d)]),
                upsert: false,
            }
        }),
        (any::<usize>(), any::<usize>(), kind, lag).prop_map(|(a, b, k, l)| Op::DepAdd {
            pred: id(a),
            succ: id(b),
            kind: Some(k.into()),
            lag: Some(l.into()),
            nature: None,
            scope: None,
            rationale: Some("test".into()),
        }),
        (any::<usize>(), any::<usize>()).prop_map(|(a, b)| Op::DepRm {
            pred: id(a),
            succ: id(b)
        }),
        (any::<usize>(), any::<bool>()).prop_map(|(i, c)| Op::TaskRm {
            id: id(i),
            cascade: c
        }),
        (any::<usize>(), any::<usize>(), any::<bool>()).prop_map(|(i, p, top)| Op::TaskMove {
            id: id(i),
            parent: Some(if top { String::new() } else { id(p) }),
            after: None,
        }),
        (
            any::<usize>(),
            prop_oneof![Just("dev"), Just("qa"), Just("nobody")],
            prop_oneof![Just("1"), Just("0.5")]
        )
            .prop_map(|(i, r, u)| Op::Assign {
                task: id(i),
                resource: r.into(),
                units: Some(u.into())
            }),
        (any::<usize>(), prop_oneof![Just("dev"), Just("qa")]).prop_map(|(i, r)| Op::Unassign {
            task: id(i),
            resource: r.into()
        }),
        (0u32..20).prop_map(|d| Op::CalExcept {
            calendar: "cl-standard".into(),
            name: format!("Holiday {d}"),
            from: format!("2027-06-{:02}", d + 1),
            to: format!("2027-06-{:02}", d + 2),
            working: false,
        }),
        prop_oneof![Just("off"), Just("info"), Just("bogus")].prop_map(|s| Op::RuleSet {
            code: "L-DEP-02".into(),
            severity: s.into()
        }),
        (1u32..5).prop_map(|c| Op::CadenceAdd {
            id: "team-b".into(),
            start: "2027-05-03".into(),
            length: "2w".into(),
            count: c,
            name: None,
            prefix: Some("b".into()),
            calendar: None,
        }),
    ]
}

/// 64 cases keep CI fast; set `PROPTEST_CASES=2000` for a thorough run.
fn cases() -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(64)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(cases()))]

    #[test]
    fn damaged_files_never_crash(ds in proptest::collection::vec(arb_damage(), 1..4)) {
        let mut text = FIXTURE.to_string();
        for d in &ds {
            text = damage(&text, d);
        }
        let loaded = alma_ops::load(&text);
        if let (Some(plan), Some(s)) = (&loaded.plan, &loaded.schedule) {
            let _ = alma_ops::query::task_views(plan, s);
            let _ = alma_ops::export::svg(plan, s, &Default::default());
        } else {
            // A plan that does not load always says why, with a code.
            prop_assert!(loaded.diagnostics.iter().any(|d| d.code.starts_with("E-")));
        }
        let _ = fmt_text(&text);
    }

    #[test]
    fn random_commands_keep_the_file_valid_and_canonical(ops in proptest::collection::vec(arb_op(), 1..12)) {
        let mut text = FIXTURE.to_string();
        for op in &ops {
            let (out, report) = apply(&text, std::slice::from_ref(op), false);
            match out {
                Some(new) => {
                    prop_assert!(report.ok);
                    let loaded = alma_ops::load(&new);
                    let hard: Vec<_> = loaded.diagnostics.iter().filter(|d| d.code.starts_with("E-")).collect();
                    prop_assert!(hard.is_empty(), "{:?} left errors: {:?}", op, hard);
                    prop_assert_eq!(fmt_text(&new).unwrap(), new.clone(), "{:?} broke the canonical layout", op);
                    // Idempotent: running the same command again changes nothing (or is refused,
                    // for removals of something already gone).
                    if !matches!(op, Op::TaskRm { .. } | Op::DepRm { .. } | Op::Unassign { .. }) {
                        let (again, r2) = apply(&new, std::slice::from_ref(op), false);
                        prop_assert!(r2.ok, "{:?} not repeatable: {:?}", op, r2.errors);
                        prop_assert_eq!(again.as_deref(), Some(new.as_str()), "{:?} not idempotent", op);
                    }
                    text = new;
                }
                None => {
                    prop_assert!(!report.ok);
                    prop_assert!(report.errors.iter().all(|e| e.code.starts_with("E-")), "{:?}", report.errors);
                    prop_assert!(!report.errors.is_empty());
                }
            }
        }
    }
}
