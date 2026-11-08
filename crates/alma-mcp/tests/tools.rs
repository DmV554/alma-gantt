//! MCP tools, called directly: same results as the CLI, dry runs, atomic batches, images.

use std::path::PathBuf;

use alma_mcp::{AlmaServer, tools};
use serde_json::{Value, json};

const FIXTURE: &str = include_str!("../../../examples/hybrid-terminal.alma.toml");

struct Dir(PathBuf);

impl Dir {
    fn new(name: &str) -> Self {
        let d = std::env::temp_dir().join(format!("alma-mcp-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("plan.alma.toml"), FIXTURE).unwrap();
        Self(d)
    }
    fn server(&self) -> AlmaServer {
        AlmaServer::new(self.0.clone(), None)
    }
    fn text(&self) -> String {
        std::fs::read_to_string(self.0.join("plan.alma.toml")).unwrap()
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn args(v: Value) -> serde_json::Map<String, Value> {
    v.as_object().cloned().unwrap()
}

#[test]
fn every_tool_has_a_standalone_object_schema() {
    let ts = tools();
    let names: Vec<&str> = ts.iter().map(|t| t.name.as_ref()).collect();
    for expected in [
        "guide",
        "check",
        "render",
        "apply",
        "task_add",
        "dep_add",
        "cal_except",
        "rule_set",
        "level",
        "settings_set",
        "list_resources",
        "resource_usage",
    ] {
        assert!(names.contains(&expected), "missing {expected}");
    }
    let mut sorted = names.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), names.len(), "duplicate tool names");
    for t in &ts {
        let s = Value::Object((*t.input_schema).clone());
        assert_eq!(s["type"], "object", "{}", t.name);
        assert!(
            !s.to_string().contains("$ref"),
            "{} has an unresolved $ref",
            t.name
        );
        assert!(
            t.description.as_ref().is_some_and(|d| !d.is_empty()),
            "{}",
            t.name
        );
        assert!(
            t.name.chars().all(|c| c.is_ascii_lowercase() || c == '_'),
            "{}",
            t.name
        );
    }
    // Every batch operation has a tool.
    let schema: Value = serde_json::from_str(alma_ops::schema::OPS_SCHEMA).unwrap();
    for v in schema["$defs"]["op"]["oneOf"].as_array().unwrap() {
        let op = v["properties"]["op"]["const"]
            .as_str()
            .unwrap()
            .replace('.', "_");
        assert!(names.contains(&op.as_str()), "no tool for {op}");
    }
}

#[test]
fn queries_match_the_cli_json() {
    let d = Dir::new("queries");
    let s = d.server();
    let check = s.call("check", &args(json!({})));
    assert!(!check.is_error);
    assert_eq!(check.json["counts"]["error"], 0);
    let crit = s.call("critical_path", &args(json!({})));
    assert_eq!(crit.json["project_finish"], "2027-09-24");
    let show = s.call("show_task", &args(json!({ "id": "gate-sw" })));
    assert_eq!(show.json["start"], "2027-05-17");
    let list = s.call("list_tasks", &args(json!({ "limit": 5, "fields": ["id"] })));
    assert_eq!(list.json["tasks"].as_array().unwrap().len(), 5);
    assert_eq!(list.json["next_cursor"], "5");
    let explain = s.call("explain", &args(json!({ "id": "uat" })));
    assert_eq!(explain.json["chain"][0]["driven_by"], "h4");
    let keys = s.call("keys", &args(json!({ "table": "task.deps" })));
    assert!(
        keys.json["keys"]
            .as_array()
            .unwrap()
            .iter()
            .any(|k| k["key"] == "lag")
    );
    assert!(
        s.call("guide", &args(json!({}))).json["guide"]
            .as_str()
            .unwrap()
            .contains("Recommended loop")
    );
}

#[test]
fn mutations_dry_run_and_write() {
    let d = Dir::new("mutations");
    let s = d.server();
    let before = d.text();
    let r = s.call(
        "task_set",
        &args(json!({ "id": "yard-sw", "fields": { "duration": "55d" }, "dry_run": true })),
    );
    assert!(!r.is_error, "{}", r.json);
    assert_eq!(r.json["end_date_delta"], "+5d");
    assert_eq!(d.text(), before, "dry run writes nothing");
    let r = s.call(
        "task_set",
        &args(json!({ "id": "yard-sw", "fields": { "duration": "55d" } })),
    );
    assert!(!r.is_error);
    assert!(d.text().contains("duration = \"55d\""));
    let r = s.call(
        "cal_except",
        &args(json!({
            "calendar": "cl-standard", "name": "Feriado", "from": "2027-06-21", "to": "2027-06-22"
        })),
    );
    assert!(!r.is_error, "{}", r.json);
}

#[test]
fn resources_and_leveling() {
    let d = Dir::new("resources");
    let s = d.server();
    let r = s.call("list_resources", &args(json!({})));
    assert!(!r.is_error, "{}", r.json);
    assert_eq!(r.json["resources"].as_array().unwrap().len(), 4);
    let r = s.call(
        "resource_usage",
        &args(json!({ "id": "dev", "period": "month", "limit": 2 })),
    );
    assert!(!r.is_error, "{}", r.json);
    assert_eq!(r.json["periods"].as_array().unwrap().len(), 2);
    assert_eq!(r.json["next_cursor"], "2");
    let r = s.call("resource_usage", &args(json!({ "id": "nobody" })));
    assert!(r.is_error);

    let before = d.text();
    let r = s.call("level", &args(json!({ "mode": "off", "dry_run": true })));
    assert!(!r.is_error, "{}", r.json);
    assert_eq!(r.json["leveling"]["over_allocated"][0]["resource"], "dev");
    assert_eq!(d.text(), before);
    let r = s.call(
        "settings_set",
        &args(json!({ "fields": { "leveling": "smoothing" } })),
    );
    assert!(!r.is_error, "{}", r.json);
    assert_eq!(r.json["leveling"]["mode"], "smoothing");
    assert!(d.text().contains("leveling = \"smoothing\""));
}

#[test]
fn failures_are_tool_errors_with_codes_and_leave_the_file_alone() {
    let d = Dir::new("failures");
    let s = d.server();
    let before = d.text();
    for (tool, a) in [
        ("dep_add", json!({ "pred": "h4", "succ": "discovery" })),
        (
            "task_set",
            json!({ "id": "nope", "fields": { "duration": "1d" } }),
        ),
        (
            "task_set",
            json!({ "id": "uat", "fields": { "duraton": "1d" } }),
        ),
        ("task_add", json!({ "id": "x", "name": "X", "bogus": 1 })),
        (
            "apply",
            json!({ "ops": [
            { "op": "task.add", "id": "docs", "name": "Docs", "parent": "golive" },
            { "op": "dep.add", "pred": "docs", "succ": "missing" }
        ] }),
        ),
        ("nonexistent", json!({})),
    ] {
        let r = s.call(tool, &args(a));
        assert!(r.is_error, "{tool} should fail");
        let code = r.json["errors"][0]["code"].as_str().unwrap_or_default();
        assert!(code.starts_with("E-"), "{tool}: {}", r.json);
        assert_eq!(d.text(), before, "{tool} changed the file");
    }
}

#[test]
fn render_returns_an_image() {
    let d = Dir::new("render");
    let s = d.server();
    let r = s.call("render", &args(json!({ "range": "2027-01..2027-03" })));
    assert!(!r.is_error, "{}", r.json);
    assert_eq!(&r.png.as_ref().unwrap()[..4], b"\x89PNG");
    let result = alma_mcp::to_result(r);
    let v = serde_json::to_value(&result).unwrap();
    assert_eq!(v["content"][1]["type"], "image");
    assert_eq!(v["content"][1]["mimeType"], "image/png");
    let svg = s.call("render", &args(json!({ "format": "svg" })));
    assert!(svg.json["svg"].as_str().unwrap().starts_with("<svg"));
}

#[test]
fn init_then_build_a_plan() {
    let d = Dir::new("init");
    std::fs::remove_file(d.0.join("plan.alma.toml")).unwrap();
    let s = d.server();
    let r = s.call(
        "init",
        &args(json!({ "path": "new.alma.toml", "name": "New", "start": "2027-01-04" })),
    );
    assert!(!r.is_error, "{}", r.json);
    // The new file is the only plan in the directory, so tools find it without `file`.
    let r = s.call(
        "apply",
        &args(json!({ "ops": [
        { "op": "task.add", "id": "start", "name": "Start", "fields": { "kind": "milestone" } },
        { "op": "task.add", "id": "work", "name": "Work", "fields": { "duration": "5d" } },
        { "op": "dep.add", "pred": "start", "succ": "work" }
    ] })),
    );
    assert!(!r.is_error, "{}", r.json);
    let r = s.call("show_task", &args(json!({ "id": "work" })));
    assert_eq!(r.json["finish"], "2027-01-08");
    let again = s.call(
        "init",
        &args(json!({ "path": "new.alma.toml", "name": "New", "start": "2027-01-04" })),
    );
    assert!(again.is_error, "init refuses to overwrite");
}

#[test]
fn concurrent_writes_do_not_interleave() {
    let d = Dir::new("concurrent");
    let s = d.server();
    let handles: Vec<_> = (0..8)
        .map(|i| {
            let s = s.clone();
            std::thread::spawn(move || {
                s.call("task_add", &args(json!({ "id": format!("extra-{i}"), "name": "Extra", "parent": "golive", "fields": { "duration": "1d" } })))
            })
        })
        .collect();
    for h in handles {
        assert!(!h.join().unwrap().is_error);
    }
    let text = d.text();
    for i in 0..8 {
        assert!(
            text.contains(&format!("id = \"extra-{i}\"")),
            "lost write {i}"
        );
    }
}
