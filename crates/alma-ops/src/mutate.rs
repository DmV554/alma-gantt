//! Mutations: typed operations applied to the TOML document losslessly, validated, and committed
//! all-or-nothing (SPEC.md §8a, ADR 0004).
//!
//! An operation edits a `toml_edit::DocumentMut`, so comments and formatting of untouched lines
//! survive. After every batch the new text is parsed and scheduled; if that produces any hard
//! error (`E-*`), nothing is written.

// A failed operation returns one `Diagnostic` once per command; boxing it buys nothing.
#![allow(clippy::result_large_err)]

use std::collections::BTreeMap;
use std::path::Path;

use alma_core::diag::Diagnostic;
use alma_core::time::Instant;
use alma_format::keys;
use serde::{Deserialize, Serialize};
use toml_edit::{Array, ArrayOfTables, DocumentMut, InlineTable, Item, Table, Value, value};

use crate::{Loaded, OUTPUT_SCHEMA, load};

/// A field value given on the command line or in a batch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FieldValue {
    /// A string; ISO dates (`2027-01-04`) are written as TOML dates.
    Text(String),
    /// A list of strings.
    List(Vec<String>),
}

/// One operation. In JSON, `op` names the operation (`{"op": "task.add", "id": "a", ...}`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", deny_unknown_fields)]
pub enum Op {
    /// Add a task.
    #[serde(rename = "task.add")]
    TaskAdd {
        /// Id.
        id: String,
        /// Name.
        name: String,
        /// Parent id.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        parent: Option<String>,
        /// Place after this sibling (and its subtree).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        after: Option<String>,
        /// Other `[[task]]` keys.
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        fields: BTreeMap<String, FieldValue>,
        /// When the id exists, update it instead of failing.
        #[serde(default)]
        upsert: bool,
    },
    /// Set or unset task fields.
    #[serde(rename = "task.set")]
    TaskSet {
        /// Id.
        id: String,
        /// Keys to set.
        #[serde(default)]
        fields: BTreeMap<String, FieldValue>,
        /// Keys to remove.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        unset: Vec<String>,
    },
    /// Remove a task.
    #[serde(rename = "task.rm")]
    TaskRm {
        /// Id.
        id: String,
        /// Also remove its subtree and every link to removed tasks.
        #[serde(default)]
        cascade: bool,
    },
    /// Move a task (with its subtree) under another parent or after a sibling.
    #[serde(rename = "task.move")]
    TaskMove {
        /// Id.
        id: String,
        /// New parent; `""` moves it to the top level.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        parent: Option<String>,
        /// Place after this sibling.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        after: Option<String>,
    },
    /// Add a dependency `pred → succ`.
    #[serde(rename = "dep.add")]
    DepAdd {
        /// Predecessor.
        pred: String,
        /// Successor.
        succ: String,
        /// `FS`, `SS`, `FF` or `SF`.
        #[serde(default, rename = "type", skip_serializing_if = "Option::is_none")]
        kind: Option<String>,
        /// Lag literal.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        lag: Option<String>,
        /// `mandatory` or `discretionary`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        nature: Option<String>,
        /// `internal` or `external`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        scope: Option<String>,
        /// Why.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        rationale: Option<String>,
    },
    /// Remove the dependency `pred → succ`.
    #[serde(rename = "dep.rm")]
    DepRm {
        /// Predecessor.
        pred: String,
        /// Successor.
        succ: String,
    },
    /// Add a resource.
    #[serde(rename = "res.add")]
    ResAdd {
        /// Id.
        id: String,
        /// Name.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        /// Kind.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        kind: Option<String>,
        /// Rate as `<amount>/<per>`, e.g. `4.2/day`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        rate: Option<String>,
        /// Capacity.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_units: Option<String>,
    },
    /// Assign a resource to a task (replaces the units of an existing assignment).
    #[serde(rename = "assign")]
    Assign {
        /// Task.
        task: String,
        /// Resource.
        resource: String,
        /// Units (default 1).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        units: Option<String>,
    },
    /// Remove an assignment.
    #[serde(rename = "unassign")]
    Unassign {
        /// Task.
        task: String,
        /// Resource.
        resource: String,
    },
    /// Add a cadence of equal iterations (sprints).
    #[serde(rename = "cadence.add")]
    CadenceAdd {
        /// Id.
        id: String,
        /// First iteration start (YYYY-MM-DD).
        start: String,
        /// Iteration length, e.g. `10d`.
        length: String,
        /// Number of iterations.
        count: u32,
        /// Band label.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        /// Prefix of generated iteration ids (default: the id).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        prefix: Option<String>,
        /// Calendar id.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        calendar: Option<String>,
    },
    /// Remove a cadence.
    #[serde(rename = "cadence.rm")]
    CadenceRm {
        /// Id.
        id: String,
    },
    /// Set `[project]` keys (name, calendar, deadline, currency, description).
    #[serde(rename = "project.set")]
    ProjectSet {
        /// Keys to set.
        fields: BTreeMap<String, FieldValue>,
    },
    /// Add a working calendar.
    #[serde(rename = "cal.add")]
    CalAdd {
        /// Id.
        id: String,
        /// Name.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        /// Working weekdays, e.g. `["mon", "tue", "wed", "thu", "fri"]`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        workdays: Option<Vec<String>>,
        /// Working intervals, e.g. `["09:00-13:00", "14:00-18:00"]`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        hours: Option<Vec<String>>,
    },
    /// Add a holiday or other exception to a calendar (dates inclusive).
    #[serde(rename = "cal.except")]
    CalExcept {
        /// Calendar id.
        calendar: String,
        /// Name, shown by `explain`.
        name: String,
        /// First day.
        from: String,
        /// Last day.
        to: String,
        /// True for extra working time (default: non-working).
        #[serde(default)]
        working: bool,
    },
    /// Set the severity of a lint rule for this plan (`off`, `info`, `warn`, `error`).
    #[serde(rename = "rule.set")]
    RuleSet {
        /// Lint code, e.g. `L-WBS-05`.
        code: String,
        /// New severity.
        severity: String,
    },
}

/// A task whose dates moved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Moved {
    /// Id.
    pub id: String,
    /// Change of early start, in working days (`+2d`), or `new`.
    pub start: String,
    /// Change of early finish.
    pub finish: String,
}

/// Result of a mutation.
#[derive(Debug, Clone, Serialize)]
pub struct MutationReport {
    /// Output schema version.
    pub schema: u32,
    /// True when the batch applied (or would apply, with `--dry-run`).
    pub ok: bool,
    /// True when nothing was written because of `--dry-run`.
    pub dry_run: bool,
    /// True when the batch left the file unchanged (idempotent retry).
    pub noop: bool,
    /// Tasks whose dates moved, added or removed.
    pub changed: Vec<Moved>,
    /// Change of the project finish (`+3d`, `-1d`, `0d`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_date_delta: Option<String>,
    /// Diagnostics that the change introduced.
    pub new_diagnostics: Vec<Diagnostic>,
    /// Why the batch was rejected (empty when `ok`).
    pub errors: Vec<Diagnostic>,
}

impl MutationReport {
    fn rejected(errors: Vec<Diagnostic>, dry_run: bool) -> Self {
        Self {
            schema: OUTPUT_SCHEMA,
            ok: false,
            dry_run,
            noop: false,
            changed: Vec::new(),
            end_date_delta: None,
            new_diagnostics: Vec::new(),
            errors,
        }
    }
}

fn op_err(code: &'static str, msg: impl Into<String>, ids: &[&str]) -> Diagnostic {
    Diagnostic::error(code, msg).with_ids(ids.iter().map(|s| s.to_string()))
}

/// Applies a batch to plan text. Returns the new text and the report; on rejection the text
/// is `None` and the report lists the errors.
pub fn apply(src: &str, ops: &[Op], dry_run: bool) -> (Option<String>, MutationReport) {
    let before = load(src);
    if before.plan.is_none() {
        let errors: Vec<Diagnostic> = before
            .diagnostics
            .iter()
            .filter(|d| d.code.starts_with("E-"))
            .cloned()
            .collect();
        let mut first = op_err(
            "E-OPS-04",
            "the plan file has errors; fix them before running mutations",
            &[],
        );
        first.hint = Some("run `alma check` to list them".into());
        let mut all = vec![first];
        all.extend(errors);
        return (None, MutationReport::rejected(all, dry_run));
    }
    let mut doc: DocumentMut = match src.parse() {
        Ok(d) => d,
        Err(e) => {
            let d = op_err("E-FMT-01", format!("TOML syntax error: {e}"), &[]);
            return (None, MutationReport::rejected(vec![d], dry_run));
        }
    };
    for (n, op) in ops.iter().enumerate() {
        if let Err(mut e) = apply_op(&mut doc, op) {
            if ops.len() > 1 {
                e.message = format!("operation #{}: {}", n + 1, e.message);
            }
            return (None, MutationReport::rejected(vec![e], dry_run));
        }
    }
    let text = doc.to_string();
    let after = load(&text);
    let hard: Vec<Diagnostic> = after
        .diagnostics
        .iter()
        .filter(|d| d.code.starts_with("E-"))
        .cloned()
        .collect();
    if !hard.is_empty() {
        return (None, MutationReport::rejected(hard, dry_run));
    }
    let report = MutationReport {
        schema: OUTPUT_SCHEMA,
        ok: true,
        dry_run,
        noop: text == src,
        changed: moved(&before, &after),
        end_date_delta: end_delta(&before, &after),
        new_diagnostics: new_diagnostics(&before, &after),
        errors: Vec::new(),
    };
    (Some(text), report)
}

/// Applies a batch to a file atomically: on success the new text replaces the file through a
/// rename; on rejection (or `dry_run`) the file is untouched.
pub fn apply_to_file(path: &Path, ops: &[Op], dry_run: bool) -> std::io::Result<MutationReport> {
    let src = std::fs::read_to_string(path)?;
    let (text, report) = apply(&src, ops, dry_run);
    if let Some(text) = text
        && !dry_run
        && !report.noop
    {
        write_atomic(path, &text)?;
        append_log(path, ops);
    }
    Ok(report)
}

/// Writes `text` to `path` through a temporary file in the same directory and a rename.
pub fn write_atomic(path: &Path, text: &str) -> std::io::Result<()> {
    let dir = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("plan");
    let tmp = dir.join(format!(".{name}.tmp-{}", std::process::id()));
    std::fs::write(&tmp, text)?;
    match std::fs::rename(&tmp, path) {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            Err(e)
        }
    }
}

/// Appends the batch to `.alma/log.jsonl` next to the plan. Failures are ignored: the log is
/// for review, git is the history.
fn append_log(path: &Path, ops: &[Op]) {
    use std::io::Write;
    let dir = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
        .join(".alma");
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let line = serde_json::json!({
        "time": secs,
        "actor": std::env::var("ALMA_ACTOR").unwrap_or_else(|_| "cli".into()),
        "file": path.file_name().and_then(|n| n.to_str()),
        "ops": ops,
    });
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("log.jsonl"))
    {
        let _ = writeln!(f, "{line}");
    }
}

fn day_delta(minutes: i64, day: i64) -> String {
    let v = minutes as f64 / day as f64;
    let text = if v.fract().abs() < 1e-9 {
        format!("{}", v as i64)
    } else {
        format!("{v:.2}")
    };
    if minutes > 0 {
        format!("+{text}d")
    } else {
        format!("{text}d")
    }
}

fn dates(l: &Loaded) -> BTreeMap<String, (Instant, Instant)> {
    match (&l.plan, &l.schedule) {
        (Some(p), Some(s)) => p
            .tasks
            .iter()
            .zip(&s.tasks)
            .map(|(t, d)| (t.id.clone(), (d.es, d.ef)))
            .collect(),
        _ => BTreeMap::new(),
    }
}

pub(crate) fn moved(before: &Loaded, after: &Loaded) -> Vec<Moved> {
    let (Some(plan), Some(_)) = (&after.plan, &after.schedule) else {
        return Vec::new();
    };
    let cals = alma_sched::cpm::Calendars::new(plan);
    let cal = cals.project();
    let day = plan.settings.units.minutes_per_day;
    let (b, a) = (dates(before), dates(after));
    let mut out = Vec::new();
    // Source order of the new plan, then removed tasks.
    for t in &plan.tasks {
        let new = a[&t.id];
        match b.get(&t.id) {
            None => out.push(Moved {
                id: t.id.clone(),
                start: "new".into(),
                finish: "new".into(),
            }),
            Some(old) if *old != new => out.push(Moved {
                id: t.id.clone(),
                start: day_delta(cal.between(old.0, new.0), day),
                finish: day_delta(cal.between(old.1, new.1), day),
            }),
            _ => {}
        }
    }
    for id in b.keys().filter(|id| !a.contains_key(*id)) {
        out.push(Moved {
            id: id.clone(),
            start: "removed".into(),
            finish: "removed".into(),
        });
    }
    out
}

pub(crate) fn end_delta(before: &Loaded, after: &Loaded) -> Option<String> {
    let (Some(b), Some(a), Some(plan)) = (&before.schedule, &after.schedule, &after.plan) else {
        return None;
    };
    let cals = alma_sched::cpm::Calendars::new(plan);
    let day = plan.settings.units.minutes_per_day;
    let d = cals.project().between(b.finish, a.finish);
    Some(if d == 0 {
        "0d".into()
    } else {
        day_delta(d, day)
    })
}

pub(crate) fn new_diagnostics(before: &Loaded, after: &Loaded) -> Vec<Diagnostic> {
    // Same rule on the same objects is the same finding, even if its message text changed.
    let key = |d: &Diagnostic| (d.code, d.ids.clone());
    let old: std::collections::HashSet<_> = before.diagnostics.iter().map(key).collect();
    after
        .diagnostics
        .iter()
        .filter(|d| !old.contains(&key(d)))
        .cloned()
        .collect()
}

// ---------------------------------------------------------------------------------------------
// Document editing.

fn tasks_mut(doc: &mut DocumentMut) -> &mut ArrayOfTables {
    if doc.get("task").and_then(Item::as_array_of_tables).is_none() {
        doc.insert("task", Item::ArrayOfTables(ArrayOfTables::new()));
    }
    doc["task"].as_array_of_tables_mut().expect("just ensured")
}

fn task_ids(doc: &DocumentMut) -> Vec<(String, Option<String>)> {
    doc.get("task")
        .and_then(Item::as_array_of_tables)
        .map(|aot| {
            aot.iter()
                .map(|t| {
                    (
                        t.get("id")
                            .and_then(Item::as_str)
                            .unwrap_or_default()
                            .to_string(),
                        t.get("parent").and_then(Item::as_str).map(str::to_string),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

fn find_task(doc: &DocumentMut, id: &str) -> Option<usize> {
    task_ids(doc).iter().position(|(t, _)| t == id)
}

fn require_task(doc: &DocumentMut, id: &str) -> Result<usize, Diagnostic> {
    find_task(doc, id).ok_or_else(|| {
        op_err("E-OPS-02", format!("no task with id `{id}`"), &[id])
            .with_hint("run `alma ls --fields id,name` to list task ids")
    })
}

/// Index range `[i, end)` of task `i` and its subtree (assumes WBS order).
fn subtree(ids: &[(String, Option<String>)], i: usize) -> std::ops::Range<usize> {
    let mut members = vec![ids[i].0.clone()];
    let mut end = i + 1;
    while end < ids.len() {
        match &ids[end].1 {
            Some(p) if members.contains(p) => {
                members.push(ids[end].0.clone());
                end += 1;
            }
            _ => break,
        }
    }
    i..end
}

/// Index where a new task under `parent` (after `after`) goes.
fn insert_index(
    doc: &DocumentMut,
    parent: Option<&str>,
    after: Option<&str>,
) -> Result<usize, Diagnostic> {
    let ids = task_ids(doc);
    if let Some(a) = after {
        let i = require_task(doc, a)?;
        if ids[i].1.as_deref() != parent {
            return Err(op_err(
                "E-OPS-04",
                format!("`{a}` is not a sibling under the requested parent"),
                &[a],
            ));
        }
        return Ok(subtree(&ids, i).end);
    }
    match parent {
        Some(p) => {
            let i = require_task(doc, p)?;
            Ok(subtree(&ids, i).end)
        }
        None => Ok(ids.len()),
    }
}

fn to_item(key: &str, v: &FieldValue) -> Item {
    match v {
        FieldValue::Text(s) => {
            if let Ok(d) = s.parse::<toml_edit::Datetime>()
                && d.date.is_some()
                && d.time.is_none()
                && is_date_key(key)
            {
                return value(d);
            }
            value(s.as_str())
        }
        FieldValue::List(l) => {
            let mut a = Array::new();
            for s in l {
                a.push(s.as_str());
            }
            value(a)
        }
    }
}

fn is_date_key(key: &str) -> bool {
    matches!(key, "deadline" | "start" | "date" | "from" | "to")
}

/// Inserts `key` into `table` at its canonical position (registry order), keeping every other
/// key's formatting and comments.
fn insert_canonical(table: &mut Table, registry: &str, key: &str, item: Item) {
    if let Some(slot) = table.get_mut(key) {
        // Keep the key's decor; replace the value but keep its surrounding whitespace.
        let decor = slot.as_value().map(|v| v.decor().clone());
        *slot = item;
        if let (Some(decor), Some(v)) = (decor, slot.as_value_mut()) {
            *v.decor_mut() = decor;
        }
        return;
    }
    let order: Vec<&str> = keys::keys_of(registry).map(|k| k.key).collect();
    let rank = |k: &str| order.iter().position(|x| *x == k).unwrap_or(usize::MAX);
    let mine = rank(key);
    let later: Vec<String> = table
        .iter()
        .map(|(k, _)| k.to_string())
        .filter(|k| rank(k) > mine)
        .collect();
    let moved: Vec<(toml_edit::Key, Item)> =
        later.iter().filter_map(|k| table.remove_entry(k)).collect();
    table.insert(key, item);
    for (k, v) in moved {
        table.insert_formatted(&k, v);
    }
}

fn set_task_fields(
    table: &mut Table,
    id: &str,
    fields: &BTreeMap<String, FieldValue>,
) -> Result<(), Diagnostic> {
    for (key, v) in fields {
        if let Some((outer, inner)) = key.split_once('.') {
            set_sub_field(table, id, outer, inner, v)?;
            continue;
        }
        check_settable(id, key)?;
        let item = match key.as_str() {
            "constraint" => constraint_item(id, v)?,
            "estimate" => {
                let (o, m, p) = split3(id, v)?;
                for (k, x) in [("o", o), ("m", m), ("p", p)] {
                    set_sub_field(table, id, "estimate", k, &FieldValue::Text(x))?;
                }
                continue;
            }
            "spans" => {
                let FieldValue::Text(t) = v else {
                    return Err(op_err("E-OPS-04", "spans must be FROM..TO", &[id]));
                };
                let (a, b) = t
                    .split_once("..")
                    .ok_or_else(|| op_err("E-OPS-04", "spans must be FROM..TO", &[id]))?;
                set_sub_field(table, id, "spans", "from", &FieldValue::Text(a.into()))?;
                set_sub_field(table, id, "spans", "to", &FieldValue::Text(b.into()))?;
                continue;
            }
            "fixed_cost" => {
                set_sub_field(table, id, "fixed_cost", "amount", v)?;
                continue;
            }
            _ => to_item(key, v),
        };
        insert_canonical(table, "task", key, item);
    }
    Ok(())
}

/// `"25d/30d/40d"` → `("25d", "30d", "40d")`.
fn split3(id: &str, v: &FieldValue) -> Result<(String, String, String), Diagnostic> {
    let parts: Vec<String> = match v {
        FieldValue::Text(t) => t.split('/').map(|x| x.trim().to_string()).collect(),
        FieldValue::List(l) => l.clone(),
    };
    match parts.as_slice() {
        [o, m, p] => Ok((o.clone(), m.clone(), p.clone())),
        _ => Err(op_err(
            "E-OPS-04",
            "estimate must be OPTIMISTIC/MOST-LIKELY/PESSIMISTIC, e.g. 25d/30d/40d",
            &[id],
        )),
    }
}

/// Sets one key of an inline sub-table (`estimate.basis`, `fixed_cost.accrue`,
/// `custom.zone`), creating the table when needed.
fn set_sub_field(
    table: &mut Table,
    id: &str,
    outer: &str,
    inner: &str,
    v: &FieldValue,
) -> Result<(), Diagnostic> {
    const SUBS: &[&str] = &["estimate", "spans", "fixed_cost", "custom"];
    if !SUBS.contains(&outer) {
        return Err(op_err(
            "E-OPS-04",
            format!("`{outer}.{inner}`: only {} have sub-keys", SUBS.join(", ")),
            &[id],
        ));
    }
    let registry = format!("task.{outer}");
    if outer != "custom" && keys::lookup(&registry, inner).is_none() {
        let known: Vec<&str> = keys::keys_of(&registry).map(|k| k.key).collect();
        return Err(op_err(
            "E-FMT-02",
            format!("unknown key `{inner}` in `{outer}`"),
            &[id],
        )
        .with_hint(format!("`{outer}` takes: {}", known.join(", "))));
    }
    if table.get(outer).and_then(Item::as_inline_table).is_none() {
        let mut t = InlineTable::new();
        t.fmt();
        insert_canonical(table, "task", outer, value(t));
    }
    let it = table
        .get_mut(outer)
        .and_then(Item::as_inline_table_mut)
        .expect("just ensured");
    let new: Value = match to_item(inner, v) {
        Item::Value(x) => x,
        _ => return Err(op_err("E-OPS-04", "invalid value", &[id])),
    };
    it.insert(inner, new);
    let order: Vec<&str> = keys::keys_of(&registry).map(|k| k.key).collect();
    let rank = |k: &str| order.iter().position(|x| *x == k).unwrap_or(usize::MAX);
    it.sort_values_by(|a, _, b, _| rank(a.get()).cmp(&rank(b.get())));
    it.fmt();
    Ok(())
}

/// Keys that have their own commands or cannot be set as plain values.
const STRUCTURED: &[&str] = &["id", "parent", "deps", "assign"];

fn check_settable(id: &str, key: &str) -> Result<(), Diagnostic> {
    match keys::lookup("task", key) {
        None => {
            let hint = keys::suggest("task", key)
                .map(|s| format!("did you mean `{s}`?"))
                .unwrap_or_else(|| "see docs/format.md §6 for task keys".into());
            Err(op_err("E-FMT-02", format!("unknown task key `{key}`"), &[id]).with_hint(hint))
        }
        Some(def) if matches!(def.tier, keys::Tier::R(_)) => Err(op_err(
            "E-FMT-05",
            format!("task key `{key}` is reserved for a later milestone"),
            &[id],
        )),
        Some(_) if STRUCTURED.contains(&key) => Err(op_err(
            "E-OPS-04",
            format!("`{key}` cannot be set as a plain field"),
            &[id],
        )
        .with_hint(match key {
            "deps" => "use `alma dep add` / `alma dep rm`",
            "assign" => "use `alma assign` / `alma unassign`",
            "parent" => "use `alma task move`",
            "id" => "ids are stable; remove and re-add the task to change it",
            _ => "edit the file directly for this key, then run `alma check`",
        })),
        Some(_) => Ok(()),
    }
}

/// `"SNET:2027-03-01"` or `"ASAP"` → `{ type = "SNET", date = 2027-03-01 }`.
fn constraint_item(id: &str, v: &FieldValue) -> Result<Item, Diagnostic> {
    let FieldValue::Text(s) = v else {
        return Err(op_err(
            "E-OPS-04",
            "constraint must be TYPE or TYPE:DATE",
            &[id],
        ));
    };
    let (kind, date) = match s.split_once(':') {
        Some((k, d)) => (k, Some(d)),
        None => (s.as_str(), None),
    };
    let mut t = InlineTable::new();
    t.insert("type", kind.into());
    if let Some(d) = date {
        let dt: toml_edit::Datetime = d.parse().map_err(|_| {
            op_err(
                "E-OPS-04",
                format!("`{d}` is not a date like 2027-03-01"),
                &[id],
            )
        })?;
        t.insert("date", Value::from(dt));
    }
    t.fmt();
    Ok(value(t))
}

fn new_table() -> Table {
    let mut t = Table::new();
    t.decor_mut().set_prefix("\n");
    t
}

fn apply_op(doc: &mut DocumentMut, op: &Op) -> Result<(), Diagnostic> {
    match op {
        Op::TaskAdd {
            id,
            name,
            parent,
            after,
            fields,
            upsert,
        } => {
            if let Some(i) = find_task(doc, id) {
                // Idempotent: an identical definition is a no-op; `upsert` updates.
                let table = tasks_mut(doc).get_mut(i).expect("index from find_task");
                let mut probe = table.clone();
                insert_canonical(&mut probe, "task", "name", value(name.as_str()));
                set_task_fields(&mut probe, id, fields)?;
                let same_parent = probe.get("parent").and_then(Item::as_str) == parent.as_deref();
                if probe.to_string() == table.to_string() && same_parent {
                    return Ok(());
                }
                if !upsert {
                    return Err(op_err(
                        "E-OPS-01",
                        format!("task `{id}` already exists with a different definition"),
                        &[id],
                    )
                    .with_hint("use `alma task set` to change it, or pass --upsert"));
                }
                *table = probe;
                if !same_parent {
                    return apply_op(
                        doc,
                        &Op::TaskMove {
                            id: id.clone(),
                            parent: Some(parent.clone().unwrap_or_default()),
                            after: after.clone(),
                        },
                    );
                }
                return Ok(());
            }
            let index = insert_index(doc, parent.as_deref(), after.as_deref())?;
            let mut t = new_table();
            t.insert("id", value(id.as_str()));
            if let Some(p) = parent {
                t.insert("parent", value(p.as_str()));
            }
            t.insert("name", value(name.as_str()));
            set_task_fields(&mut t, id, fields)?;
            tasks_mut(doc).insert(index, t);
            Ok(())
        }
        Op::TaskSet { id, fields, unset } => {
            let i = require_task(doc, id)?;
            let table = tasks_mut(doc).get_mut(i).expect("index from require_task");
            set_task_fields(table, id, fields)?;
            for key in unset {
                if matches!(key.as_str(), "id" | "name") {
                    return Err(op_err("E-OPS-04", format!("`{key}` is required"), &[id]));
                }
                table.remove(key);
            }
            Ok(())
        }
        Op::TaskRm { id, cascade } => {
            let i = require_task(doc, id)?;
            let ids = task_ids(doc);
            let range = subtree(&ids, i);
            let removed: Vec<String> = ids[range.clone()].iter().map(|(t, _)| t.clone()).collect();
            if range.len() > 1 && !cascade {
                return Err(op_err(
                    "E-OPS-03",
                    format!("task `{id}` has {} descendants", range.len() - 1),
                    &[id],
                )
                .with_hint(
                    "pass --cascade to remove the subtree too, or move the children first",
                ));
            }
            let dependents: Vec<String> = dependents_of(doc, &removed);
            if !dependents.is_empty() && !cascade {
                let mut ids: Vec<&str> = vec![id.as_str()];
                ids.extend(dependents.iter().map(String::as_str));
                return Err(op_err(
                    "E-OPS-03",
                    format!("other tasks depend on `{id}`: {}", dependents.join(", ")),
                    &ids,
                )
                .with_hint("pass --cascade to remove those links too"));
            }
            let aot = tasks_mut(doc);
            for _ in range.clone() {
                aot.remove(range.start);
            }
            for t in aot.iter_mut() {
                remove_deps_on(t, &removed);
            }
            Ok(())
        }
        Op::TaskMove { id, parent, after } => {
            let i = require_task(doc, id)?;
            let ids = task_ids(doc);
            let range = subtree(&ids, i);
            let members: Vec<String> = ids[range.clone()].iter().map(|(t, _)| t.clone()).collect();
            let new_parent = match parent {
                Some(p) if p.is_empty() => None,
                Some(p) => Some(p.clone()),
                None => ids[i].1.clone(),
            };
            if let Some(p) = &new_parent {
                if members.contains(p) {
                    return Err(op_err(
                        "E-OPS-04",
                        format!("cannot move `{id}` under its own subtree (`{p}`)"),
                        &[id, p],
                    ));
                }
                require_task(doc, p)?;
            }
            let aot = tasks_mut(doc);
            let mut tables: Vec<Table> = Vec::new();
            for _ in range.clone() {
                tables.push(aot.remove(range.start));
            }
            match &new_parent {
                Some(p) => insert_canonical(&mut tables[0], "task", "parent", value(p.as_str())),
                None => {
                    tables[0].remove("parent");
                }
            }
            let index = insert_index(doc, new_parent.as_deref(), after.as_deref())?;
            let aot = tasks_mut(doc);
            for (k, mut t) in tables.into_iter().enumerate() {
                // A table renders at its stored position, not at its index in the array; clear it
                // so the moved table renders where it now sits, like a new table.
                t.set_position(None);
                aot.insert(index + k, t);
            }
            Ok(())
        }
        Op::DepAdd {
            pred,
            succ,
            kind,
            lag,
            nature,
            scope,
            rationale,
        } => {
            require_task(doc, pred)?;
            let i = require_task(doc, succ)?;
            let mut link = InlineTable::new();
            link.insert("pred", pred.as_str().into());
            if let Some(k) = kind.as_deref().filter(|k| *k != "FS") {
                link.insert("type", k.into());
            }
            if let Some(l) = lag.as_deref().filter(|l| *l != "0d") {
                link.insert("lag", l.into());
            }
            if let Some(n) = nature.as_deref().filter(|n| *n != "mandatory") {
                link.insert("nature", n.into());
            }
            if let Some(s) = scope.as_deref().filter(|s| *s != "internal") {
                link.insert("scope", s.into());
            }
            if let Some(r) = rationale {
                link.insert("rationale", r.as_str().into());
            }
            link.fmt();
            let table = tasks_mut(doc).get_mut(i).expect("index from require_task");
            let existing = table.get_mut("deps").and_then(Item::as_array_mut);
            match existing {
                Some(arr) => {
                    let pos = arr.iter().position(|v| {
                        v.as_inline_table()
                            .and_then(|t| t.get("pred"))
                            .and_then(Value::as_str)
                            == Some(pred.as_str())
                    });
                    match pos {
                        // Same pair: an identical link is a no-op, a different one replaces it.
                        Some(p) => {
                            let decor = arr.get(p).map(|v| v.decor().clone());
                            arr.replace(p, link);
                            if let (Some(d), Some(v)) = (decor, arr.get_mut(p)) {
                                *v.decor_mut() = d;
                            }
                        }
                        None => push_like_last(arr, link),
                    }
                }
                None => {
                    let mut arr = Array::new();
                    arr.push(link);
                    insert_canonical(table, "task", "deps", value(arr));
                }
            }
            Ok(())
        }
        Op::DepRm { pred, succ } => {
            let i = require_task(doc, succ)?;
            let table = tasks_mut(doc).get_mut(i).expect("index from require_task");
            let before = deps_len(table);
            remove_deps_on(table, std::slice::from_ref(pred));
            if deps_len(table) == before {
                return Err(op_err(
                    "E-OPS-02",
                    format!("`{succ}` has no dependency on `{pred}`"),
                    &[pred, succ],
                ));
            }
            Ok(())
        }
        Op::ResAdd {
            id,
            name,
            kind,
            rate,
            max_units,
        } => {
            let mut t = new_table();
            t.insert("id", value(id.as_str()));
            if let Some(n) = name {
                t.insert("name", value(n.as_str()));
            }
            if let Some(k) = kind {
                t.insert("kind", value(k.as_str()));
            }
            if let Some(r) = rate {
                let (amount, per) = r.split_once('/').ok_or_else(|| {
                    op_err(
                        "E-OPS-04",
                        format!("rate `{r}` must look like 4.2/day"),
                        &[id],
                    )
                })?;
                let mut rt = InlineTable::new();
                rt.insert("amount", amount.into());
                rt.insert("per", per.into());
                rt.fmt();
                t.insert("rate", value(rt));
            }
            if let Some(m) = max_units {
                let v: Value = m.parse().map_err(|_| {
                    op_err(
                        "E-OPS-04",
                        format!("max_units `{m}` is not a number"),
                        &[id],
                    )
                })?;
                t.insert("max_units", Item::Value(v));
            }
            push_top_table(doc, "resource", id, t)
        }
        Op::Assign {
            task,
            resource,
            units,
        } => {
            let i = require_task(doc, task)?;
            let units_v: Option<Value> = match units {
                Some(u) => Some(u.parse().map_err(|_| {
                    op_err("E-OPS-04", format!("units `{u}` is not a number"), &[task])
                })?),
                None => None,
            };
            let mut a = InlineTable::new();
            a.insert("resource", resource.as_str().into());
            a.insert("units", units_v.unwrap_or_else(|| 1.into()));
            a.fmt();
            let table = tasks_mut(doc).get_mut(i).expect("index from require_task");
            match table.get_mut("assign").and_then(Item::as_array_mut) {
                Some(arr) => {
                    let pos = arr.iter().position(|v| {
                        v.as_inline_table()
                            .and_then(|t| t.get("resource"))
                            .and_then(Value::as_str)
                            == Some(resource.as_str())
                    });
                    match pos {
                        Some(p) => {
                            arr.replace(p, a);
                        }
                        None => push_like_last(arr, a),
                    }
                }
                None => {
                    let mut arr = Array::new();
                    arr.push(a);
                    insert_canonical(table, "task", "assign", value(arr));
                }
            }
            Ok(())
        }
        Op::CadenceAdd {
            id,
            start,
            length,
            count,
            name,
            prefix,
            calendar,
        } => {
            let date: toml_edit::Datetime = start.parse().map_err(|_| {
                op_err(
                    "E-OPS-04",
                    format!("`{start}` is not a date like 2027-03-01"),
                    &[id],
                )
            })?;
            let mut t = new_table();
            t.insert("id", value(id.as_str()));
            if let Some(n) = name {
                t.insert("name", value(n.as_str()));
            }
            if let Some(p) = prefix {
                t.insert("prefix", value(p.as_str()));
            }
            t.insert("start", value(date));
            t.insert("length", value(length.as_str()));
            t.insert("count", value(i64::from(*count)));
            if let Some(c) = calendar {
                t.insert("calendar", value(c.as_str()));
            }
            push_top_table(doc, "cadence", id, t)
        }
        Op::CadenceRm { id } => {
            let aot = doc
                .get_mut("cadence")
                .and_then(Item::as_array_of_tables_mut);
            let pos = aot.as_ref().and_then(|a| {
                a.iter()
                    .position(|t| t.get("id").and_then(Item::as_str) == Some(id.as_str()))
            });
            match (aot, pos) {
                (Some(a), Some(p)) => {
                    a.remove(p);
                    if a.is_empty() {
                        doc.remove("cadence");
                    }
                    Ok(())
                }
                _ => Err(op_err(
                    "E-OPS-02",
                    format!("no cadence with id `{id}`"),
                    &[id],
                )),
            }
        }
        Op::ProjectSet { fields } => {
            let Some(project) = doc.get_mut("project").and_then(Item::as_table_mut) else {
                return Err(op_err("E-OPS-04", "the plan has no [project] table", &[]));
            };
            for (key, v) in fields {
                match keys::lookup("project", key) {
                    Some(def) if !matches!(def.tier, keys::Tier::R(_)) && key != "format" => {}
                    _ => {
                        let known: Vec<&str> = keys::keys_of("project")
                            .filter(|k| !matches!(k.tier, keys::Tier::R(_)) && k.key != "format")
                            .map(|k| k.key)
                            .collect();
                        return Err(op_err(
                            "E-FMT-02",
                            format!("`{key}` cannot be set on [project]"),
                            &[],
                        )
                        .with_hint(format!("[project] takes: {}", known.join(", "))));
                    }
                }
                insert_canonical(project, "project", key, to_item(key, v));
            }
            Ok(())
        }
        Op::CalAdd {
            id,
            name,
            workdays,
            hours,
        } => {
            let mut t = new_table();
            t.insert("id", value(id.as_str()));
            if let Some(n) = name {
                t.insert("name", value(n.as_str()));
            }
            let list = |items: &[String]| {
                let mut a = Array::new();
                for x in items {
                    a.push(x.as_str());
                }
                value(a)
            };
            if let Some(w) = workdays {
                t.insert("workdays", list(w));
            }
            if let Some(h) = hours {
                t.insert("hours", list(h));
            }
            push_top_table(doc, "calendar", id, t)
        }
        Op::CalExcept {
            calendar,
            name,
            from,
            to,
            working,
        } => {
            let date = |d: &str| -> Result<Value, Diagnostic> {
                d.parse::<toml_edit::Datetime>()
                    .map(Value::from)
                    .map_err(|_| {
                        op_err(
                            "E-OPS-04",
                            format!("`{d}` is not a date like 2027-12-15"),
                            &[calendar],
                        )
                    })
            };
            let (from, to) = (date(from)?, date(to)?);
            let cal = doc
                .get_mut("calendar")
                .and_then(Item::as_array_of_tables_mut)
                .and_then(|a| {
                    a.iter_mut()
                        .find(|t| t.get("id").and_then(Item::as_str) == Some(calendar.as_str()))
                })
                .ok_or_else(|| {
                    op_err(
                        "E-OPS-02",
                        format!("no calendar with id `{calendar}`"),
                        &[calendar],
                    )
                    .with_hint("create it with `alma cal add`")
                })?;
            let mut ex = InlineTable::new();
            ex.insert("name", name.as_str().into());
            ex.insert("from", from);
            ex.insert("to", to);
            ex.insert("working", (*working).into());
            ex.fmt();
            match cal.get_mut("exceptions").and_then(Item::as_array_mut) {
                Some(arr) => {
                    let same = arr
                        .iter()
                        .any(|v| v.to_string().trim() == ex.to_string().trim());
                    if !same {
                        push_like_last(arr, ex);
                    }
                }
                None => {
                    let mut arr = Array::new();
                    arr.push(ex);
                    insert_canonical(cal, "calendar", "exceptions", value(arr));
                }
            }
            Ok(())
        }
        Op::RuleSet { code, severity } => {
            if !["off", "info", "warn", "error"].contains(&severity.as_str()) {
                return Err(op_err(
                    "E-OPS-04",
                    format!("severity `{severity}` must be off, info, warn or error"),
                    &[code],
                ));
            }
            let created = doc.get("rules").is_none();
            if created {
                let mut rules = Table::new();
                rules.set_implicit(true);
                doc.insert("rules", Item::Table(rules));
            }
            let rules = doc["rules"]
                .as_table_mut()
                .ok_or_else(|| op_err("E-OPS-04", "`rules` is not a table", &[]))?;
            let created = created || rules.get("severity").is_none();
            if rules.get("severity").is_none() {
                rules.insert("severity", Item::Table(new_table()));
            }
            let sev = rules["severity"]
                .as_table_mut()
                .ok_or_else(|| op_err("E-OPS-04", "`rules.severity` is not a table", &[]))?;
            sev.insert(code, value(severity.as_str()));
            if created {
                alma_format::place_tables(doc);
            }
            Ok(())
        }
        Op::Unassign { task, resource } => {
            let i = require_task(doc, task)?;
            let table = tasks_mut(doc).get_mut(i).expect("index from require_task");
            let Some(arr) = table.get_mut("assign").and_then(Item::as_array_mut) else {
                return Err(op_err(
                    "E-OPS-02",
                    format!("`{task}` has no assignments"),
                    &[task],
                ));
            };
            let before = arr.len();
            arr.retain(|v| {
                v.as_inline_table()
                    .and_then(|t| t.get("resource"))
                    .and_then(Value::as_str)
                    != Some(resource.as_str())
            });
            if arr.len() == before {
                return Err(op_err(
                    "E-OPS-02",
                    format!("`{task}` has no assignment of `{resource}`"),
                    &[task, resource],
                ));
            }
            if arr.is_empty() {
                table.remove("assign");
            }
            Ok(())
        }
    }
}

/// Appends a `[[kind]]` table. An identical existing table is a no-op; a different one with the
/// same id is `E-OPS-01`. Creating the first table of a kind places it in canonical order.
fn push_top_table(doc: &mut DocumentMut, kind: &str, id: &str, t: Table) -> Result<(), Diagnostic> {
    let created = doc.get(kind).and_then(Item::as_array_of_tables).is_none();
    if created {
        doc.insert(kind, Item::ArrayOfTables(ArrayOfTables::new()));
    }
    let aot = doc[kind].as_array_of_tables_mut().expect("just ensured");
    if let Some(existing) = aot
        .iter()
        .find(|r| r.get("id").and_then(Item::as_str) == Some(id))
    {
        if existing.to_string() == t.to_string() {
            return Ok(());
        }
        return Err(op_err(
            "E-OPS-01",
            format!("{kind} `{id}` already exists with a different definition"),
            &[id],
        ));
    }
    aot.push(t);
    if created {
        alma_format::place_tables(doc);
    }
    Ok(())
}

/// Appends to an inline array using the spacing of its last element, so a multi-line array
/// stays one element per line.
fn push_like_last(arr: &mut Array, v: impl Into<Value>) {
    let decor = arr.iter().last().map(|last| last.decor().clone());
    arr.push(v);
    if let (Some(d), Some(new)) = (decor, arr.iter_mut().last()) {
        let multiline = d
            .prefix()
            .and_then(|p| p.as_str())
            .is_some_and(|p| p.contains('\n'));
        if multiline {
            *new.decor_mut() = d;
        }
    }
}

fn deps_len(table: &Table) -> usize {
    table
        .get("deps")
        .and_then(Item::as_array)
        .map_or(0, Array::len)
}

fn remove_deps_on(table: &mut Table, preds: &[String]) {
    let Some(arr) = table.get_mut("deps").and_then(Item::as_array_mut) else {
        return;
    };
    arr.retain(|v| {
        let p = v
            .as_inline_table()
            .and_then(|t| t.get("pred"))
            .and_then(Value::as_str);
        !p.is_some_and(|p| preds.iter().any(|x| x == p))
    });
    if arr.is_empty() {
        table.remove("deps");
    } else if let Some(first) = arr.get_mut(0) {
        // Removing the first element can leave a leading space; normalize it.
        let suffix = first.decor().suffix().cloned();
        first.decor_mut().set_prefix("");
        if let Some(s) = suffix {
            first.decor_mut().set_suffix(s);
        }
    }
}

fn dependents_of(doc: &DocumentMut, ids: &[String]) -> Vec<String> {
    let Some(aot) = doc.get("task").and_then(Item::as_array_of_tables) else {
        return Vec::new();
    };
    aot.iter()
        .filter(|t| {
            let own = t.get("id").and_then(Item::as_str).unwrap_or_default();
            !ids.iter().any(|x| x == own)
                && t.get("deps").and_then(Item::as_array).is_some_and(|a| {
                    a.iter().any(|v| {
                        v.as_inline_table()
                            .and_then(|x| x.get("pred"))
                            .and_then(Value::as_str)
                            .is_some_and(|p| ids.iter().any(|x| x == p))
                    })
                })
        })
        .filter_map(|t| t.get("id").and_then(Item::as_str).map(str::to_string))
        .collect()
}

/// `alma fmt`: the canonical text of a plan, or the syntax error.
pub fn fmt_text(src: &str) -> Result<String, Diagnostic> {
    let out = alma_format::format_text(src).map_err(|e| {
        op_err(
            "E-FMT-01",
            format!("TOML syntax error: {}", e.message()),
            &[],
        )
    })?;
    // Formatting must never change meaning; refuse rather than write a different plan.
    let strip = |text: &str| {
        alma_format::parse(text).plan.map(|mut p| {
            for t in &mut p.tasks {
                t.span = None;
            }
            p.tasks.sort_by(|a, b| a.id.cmp(&b.id));
            p
        })
    };
    if strip(src) != strip(&out) {
        return Err(op_err(
            "E-OPS-04",
            "formatting would change the meaning of this plan; it was left untouched",
            &[],
        )
        .with_hint("please report this as a bug, with the file"));
    }
    Ok(out)
}

/// Creates the text of a new plan (`alma init`).
pub fn init_text(name: &str, start: &str) -> Result<String, Diagnostic> {
    let date: toml_edit::Datetime = start.parse().map_err(|_| {
        op_err(
            "E-OPS-04",
            format!("`{start}` is not a date like 2027-01-04"),
            &[],
        )
    })?;
    let mut doc = DocumentMut::new();
    let mut p = Table::new();
    p.insert("format", value(alma_format::FORMAT_VERSION));
    p.insert("name", value(name));
    p.insert("start", value(date));
    doc.insert("project", Item::Table(p));
    let text = doc.to_string();
    let loaded = load(&text);
    if let Some(e) = loaded.diagnostics.iter().find(|d| d.code.starts_with("E-")) {
        return Err(e.clone());
    }
    Ok(text)
}
