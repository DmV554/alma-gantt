//! The agent contract (SPEC.md §8a), tested through the real binary: JSON output, dry run,
//! atomicity, idempotency, pagination, line-local edits, batches and exit codes.

use std::path::PathBuf;
use std::process::{Command, Output};

use serde_json::Value;

const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../examples/hybrid-terminal.alma.toml"
);

/// A copy of the fixture in a fresh temporary directory.
struct Plan {
    dir: PathBuf,
    file: PathBuf,
}

impl Plan {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("alma-contract-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("plan.alma.toml");
        std::fs::copy(FIXTURE, &file).unwrap();
        Self { dir, file }
    }

    fn text(&self) -> String {
        std::fs::read_to_string(&self.file).unwrap()
    }

    fn alma(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_alma"))
            .arg("--file")
            .arg(&self.file)
            .args(args)
            .current_dir(&self.dir)
            .output()
            .unwrap()
    }

    fn json(&self, args: &[&str]) -> (i32, Value) {
        let mut a = args.to_vec();
        a.push("--json");
        let out = self.alma(&a);
        let v: Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
            panic!(
                "{args:?}: not JSON ({e}): {}",
                String::from_utf8_lossy(&out.stdout)
            )
        });
        (out.status.code().unwrap(), v)
    }
}

impl Drop for Plan {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn changed_lines(a: &str, b: &str) -> usize {
    let (a, b): (Vec<&str>, Vec<&str>) = (a.lines().collect(), b.lines().collect());
    // Lines only in one side, by a simple LCS-free count (edits here are local).
    let common_prefix = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    let common_suffix = a[common_prefix..]
        .iter()
        .rev()
        .zip(b[common_prefix..].iter().rev())
        .take_while(|(x, y)| x == y)
        .count();
    (a.len() - common_prefix - common_suffix).max(b.len() - common_prefix - common_suffix)
}

#[test]
fn json_output_on_every_query() {
    let p = Plan::new("json");
    let (code, v) = p.json(&["check"]);
    assert_eq!(code, 0);
    assert_eq!(v["ok"], true);
    assert_eq!(v["counts"]["error"], 0);
    assert_eq!(v["counts"]["warn"], 0);
    // The fixture is leveled: gate software waits for the developers yard software holds.
    let (_, v) = p.json(&["show", "gate-sw"]);
    assert_eq!(v["start"], "2027-05-17");
    assert_eq!(v["finish"], "2027-07-09");
    assert_eq!(v["tf"], 0.0);
    let (_, v) = p.json(&["critical"]);
    assert_eq!(v["critical"][0]["id"], "kickoff");
    let (_, v) = p.json(&["explain", "uat"]);
    assert_eq!(v["chain"][0]["driven_by"], "h4");
    let (_, v) = p.json(&["ls", "--fields", "id,tf", "--critical"]);
    assert!(
        v["tasks"]
            .as_array()
            .unwrap()
            .iter()
            .all(|t| t["tf"] == 0.0)
    );
}

#[test]
fn ls_paginates() {
    let p = Plan::new("page");
    let (_, first) = p.json(&["ls", "--limit", "10", "--fields", "id"]);
    assert_eq!(first["tasks"].as_array().unwrap().len(), 10);
    assert_eq!(first["total"], 25);
    let cursor = first["next_cursor"].as_str().unwrap().to_string();
    let (_, second) = p.json(&["ls", "--limit", "10", "--cursor", &cursor, "--fields", "id"]);
    assert_ne!(first["tasks"][0]["id"], second["tasks"][0]["id"]);
    let (_, last) = p.json(&["ls", "--limit", "10", "--cursor", "20", "--fields", "id"]);
    assert_eq!(last["tasks"].as_array().unwrap().len(), 5);
    assert!(last.get("next_cursor").is_none());
}

#[test]
fn dry_run_writes_nothing() {
    let p = Plan::new("dry");
    let before = p.text();
    let (code, v) = p.json(&["task", "set", "yard-sw", "--duration", "55d", "--dry-run"]);
    assert_eq!(code, 0);
    assert_eq!(v["dry_run"], true);
    assert_eq!(v["end_date_delta"], "+5d");
    assert_eq!(p.text(), before);
    assert!(!p.dir.join(".alma").exists(), "no log entry for a dry run");
}

#[test]
fn edits_change_only_their_lines() {
    let p = Plan::new("local");
    let before = p.text();
    let (code, v) = p.json(&["task", "set", "yard-sw", "--duration", "55d"]);
    assert_eq!(code, 0, "{v}");
    let after = p.text();
    assert_eq!(changed_lines(&before, &after), 1);
    assert!(after.contains("duration = \"55d\""));
    // Comments survive.
    assert!(after.contains("# --- Yard subproject"));
    // Longer yard software holds the developers longer, so leveled gate software moves too.
    let moved: Vec<&str> = v["changed"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["id"].as_str().unwrap())
        .collect();
    assert!(
        moved.contains(&"yard-sw") && moved.contains(&"gate-sw"),
        "{v}"
    );
    assert!(p.dir.join(".alma/log.jsonl").exists());
}

#[test]
fn failed_commands_leave_the_file_byte_identical() {
    let p = Plan::new("atomic");
    let before = p.text();
    for args in [
        vec!["dep", "add", "h4", "discovery"],               // cycle
        vec!["task", "set", "nope", "--duration", "1d"],     // unknown task
        vec!["task", "set", "uat", "--field", "duraton=1d"], // unknown key
        vec!["task", "set", "uat", "--duration", "3mo"],     // bad duration
        vec!["task", "rm", "h4"],                            // has dependents
    ] {
        let (code, v) = p.json(&args);
        assert_eq!(code, 1, "{args:?}: {v}");
        assert_eq!(v["ok"], false);
        assert!(
            v["errors"][0]["code"].as_str().unwrap().starts_with("E-"),
            "{v}"
        );
        assert_eq!(p.text(), before, "{args:?} changed the file");
    }
    let (_, v) = p.json(&["task", "set", "uat", "--field", "duraton=1d"]);
    assert_eq!(v["errors"][0]["hint"], "did you mean `duration`?");
}

#[test]
fn adds_are_idempotent() {
    let p = Plan::new("idem");
    let args = [
        "task",
        "add",
        "qa-plan",
        "--name",
        "QA plan",
        "--parent",
        "golive",
        "--after",
        "integration-tests",
        "--duration",
        "3d",
    ];
    let (code, v) = p.json(&args);
    assert_eq!(code, 0, "{v}");
    assert_eq!(v["noop"], false);
    let once = p.text();
    let (code, v) = p.json(&args);
    assert_eq!(code, 0);
    assert_eq!(v["noop"], true);
    assert_eq!(p.text(), once);
    // A different definition is an error unless --upsert.
    let mut other = args.to_vec();
    other[10] = "4d";
    let (code, v) = p.json(&other);
    assert_eq!(
        (code, v["errors"][0]["code"].as_str()),
        (1, Some("E-OPS-01"))
    );
    other.push("--upsert");
    let (code, _) = p.json(&other);
    assert_eq!(code, 0);
    assert!(p.text().contains("duration = \"4d\""));
    // dep add of the same link twice.
    p.json(&["dep", "add", "integration-tests", "qa-plan"]);
    let (_, v) = p.json(&["dep", "add", "integration-tests", "qa-plan"]);
    assert_eq!(v["noop"], true);
}

#[test]
fn batches_are_all_or_nothing() {
    let p = Plan::new("batch");
    let before = p.text();
    let ops = p.dir.join("ops.json");
    std::fs::write(
        &ops,
        r#"[
            {"op": "task.add", "id": "docs", "name": "Documentation", "parent": "golive", "fields": {"duration": "5d"}},
            {"op": "dep.add", "pred": "uat", "succ": "docs"},
            {"op": "dep.add", "pred": "docs", "succ": "missing"}
        ]"#,
    )
    .unwrap();
    let (code, v) = p.json(&["apply", ops.to_str().unwrap()]);
    assert_eq!(code, 1);
    assert!(
        v["errors"][0]["message"]
            .as_str()
            .unwrap()
            .starts_with("operation #3")
    );
    assert_eq!(p.text(), before);

    std::fs::write(
        &ops,
        "{\"op\": \"task.add\", \"id\": \"docs\", \"name\": \"Documentation\", \"parent\": \"golive\", \"fields\": {\"duration\": \"5d\"}}\n\
         {\"op\": \"dep.add\", \"pred\": \"uat\", \"succ\": \"docs\"}\n\
         {\"op\": \"dep.add\", \"pred\": \"docs\", \"succ\": \"finish\"}\n",
    )
    .unwrap();
    let (code, v) = p.json(&["apply", ops.to_str().unwrap()]);
    assert_eq!(code, 0, "{v}");
    assert!(p.text().contains("id = \"docs\""));
}

#[test]
fn exit_codes() {
    let p = Plan::new("exit");
    assert_eq!(p.alma(&["check"]).status.code(), Some(0));
    assert_eq!(p.alma(&["frobnicate"]).status.code(), Some(2));
    let missing = Command::new(env!("CARGO_BIN_EXE_alma"))
        .args(["--file", "/nonexistent/plan.alma.toml", "check"])
        .output()
        .unwrap();
    assert_eq!(missing.status.code(), Some(2));
    std::fs::write(&p.file, "[project]\nformat = 0\nname = \"x\"\n").unwrap();
    assert_eq!(p.alma(&["check"]).status.code(), Some(1));
}

#[test]
fn init_creates_a_valid_plan_and_an_agent_can_build_on_it() {
    let dir = std::env::temp_dir().join(format!("alma-contract-{}-init", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_alma"))
            .args(args)
            .current_dir(&dir)
            .output()
            .unwrap()
    };
    let out = run(&[
        "init",
        "new.alma.toml",
        "--name",
        "New",
        "--start",
        "2027-01-04",
    ]);
    assert!(out.status.success());
    // The file is found without --file because it is the only plan in the directory.
    for args in [
        vec![
            "task",
            "add",
            "start",
            "--name",
            "Start",
            "--kind",
            "milestone",
        ],
        vec!["task", "add", "work", "--name", "Work", "--duration", "5d"],
        vec![
            "task",
            "add",
            "done",
            "--name",
            "Done",
            "--kind",
            "milestone",
        ],
        vec!["dep", "add", "start", "work"],
        vec!["dep", "add", "work", "done"],
    ] {
        assert!(run(&args).status.success(), "{args:?}");
    }
    let out = run(&["show", "done", "--json"]);
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["start"], "2027-01-08");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn guide_and_schema_print() {
    let p = Plan::new("guide");
    let out = p.alma(&["guide"]);
    assert!(String::from_utf8_lossy(&out.stdout).contains("Recommended loop"));
    let out = p.alma(&["schema"]);
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["type"], "array");
}

#[test]
fn export_writes_files_and_reports_json() {
    let p = Plan::new("export");
    let out = p.dir.join("gantt.svg");
    let (code, v) = p.json(&["export", out.to_str().unwrap()]);
    assert_eq!(code, 0, "{v}");
    assert_eq!(v["format"], "svg");
    assert!(std::fs::read_to_string(&out).unwrap().starts_with("<svg"));
    let png = p.dir.join("gantt.png");
    let (code, _) = p.json(&[
        "render",
        png.to_str().unwrap(),
        "--range",
        "2027-01..2027-03",
    ]);
    assert_eq!(code, 0);
    assert!(std::fs::metadata(&png).unwrap().len() > 1000);
    let csv = p.alma(&["export", "-", "--format", "csv"]);
    assert!(String::from_utf8_lossy(&csv.stdout).starts_with("wbs,id,name"));
    let (code, _) = p.json(&["export", "x.pdf"]);
    assert_eq!(code, 2);
}

#[test]
fn fmt_check_and_rewrite() {
    let p = Plan::new("fmt");
    let (code, v) = p.json(&["fmt", "--check"]);
    assert_eq!((code, v["changed"].clone()), (0, Value::Bool(false)));
    // Swap two keys of the first task so the file is no longer canonical.
    let text = p.text().replacen(
        "id = \"mgmt\"\nname = \"Project management\"",
        "name = \"Project management\"\nid = \"mgmt\"",
        1,
    );
    std::fs::write(&p.file, &text).unwrap();
    let (code, v) = p.json(&["fmt", "--check"]);
    assert_eq!((code, v["changed"].clone()), (1, Value::Bool(true)));
    assert_eq!(p.text(), text, "--check writes nothing");
    let (code, _) = p.json(&["fmt"]);
    assert_eq!(code, 0);
    assert_eq!(p.text(), std::fs::read_to_string(FIXTURE).unwrap());
}

#[test]
fn diff_against_a_file() {
    let p = Plan::new("diff");
    let old = p.dir.join("old.alma.toml.bak");
    std::fs::copy(&p.file, &old).unwrap();
    p.json(&["task", "set", "uat", "--duration", "12d"]);
    let (code, v) = p.json(&["diff", "--against", old.to_str().unwrap()]);
    assert_eq!(code, 0);
    assert_eq!(v["end_date_delta"], "+2d");
    assert!(
        v["changed"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["id"] == "uat")
    );
}

#[test]
fn iter_add_and_rm() {
    let p = Plan::new("iter");
    let (code, v) = p.json(&[
        "iter",
        "add",
        "team-b",
        "--prefix",
        "b",
        "--start",
        "2027-04-05",
        "--length",
        "2w",
        "--repeat",
        "4",
    ]);
    assert_eq!(code, 0, "{v}");
    let text = p.text();
    assert!(text.find("id = \"team-b\"").unwrap() < text.find("[[task]]").unwrap());
    // Generated ids join the global namespace: a task may not reuse one.
    let (code, v) = p.json(&["task", "add", "b01", "--name", "Clash"]);
    assert_eq!(
        (code, v["errors"][0]["code"].as_str()),
        (1, Some("E-REF-01"))
    );
    let (code, _) = p.json(&["cadence", "rm", "team-b"]);
    assert_eq!(code, 0);
    assert_eq!(p.text(), std::fs::read_to_string(FIXTURE).unwrap());
}

#[test]
fn three_point_estimates_from_the_cli() {
    let p = Plan::new("estimate");
    let (code, v) = p.json(&[
        "task",
        "set",
        "yard-lte",
        "--estimate",
        "20d/25d/35d",
        "--basis",
        "Supplier quote",
    ]);
    assert_eq!(code, 0, "{v}");
    assert!(
        p.text().contains(
            r#"estimate = { o = "20d", m = "25d", p = "35d", basis = "Supplier quote" }"#
        )
    );
    let (code, v) = p.json(&["task", "set", "yard-lte", "--estimate", "20d/25d"]);
    assert_eq!(
        (code, v["errors"][0]["code"].as_str()),
        (1, Some("E-OPS-04"))
    );
    let (code, v) = p.json(&["task", "set", "yard-lte", "--field", "estimate.bases=x"]);
    assert_eq!(
        (code, v["errors"][0]["code"].as_str()),
        (1, Some("E-FMT-02"))
    );
}

#[test]
fn calendars_holidays_and_project_settings() {
    let p = Plan::new("calendar");
    // A holiday on the critical path moves the finish by its working days.
    let (code, v) = p.json(&[
        "cal",
        "except",
        "cl-standard",
        "--name",
        "Fiestas Patrias",
        "--from",
        "2027-06-21",
        "--to",
        "2027-06-22",
    ]);
    assert_eq!(code, 0, "{v}");
    assert_eq!(v["end_date_delta"], "+2d");
    let text = p.text();
    assert!(
        text.contains("working = false },\n  { name = \"Fiestas Patrias\""),
        "one per line:\n{text}"
    );
    // The holiday falls inside gate software (leveled), so it shifts integrated testing.
    let (_, v) = p.json(&["explain", "pilot"]);
    assert!(
        v["chain"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["id"] == "integration-tests"
                && s["because"].as_str().unwrap().contains("Fiestas Patrias")),
        "{v}"
    );
    // A second calendar, then make it the project default.
    let (code, _) = p.json(&[
        "cal",
        "add",
        "six-day",
        "--workdays",
        "mon,tue,wed,thu,fri,sat",
    ]);
    assert_eq!(code, 0);
    let (code, v) = p.json(&["project", "set", "--calendar", "six-day"]);
    assert_eq!(code, 0, "{v}");
    assert!(
        v["end_date_delta"].as_str().unwrap().starts_with('-'),
        "{v}"
    );
    let (code, v) = p.json(&["project", "set", "--calendar", "missing"]);
    assert_eq!(
        (code, v["errors"][0]["code"].as_str()),
        (1, Some("E-REF-03"))
    );
}

#[test]
fn rule_set_and_keys() {
    let p = Plan::new("rules");
    let (_, before) = p.json(&["check"]);
    let (code, _) = p.json(&["rule", "set", "L-WBS-07", "off"]);
    assert_eq!(code, 0);
    let (_, after) = p.json(&["check"]);
    assert!(after["counts"]["info"].as_u64() < before["counts"]["info"].as_u64());
    let (code, v) = p.json(&["rule", "set", "E-DEP-01", "off"]);
    assert_eq!(
        (code, v["errors"][0]["code"].as_str()),
        (1, Some("E-FMT-03"))
    );
    let (_, v) = p.json(&["keys", "task"]);
    let keys = v["keys"].as_array().unwrap();
    assert!(
        keys.iter()
            .any(|k| k["key"] == "duration" && k["status"] == "active")
    );
    assert!(
        keys.iter()
            .any(|k| k["key"] == "iteration" && k["status"] == "reserved for M4")
    );
    let (_, v) = p.json(&["critical"]);
    assert_eq!(v["project_finish"], "2027-09-24");
}

#[test]
fn moved_tasks_render_under_their_new_parent() {
    // Regression: a moved table kept its old toml_edit position and stayed in place in the file.
    let p = Plan::new("move");
    let (code, v) = p.json(&["task", "move", "gate-sw", "--parent", "mgmt"]);
    assert_eq!(code, 0, "{v}");
    let text = p.text();
    let at = |s: &str| text.find(s).unwrap();
    assert!(at("id = \"decision-gate\"") < at("id = \"gate-sw\""));
    assert!(at("id = \"gate-sw\"") < at("id = \"platform\""));
    let (code, v) = p.json(&["fmt", "--check"]);
    assert_eq!(
        (code, v["changed"].clone()),
        (0, Value::Bool(false)),
        "still canonical"
    );
}

#[test]
fn empty_plans_export_and_query() {
    let p = Plan::new("empty");
    std::fs::write(
        &p.file,
        "[project]\nformat = 0\nname = \"Empty\"\nstart = 2027-01-04\n",
    )
    .unwrap();
    for args in [vec!["check"], vec!["critical"], vec!["ls"]] {
        let (code, _) = p.json(&args);
        assert_eq!(code, 0, "{args:?}");
    }
    let out = p.dir.join("e.svg");
    let (code, _) = p.json(&["export", out.to_str().unwrap()]);
    assert_eq!(code, 0);
}

#[test]
fn resources_usage_and_leveling() {
    let p = Plan::new("level");
    let (code, v) = p.json(&["res", "ls"]);
    assert_eq!(code, 0);
    let dev = &v["resources"][1];
    assert_eq!(
        (dev["id"].as_str(), dev["over_allocations"].as_u64()),
        (Some("dev"), Some(0))
    );

    // Leveled fixture: monthly dev demand never exceeds capacity.
    let (code, v) = p.json(&["res", "usage", "dev", "--period", "month"]);
    assert_eq!(code, 0, "{v}");
    assert_eq!(v["periods"][0]["start"], "2027-01-01");
    assert!(v["over_allocated"].as_array().unwrap().is_empty());
    let (_, page) = p.json(&["res", "usage", "dev", "--period", "week", "--limit", "3"]);
    assert_eq!(page["periods"].as_array().unwrap().len(), 3);
    assert!(page["next_cursor"].is_string());

    // Turning leveling off is previewed with its consequences and writes nothing.
    let before = p.text();
    let (code, v) = p.json(&["level", "--mode", "off", "--dry-run"]);
    assert_eq!(code, 0, "{v}");
    assert_eq!(v["end_date_delta"], "-40d");
    assert_eq!(v["leveling"]["mode"], "off");
    assert_eq!(v["leveling"]["over_allocated"][0]["resource"], "dev");
    assert!(
        v["critical_path_changed"]["removed"]
            .as_array()
            .unwrap()
            .iter()
            .any(|x| x == "gate-sw"),
        "{v}"
    );
    let codes: Vec<&str> = v["new_diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["code"].as_str().unwrap())
        .collect();
    assert_eq!(codes, ["L-RES-01", "L-RES-01"]);
    assert_eq!(p.text(), before);

    // Leveling again is a no-op; a bad mode is refused and leaves the file alone.
    let (code, v) = p.json(&["level"]);
    assert_eq!((code, v["noop"].as_bool()), (0, Some(true)), "{v}");
    let (code, v) = p.json(&["level", "--mode", "fast"]);
    assert_eq!(
        (code, v["errors"][0]["code"].as_str()),
        (1, Some("E-OPS-04"))
    );
    assert_eq!(p.text(), before);

    // Smoothing keeps the finish and says what it could not fix.
    let (code, v) = p.json(&["level", "--mode", "smoothing"]);
    assert_eq!(code, 0, "{v}");
    assert_eq!(v["leveling"]["finish"], v["leveling"]["unleveled_finish"]);
    assert_eq!(v["leveling"]["unresolved"][0], "gate-sw");
    assert!(p.text().contains("leveling = \"smoothing\""));

    // An unknown resource is a usage error.
    let (code, v) = p.json(&["res", "usage", "nobody"]);
    assert_eq!(
        (code, v["errors"][0]["code"].as_str()),
        (2, Some("E-CLI-01"))
    );
}

#[test]
fn settings_set_and_unset() {
    let p = Plan::new("settings");
    let (code, v) = p.json(&[
        "settings",
        "set",
        "near_critical=10d",
        "phase=preliminary",
        "hours_per_day=8",
    ]);
    assert_eq!(code, 0, "{v}");
    let text = p.text();
    assert!(
        text.contains("hours_per_day = 8\n"),
        "numbers stay numbers:\n{text}"
    );
    assert!(text.contains("near_critical = \"10d\""), "{text}");
    assert!(text.contains("phase = \"preliminary\""));
    let (code, v) = p.json(&["settings", "set", "near_critical=10d", "phase=preliminary"]);
    assert_eq!((code, v["noop"].as_bool()), (0, Some(true)));
    let (code, v) = p.json(&["settings", "set", "--unset", "phase"]);
    assert_eq!(code, 0, "{v}");
    assert!(!p.text().contains("phase ="));
    // Unknown and reserved keys are refused, with the list of valid ones.
    let before = p.text();
    for bad in ["speed=1", "progress_logic=retained"] {
        let (code, v) = p.json(&["settings", "set", bad]);
        assert_eq!(
            (code, v["errors"][0]["code"].as_str()),
            (1, Some("E-FMT-02")),
            "{bad}"
        );
        assert!(
            v["errors"][0]["hint"]
                .as_str()
                .unwrap()
                .contains("leveling")
        );
    }
    assert_eq!(p.text(), before);
}
