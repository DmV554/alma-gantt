//! Parser tests: the fixture, every `E-*` code, and `docs/format.md` against the key registry.

use std::path::PathBuf;

use alma_core::model::{DepType, TaskKind};
use alma_format::keys::{KEYS, Tier};
use alma_format::parse;

fn repo(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(path)
}

fn read(path: &str) -> String {
    std::fs::read_to_string(repo(path)).unwrap()
}

const HEAD: &str = "[project]\nformat = 0\nname = \"t\"\nstart = 2027-01-04\n";

/// Codes reported for `HEAD` + `body`.
fn codes(body: &str) -> Vec<&'static str> {
    parse(&format!("{HEAD}{body}"))
        .diagnostics
        .iter()
        .map(|d| d.code)
        .collect()
}

#[test]
fn fixture_parses_without_diagnostics() {
    let parsed = parse(&read("examples/hybrid-terminal.alma.toml"));
    assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
    let plan = parsed.plan.unwrap();
    assert_eq!(plan.tasks.len(), 25);
    assert_eq!(plan.calendars[0].day_minutes(), 480);
    assert_eq!(plan.cadences[0].iteration_ids()[0], "s01");
    let h4 = plan.tasks.iter().find(|t| t.id == "h4").unwrap();
    assert_eq!(h4.kind, TaskKind::Milestone);
    let reefer = plan.tasks.iter().find(|t| t.id == "reefer-sw").unwrap();
    assert_eq!(reefer.deps[0].kind, DepType::Fs);
}

#[test]
fn format_doc_examples_parse() {
    let doc = read("docs/format.md");
    let mut blocks = 0;
    for block in doc.split("```toml\n").skip(1) {
        let body = &block[..block.find("```").unwrap()];
        if !body.contains("[project]") {
            continue;
        }
        blocks += 1;
        let parsed = parse(body);
        assert!(
            parsed.diagnostics.is_empty(),
            "{body}\n{:#?}",
            parsed.diagnostics
        );
    }
    assert!(blocks >= 1);
}

#[test]
fn e_fmt_01_syntax() {
    assert_eq!(codes("[[task]\n"), ["E-FMT-01"]);
}

#[test]
fn e_fmt_02_unknown_key_with_suggestion() {
    let parsed = parse(&format!(
        "{HEAD}[[task]]\nid = \"a\"\nname = \"A\"\nduraton = \"1d\"\n"
    ));
    let d = &parsed.diagnostics[0];
    assert_eq!(d.code, "E-FMT-02");
    assert_eq!(d.hint.as_deref(), Some("did you mean `duration`?"));
    assert_eq!(d.span.unwrap().line, 8);
    assert!(parsed.plan.is_none());
}

#[test]
fn e_fmt_03_wrong_type_and_combinations() {
    assert_eq!(codes("[[task]]\nid = \"a\"\nname = 3\n"), ["E-FMT-03"]);
    assert_eq!(
        codes("[[task]]\nid = \"a\"\nname = \"A\"\nfixed_cost = { amount = 4.2 }\n"),
        ["E-FMT-03"],
        "money must be a string"
    );
    assert_eq!(
        codes("[[task]]\nid = \"a\"\nname = \"A\"\nclass = \"internal\"\n"),
        ["E-FMT-03"],
        "class only on milestones"
    );
    assert_eq!(
        codes(
            "[[task]]\nid = \"p\"\nname = \"P\"\nkind = \"task\"\n\
             [[task]]\nid = \"c\"\nparent = \"p\"\nname = \"C\"\n"
        ),
        ["E-FMT-03"],
        "kind on a summary"
    );
}

#[test]
fn e_fmt_04_bad_duration() {
    assert_eq!(
        codes("[[task]]\nid = \"a\"\nname = \"A\"\nduration = \"3mo\"\n"),
        ["E-FMT-04"]
    );
    assert_eq!(
        codes("[[task]]\nid = \"a\"\nname = \"A\"\nduration = \"-1d\"\n"),
        ["E-FMT-04"]
    );
}

#[test]
fn e_fmt_05_reserved_key() {
    let parsed = parse(&format!(
        "{HEAD}[[task]]\nid = \"a\"\nname = \"A\"\nactual_start = 2027-01-04\n"
    ));
    assert_eq!(parsed.diagnostics[0].code, "E-FMT-05");
    assert!(parsed.diagnostics[0].message.contains("M4"));
    assert_eq!(codes("[[risk]]\nid = \"r\"\n"), ["E-FMT-05"]);
}

#[test]
fn e_fmt_06_format_version() {
    let parsed = parse("[project]\nname = \"t\"\nstart = 2027-01-04\n");
    assert_eq!(parsed.diagnostics[0].code, "E-FMT-06");
    let parsed = parse("[project]\nformat = 9\nname = \"t\"\nstart = 2027-01-04\n");
    assert_eq!(parsed.diagnostics[0].code, "E-FMT-06");
}

#[test]
fn e_ref_01_duplicate_id_across_types() {
    assert_eq!(
        codes("[[resource]]\nid = \"dev\"\n[[task]]\nid = \"dev\"\nname = \"Dev\"\n"),
        ["E-REF-01"]
    );
}

#[test]
fn e_ref_02_bad_id() {
    assert_eq!(
        codes("[[task]]\nid = \"Gate_A\"\nname = \"A\"\n"),
        ["E-REF-02"]
    );
}

#[test]
fn e_ref_03_unknown_reference() {
    assert_eq!(
        codes("[[task]]\nid = \"a\"\nname = \"A\"\ndeps = [{ pred = \"zz\" }]\n"),
        ["E-REF-03"]
    );
}

#[test]
fn e_ref_04_parent_cycle() {
    assert_eq!(
        codes(
            "[[task]]\nid = \"a\"\nparent = \"b\"\nname = \"A\"\n\
             [[task]]\nid = \"b\"\nparent = \"a\"\nname = \"B\"\n"
        ),
        ["E-REF-04"]
    );
}

#[test]
fn e_cal_01_no_working_time() {
    assert_eq!(
        codes("[[calendar]]\nid = \"c\"\nworkdays = []\n"),
        ["E-CAL-01"]
    );
}

#[test]
fn e_cal_02_bad_hours() {
    assert_eq!(
        codes("[[calendar]]\nid = \"c\"\nhours = \"18:00-09:00\"\n"),
        ["E-CAL-02"]
    );
    assert_eq!(
        codes("[[calendar]]\nid = \"c\"\nhours = [\"09:00-13:00\", \"12:00-18:00\"]\n"),
        ["E-CAL-02"]
    );
}

#[test]
fn collects_every_error_in_one_pass() {
    let c = codes(
        "[[task]]\nid = \"a\"\nname = \"A\"\nduraton = \"1d\"\n\
         [[task]]\nid = \"b\"\nname = \"B\"\nduration = \"3mo\"\n",
    );
    assert_eq!(c, ["E-FMT-02", "E-FMT-04"]);
}

/// Tables of `docs/format.md`, by heading, and the registry table they describe.
const DOC_TABLES: &[(&str, &str)] = &[
    ("### `[project]`", "project"),
    ("### `[settings.schedule]`", "settings.schedule"),
    ("### 6.1 `[[calendar]]`", "calendar"),
    ("### 6.2 `[[task]]`", "task"),
    ("### 6.3 `[[task]]`", "task"),
    ("### 6.4 Dependencies", "task.deps"),
    ("### 6.5 `[[task]]`", "task"),
    ("### 6.6 `[[resource]]`", "resource"),
    ("### 6.7 `[[cadence]]`", "cadence"),
];

fn backticked(cell: &str) -> Vec<String> {
    cell.split('`')
        .skip(1)
        .step_by(2)
        .filter(|w| w.chars().all(|c| c.is_ascii_lowercase() || c == '_'))
        .map(str::to_string)
        .collect()
}

#[test]
fn format_doc_matches_key_registry() {
    let doc = read("docs/format.md");
    let mut documented: Vec<(String, String, char)> = Vec::new();
    let sections: Vec<&str> = doc.split("\n### ").collect();
    for (heading, table) in DOC_TABLES {
        let title = heading.trim_start_matches("### ");
        let section = sections
            .iter()
            .find(|s| s.starts_with(title))
            .unwrap_or_else(|| panic!("section {heading} not found"));
        let mut header: Vec<String> = Vec::new();
        for line in section.lines() {
            if line.starts_with('|') {
                let cells: Vec<String> = line
                    .trim_matches('|')
                    .split('|')
                    .map(|c| c.trim().to_string())
                    .collect();
                if header.is_empty() {
                    header = cells;
                    continue;
                }
                if cells[0].starts_with("---") {
                    continue;
                }
                let key_col = header.iter().position(|h| h == "Key").unwrap();
                let tier = header
                    .iter()
                    .position(|h| h == "Tier")
                    .map(|i| cells[i].chars().next().unwrap())
                    .unwrap_or(if *table == "resource" || *table == "cadence" {
                        'B'
                    } else {
                        'A'
                    });
                for key in backticked(&cells[key_col]) {
                    documented.push((table.to_string(), key, tier));
                }
            } else if line.starts_with("Reserved") {
                // "Reserved on tasks (R): `a`, `b` …" lines list reserved keys in prose.
                for key in backticked(line) {
                    documented.push((table.to_string(), key, 'R'));
                }
            }
        }
    }
    for (table, key, tier) in &documented {
        let def = KEYS
            .iter()
            .find(|k| {
                (k.table == table || k.table.starts_with(&format!("{table}."))) && k.key == key
            })
            .unwrap_or_else(|| {
                panic!("docs/format.md lists `{key}` in {table}, the registry does not")
            });
        let reg = match def.tier {
            Tier::A => 'A',
            Tier::B => 'B',
            Tier::R(_) => 'R',
        };
        assert_eq!(reg, *tier, "tier of {table}.{key}");
    }
    for table in [
        "project",
        "settings.schedule",
        "calendar",
        "task",
        "task.deps",
        "resource",
        "cadence",
    ] {
        for def in KEYS
            .iter()
            .filter(|k| k.table == table && !matches!(k.tier, Tier::R(_)))
        {
            assert!(
                documented
                    .iter()
                    .any(|(t, k, _)| t == table && k == def.key),
                "registry key {table}.{} is not documented in docs/format.md",
                def.key
            );
        }
    }
}

#[test]
fn round_trip_is_byte_identical() {
    {
        let path = "examples/hybrid-terminal.alma.toml";
        let src = read(path);
        let doc: toml_edit::DocumentMut = src.parse().unwrap();
        assert_eq!(doc.to_string(), src, "{path}");
    }
}
