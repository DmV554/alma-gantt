//! MCP server: the same operations as the CLI, as MCP tools over stdio (SPEC.md §5 M0.5,
//! ADR 0010).
//!
//! Mutation tools are generated from the batch schema (`alma schema`), so a tool's input is
//! exactly one CLI batch operation plus `file` and `dry_run`. Every tool returns the same JSON
//! the CLI prints with `--json`, as text and as structured content. Like the CLI, the server
//! only calls `alma-ops` (ADR 0004).

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use alma_ops::Severity;
use alma_ops::export::RenderRequest;
use alma_ops::mutate::{self, Op};
use alma_ops::query::{self, ListFilter, Page, TASK_FIELDS};
use base64::Engine;
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, Implementation,
    JsonObject, ListToolsResult, PaginatedRequestParams, ServerCapabilities, ServerConfig, Tool,
    ToolAnnotations,
};
use rmcp::service::RequestContext;
use rmcp::{ErrorData as McpError, RoleServer, ServerHandler, ServiceExt};
use serde_json::{Map, Value, json};

/// Short instructions sent to the client when it connects.
const INSTRUCTIONS: &str = "alma plans projects as Gantt charts stored in a TOML file. Call \
`guide` once for the workflow and value formats. Mutations accept `dry_run`; every result \
carries diagnostics with stable codes and hints. Use `render` to look at the chart.";

/// The server state: where to look for the plan, and a lock so writes never interleave.
#[derive(Clone)]
pub struct AlmaServer {
    dir: PathBuf,
    default_file: Option<PathBuf>,
    write_lock: Arc<Mutex<()>>,
}

/// The outcome of a tool call before it is wrapped for MCP.
#[derive(Debug, Clone, PartialEq)]
pub struct Outcome {
    /// JSON result (what the CLI prints with `--json`).
    pub json: Value,
    /// True when the call failed or the plan has errors.
    pub is_error: bool,
    /// A PNG to show the model, when the tool rendered one.
    pub png: Option<Vec<u8>>,
}

impl Outcome {
    fn ok(json: Value) -> Self {
        Self {
            json,
            is_error: false,
            png: None,
        }
    }

    fn error(code: &str, message: impl Into<String>) -> Self {
        Self {
            json: json!({ "ok": false, "errors": [{ "code": code, "severity": "error", "message": message.into() }] }),
            is_error: true,
            png: None,
        }
    }

    fn from<T: serde::Serialize>(v: &T, is_error: bool) -> Self {
        Self {
            json: serde_json::to_value(v).unwrap_or(Value::Null),
            is_error,
            png: None,
        }
    }
}

fn str_arg<'a>(args: &'a JsonObject, key: &str) -> Option<&'a str> {
    args.get(key).and_then(Value::as_str)
}

fn bool_arg(args: &JsonObject, key: &str) -> bool {
    args.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn usize_arg(args: &JsonObject, key: &str) -> Option<usize> {
    args.get(key).and_then(Value::as_u64).map(|v| v as usize)
}

fn schema(props: Value, required: &[&str]) -> JsonObject {
    let mut props = props.as_object().cloned().unwrap_or_default();
    props.insert(
        "file".into(),
        json!({ "type": "string", "description": "Plan file; defaults to the server's plan (--file, ALMA_FILE, or the only *.alma.toml in its directory)" }),
    );
    let v = json!({ "type": "object", "properties": props, "required": required, "additionalProperties": false });
    v.as_object().cloned().unwrap_or_default()
}

fn read_only(t: Tool) -> Tool {
    t.annotate(ToolAnnotations::new().read_only(true).idempotent(true))
}

/// Tool name of a batch operation: `task.add` → `task_add`.
fn tool_name(op: &str) -> String {
    op.replace('.', "_")
}

/// Replaces `{"$ref": "#/$defs/x"}` with the definition, so each tool schema stands alone.
fn inline_refs(v: &Value, defs: &Map<String, Value>) -> Value {
    match v {
        Value::Object(o) => {
            if let Some(Value::String(r)) = o.get("$ref")
                && let Some(def) = r.strip_prefix("#/$defs/").and_then(|k| defs.get(k))
            {
                return inline_refs(def, defs);
            }
            Value::Object(
                o.iter()
                    .map(|(k, x)| (k.clone(), inline_refs(x, defs)))
                    .collect(),
            )
        }
        Value::Array(a) => Value::Array(a.iter().map(|x| inline_refs(x, defs)).collect()),
        other => other.clone(),
    }
}

/// One MCP tool per batch operation, from `alma schema`: (tool name, op name, schema).
fn op_tools() -> Vec<(String, String, JsonObject)> {
    let root: Value = serde_json::from_str(alma_ops::schema::OPS_SCHEMA).unwrap_or(Value::Null);
    let defs = root["$defs"].as_object().cloned().unwrap_or_default();
    let mut out = Vec::new();
    for variant in root["$defs"]["op"]["oneOf"]
        .as_array()
        .into_iter()
        .flatten()
    {
        let Some(op) = variant["properties"]["op"]["const"].as_str() else {
            continue;
        };
        let mut props = variant["properties"]
            .as_object()
            .cloned()
            .unwrap_or_default();
        props.remove("op");
        props.insert(
            "dry_run".into(),
            json!({ "type": "boolean", "description": "Show the effect without writing" }),
        );
        let required: Vec<&str> = variant["required"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .filter(|r| *r != "op")
            .collect();
        let props = inline_refs(&Value::Object(props), &defs);
        out.push((tool_name(op), op.to_string(), schema(props, &required)));
    }
    out
}

fn op_description(op: &str) -> &'static str {
    match op {
        "task.add" => {
            "Add a task, summary or milestone (`fields`: duration, kind, wbs_role, owner, deadline, constraint, estimate, …; see `keys` with table `task`). Re-adding an identical task is a no-op."
        }
        "task.set" => "Set or unset fields of a task (`fields`, `unset`).",
        "task.rm" => "Remove a task; refuses when others depend on it unless `cascade`.",
        "task.move" => {
            "Move a task and its subtree under another parent (\"\" for top level) or after a sibling."
        }
        "dep.add" => {
            "Add a dependency pred → succ (type FS/SS/FF/SF, lag such as 2d or -1d, rationale)."
        }
        "dep.rm" => "Remove the dependency pred → succ.",
        "res.add" => "Add a resource (rate as AMOUNT/PER, e.g. 4.2/day; max_units).",
        "res.set" => {
            "Change a resource: rate (AMOUNT/PER, PER is hour, day, week, month or use), max_units, kind, name, calendar; `unset` removes keys."
        }
        "assign" => "Assign a resource to a task with units (1 = full time).",
        "unassign" => "Remove an assignment.",
        "cadence.add" => "Add a sprint cadence: COUNT equal iterations from START, ids <prefix>01…",
        "cadence.rm" => "Remove a cadence.",
        "project.set" => "Set project keys: name, calendar, deadline, currency, description.",
        "cal.add" => "Add a working calendar (workdays, hours like 09:00-13:00).",
        "cal.except" => "Add a holiday or blackout period to a calendar (dates inclusive).",
        "rule.set" => "Set a lint rule's severity for this plan (off, info, warn, error).",
        "reserve.add" => {
            "Add a cost reserve: kind `contingency` (identified risks; inside the cost baseline, accrues at the finish of its scope) or `management` (the unforeseen; outside the baseline, inside the budget). Give a `basis`."
        }
        "reserve.rm" => "Remove a cost reserve.",
        "level" => {
            "Resolve resource over-allocation: mode `leveling` delays tasks until their resources fit (the finish may move), `smoothing` only within free float, `off` turns it off. Saved in the plan; the result lists delayed tasks and what each waits for."
        }
        "settings.set" => {
            "Set or unset scheduling settings ([settings.schedule]): hours_per_day, days_per_week, late_anchor, critical_threshold, near_critical, phase, leveling, …"
        }
        _ => "Plan operation.",
    }
}

/// Every tool the server offers, in a stable order.
pub fn tools() -> Vec<Tool> {
    let mut t = vec![
        read_only(Tool::new(
            "guide",
            "The usage guide: workflow, commands and value formats (durations like 15d, 3ed). Read it first.",
            schema(json!({}), &[]),
        )),
        read_only(Tool::new(
            "check",
            "Validate, schedule and lint the plan. Returns counts and diagnostics with code, ids, hint.",
            schema(
                json!({
                    "min": { "enum": ["info", "warn", "error"], "description": "Lowest severity to list (default info)" },
                    "limit": { "type": "integer", "minimum": 1 },
                    "cursor": { "type": "string" },
                }),
                &[],
            ),
        )),
        read_only(Tool::new(
            "list_tasks",
            "List tasks with computed dates, paginated.",
            schema(
                json!({
                    "parent": { "type": "string", "description": "Only direct children (\"\" for top level)" },
                    "critical": { "type": "boolean" },
                    "kind": { "enum": ["task", "milestone", "loe", "buffer", "summary"] },
                    "fields": { "type": "array", "items": { "enum": TASK_FIELDS } },
                    "limit": { "type": "integer", "minimum": 1 },
                    "cursor": { "type": "string" },
                }),
                &[],
            ),
        )),
        read_only(Tool::new(
            "show_task",
            "One task with its computed dates and floats (working days).",
            schema(json!({ "id": { "type": "string" } }), &["id"]),
        )),
        read_only(Tool::new(
            "critical_path",
            "The critical tasks ordered by start, and the project finish date.",
            schema(json!({}), &[]),
        )),
        read_only(Tool::new(
            "explain",
            "Why a task starts when it does: the chain of driving links, constraints and calendar exceptions.",
            schema(json!({ "id": { "type": "string" } }), &["id"]),
        )),
        read_only(Tool::new(
            "list_resources",
            "Resources with their kind, capacity, rate, assigned tasks, booked hours and how many over-allocated stretches each has.",
            schema(json!({}), &[]),
        )),
        read_only(Tool::new(
            "resource_usage",
            "Demand vs capacity of one resource per period (hours), and the exact stretches where it is over-allocated with the competing tasks. Use it to check a fix.",
            schema(
                json!({
                    "id": { "type": "string", "description": "Resource id" },
                    "period": { "enum": ["day", "week", "month"], "description": "Default week" },
                    "limit": { "type": "integer", "minimum": 1 },
                    "cursor": { "type": "string" },
                }),
                &["id"],
            ),
        )),
        read_only(Tool::new(
            "cost",
            "Cost of the current schedule (units × working hours × rate + per-use + fixed costs), grouped by WBS branch, resource or category, with each row's share of the total.",
            schema(
                json!({
                    "by": { "enum": ["wbs", "resource", "category"], "description": "Default wbs" },
                    "depth": { "type": "integer", "minimum": 1, "description": "Deepest WBS level with by=wbs (default 1)" },
                }),
                &[],
            ),
        )),
        read_only(Tool::new(
            "scurve",
            "Cost per period and cumulative for the current schedule (a forecast), with planned value (pv) from the active baseline when there is one.",
            schema(
                json!({
                    "period": { "enum": ["day", "week", "month"], "description": "Default month" },
                    "baseline": { "type": "string", "description": "Baseline for pv (default: the active one)" },
                }),
                &[],
            ),
        )),
        read_only(Tool::new(
            "baseline_list",
            "Saved baselines with the finish date and budget each one promised; one is active.",
            schema(json!({}), &[]),
        )),
        read_only(Tool::new(
            "variance",
            "How the current schedule differs from a baseline: finish and cost, and per task start/finish variance in working days and cost variance.",
            schema(
                json!({
                    "baseline": { "type": "string", "description": "Default: the active one" },
                    "changed": { "type": "boolean", "description": "Only tasks that moved, changed cost or are new" },
                    "limit": { "type": "integer", "minimum": 1 },
                    "cursor": { "type": "string" },
                }),
                &[],
            ),
        )),
        Tool::new(
            "baseline_save",
            "Freeze the current schedule (dates, costs, planned value) as a new active baseline. Never overwrites an existing one; a revision should carry a change reference.",
            schema(
                json!({
                    "id": { "type": "string" },
                    "name": { "type": "string" },
                    "change_ref": { "type": "string", "description": "Change request that approved it" },
                    "reason": { "type": "string" },
                    "approved_by": { "type": "string" },
                    "date": { "type": "string", "description": "YYYY-MM-DD (default today)" },
                    "dry_run": { "type": "boolean" },
                }),
                &["id"],
            ),
        )
        .annotate(ToolAnnotations::new().read_only(false).destructive(false).idempotent(false)),
        read_only(Tool::new(
            "diff",
            "What moved compared with another version of the plan: a file or a git revision.",
            schema(
                json!({
                    "against": { "type": "string", "description": "Older plan file" },
                    "rev": { "type": "string", "description": "Git revision of the same file (default HEAD)" },
                }),
                &[],
            ),
        )),
        read_only(Tool::new(
            "keys",
            "The keys a table accepts (task, project, calendar, resource, cadence, task.deps, …) and which are reserved. Without `table`, lists tables.",
            schema(json!({ "table": { "type": "string" } }), &[]),
        )),
        read_only(Tool::new(
            "render",
            "Render a chart and return it as an image (PNG) so you can look at it, or as SVG text: the Gantt chart (default), a resource histogram (demand vs capacity) or the S-curve (forecast and planned value).",
            schema(
                json!({
                    "chart": { "enum": ["gantt", "histogram", "scurve"], "description": "Default gantt" },
                    "resource": { "type": "string", "description": "Resource id, for histogram" },
                    "period": { "enum": ["day", "week", "month"], "description": "histogram (default week) and scurve (default month)" },
                    "baseline": { "type": "string", "description": "scurve: baseline for planned value (default the active one)" },
                    "format": { "enum": ["png", "svg"], "description": "Default png" },
                    "range": { "type": "string", "description": "e.g. 2027-01..2027-06" },
                    "depth": { "type": "integer", "minimum": 1 },
                    "scale": { "enum": ["day", "week", "month"] },
                    "dark": { "type": "boolean" },
                }),
                &[],
            ),
        )),
        Tool::new(
            "export",
            "Write a chart (.svg, .png: gantt, histogram or scurve) or the task table (.csv) to a file.",
            schema(
                json!({
                    "out": { "type": "string" },
                    "chart": { "enum": ["gantt", "histogram", "scurve"] },
                    "resource": { "type": "string" },
                    "period": { "enum": ["day", "week", "month"] },
                    "baseline": { "type": "string" },
                    "range": { "type": "string" },
                    "depth": { "type": "integer", "minimum": 1 },
                    "scale": { "enum": ["day", "week", "month"] },
                    "dark": { "type": "boolean" },
                }),
                &["out"],
            ),
        )
        .annotate(ToolAnnotations::new().read_only(false).destructive(false).idempotent(true)),
        Tool::new(
            "init",
            "Create a new plan file.",
            schema(
                json!({
                    "path": { "type": "string" },
                    "name": { "type": "string" },
                    "start": { "type": "string", "description": "YYYY-MM-DD" },
                }),
                &["path", "name", "start"],
            ),
        )
        .annotate(ToolAnnotations::new().read_only(false).destructive(false)),
        Tool::new(
            "apply",
            "Run many operations in one all-or-nothing transaction. Each item is a batch operation as in `alma schema` (e.g. {\"op\": \"task.add\", ...}).",
            schema(
                json!({
                    "ops": { "type": "array", "items": { "type": "object" }, "minItems": 1 },
                    "dry_run": { "type": "boolean" },
                }),
                &["ops"],
            ),
        )
        .annotate(ToolAnnotations::new().read_only(false).destructive(true)),
    ];
    for (name, op, input) in op_tools() {
        let destructive = op.ends_with(".rm") || op == "unassign";
        t.push(
            Tool::new(name, op_description(&op), input).annotate(
                ToolAnnotations::new()
                    .read_only(false)
                    .destructive(destructive)
                    .idempotent(!destructive),
            ),
        );
    }
    t
}

impl AlmaServer {
    /// A server working in `dir`, with an optional default plan file.
    pub fn new(dir: PathBuf, default_file: Option<PathBuf>) -> Self {
        Self {
            dir,
            default_file,
            write_lock: Arc::new(Mutex::new(())),
        }
    }

    fn plan_path(&self, args: &JsonObject) -> Result<PathBuf, Outcome> {
        let explicit = str_arg(args, "file")
            .map(PathBuf::from)
            .or_else(|| self.default_file.clone());
        alma_ops::files::resolve_plan(explicit.as_deref(), &self.dir)
            .map_err(|m| Outcome::error("E-CLI-01", m))
    }

    fn path_arg(&self, args: &JsonObject, key: &str) -> Option<PathBuf> {
        str_arg(args, key).map(|p| {
            let p = Path::new(p);
            if p.is_absolute() {
                p.to_path_buf()
            } else {
                self.dir.join(p)
            }
        })
    }

    fn load(&self, args: &JsonObject) -> Result<(PathBuf, alma_ops::Loaded), Outcome> {
        let path = self.plan_path(args)?;
        let src = alma_ops::files::read_text(&path).map_err(|m| Outcome::error("E-CLI-01", m))?;
        Ok((path, alma_ops::load(&src)))
    }

    /// Like `load`, but fails with the plan's errors when it cannot be scheduled.
    fn scheduled(&self, args: &JsonObject) -> Result<(PathBuf, alma_ops::Loaded), Outcome> {
        let (path, l) = self.load(args)?;
        if l.plan.is_none() || l.schedule.is_none() {
            let errors: Vec<_> = l
                .diagnostics
                .iter()
                .filter(|d| d.code.starts_with("E-"))
                .cloned()
                .collect();
            return Err(Outcome {
                json: json!({ "ok": false, "message": "the plan cannot be scheduled", "errors": errors }),
                is_error: true,
                png: None,
            });
        }
        Ok((path, l))
    }

    fn mutate(&self, args: &JsonObject, ops: &[Op]) -> Outcome {
        let path = match self.plan_path(args) {
            Ok(p) => p,
            Err(e) => return e,
        };
        let _guard = self.write_lock.lock().unwrap_or_else(|p| p.into_inner());
        match mutate::apply_to_file(&path, ops, bool_arg(args, "dry_run")) {
            Ok(report) => {
                let failed = !report.ok;
                Outcome::from(&report, failed)
            }
            Err(e) => Outcome::error("E-CLI-01", format!("cannot update {}: {e}", path.display())),
        }
    }

    fn render_request(
        args: &JsonObject,
        plan: &alma_ops::Plan,
        path: &Path,
    ) -> Result<RenderRequest, Outcome> {
        let chart = str_arg(args, "chart").map(str::to_string);
        let baseline = if chart.as_deref() == Some("scurve") {
            alma_ops::baseline::snapshot_for(plan, path, str_arg(args, "baseline"))
                .map_err(|m| Outcome::error("E-CLI-01", m))?
        } else {
            None
        };
        Ok(RenderRequest {
            range: str_arg(args, "range").map(str::to_string),
            depth: usize_arg(args, "depth"),
            scale: str_arg(args, "scale").map(str::to_string),
            dark: bool_arg(args, "dark"),
            no_arrows: false,
            font_size: None,
            chart,
            resource: str_arg(args, "resource").map(str::to_string),
            period: str_arg(args, "period").map(str::to_string),
            baseline,
        })
    }

    /// Runs a tool. Synchronous and transport-free, so it is easy to test.
    pub fn call(&self, name: &str, args: &JsonObject) -> Outcome {
        match self.try_call(name, args) {
            Ok(o) | Err(o) => o,
        }
    }

    fn try_call(&self, name: &str, args: &JsonObject) -> Result<Outcome, Outcome> {
        Ok(match name {
            "guide" => Outcome::ok(json!({ "guide": alma_ops::files::GUIDE })),
            "check" => {
                let (_, l) = self.load(args)?;
                let min = match str_arg(args, "min") {
                    Some("warn") => Severity::Warn,
                    Some("error") => Severity::Error,
                    _ => Severity::Info,
                };
                let page = Page::new(usize_arg(args, "limit"), str_arg(args, "cursor"));
                let report = query::check(&l, min, page);
                Outcome::from(&report, !report.ok)
            }
            "list_tasks" => {
                let (_, l) = self.scheduled(args)?;
                let fields: Vec<String> = match args.get("fields").and_then(Value::as_array) {
                    Some(a) => a
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect(),
                    None => ["id", "name", "start", "finish", "tf"]
                        .map(String::from)
                        .to_vec(),
                };
                if let Some(bad) = fields.iter().find(|f| !TASK_FIELDS.contains(&f.as_str())) {
                    return Err(Outcome::error(
                        "E-CLI-01",
                        format!("unknown field `{bad}`; fields: {}", TASK_FIELDS.join(",")),
                    ));
                }
                let filter = ListFilter {
                    parent: str_arg(args, "parent").map(str::to_string),
                    critical: bool_arg(args, "critical"),
                    kind: str_arg(args, "kind").map(str::to_string),
                };
                let page = Page::new(usize_arg(args, "limit"), str_arg(args, "cursor"));
                let (plan, s) = (l.plan.as_ref().unwrap(), l.schedule.as_ref().unwrap());
                Outcome::from(&query::list(plan, s, &filter, &fields, page), false)
            }
            "show_task" => {
                let (_, l) = self.scheduled(args)?;
                let id = str_arg(args, "id").unwrap_or_default();
                let (plan, s) = (l.plan.as_ref().unwrap(), l.schedule.as_ref().unwrap());
                match query::task_views(plan, s).into_iter().find(|v| v.id == id) {
                    Some(v) => Outcome::from(&v, false),
                    None => Outcome::error("E-OPS-02", format!("no task with id `{id}`")),
                }
            }
            "critical_path" => {
                let (_, l) = self.scheduled(args)?;
                let (plan, s) = (l.plan.as_ref().unwrap(), l.schedule.as_ref().unwrap());
                let finish = alma_ops::project_finish(s);
                Outcome::ok(json!({
                    "schema": alma_ops::OUTPUT_SCHEMA,
                    "project_finish": finish,
                    "critical": query::critical(plan, s),
                }))
            }
            "explain" => {
                let (_, l) = self.scheduled(args)?;
                let id = str_arg(args, "id").unwrap_or_default();
                let (plan, s) = (l.plan.as_ref().unwrap(), l.schedule.as_ref().unwrap());
                match query::explain(plan, s, id) {
                    Some(chain) => {
                        Outcome::ok(json!({ "schema": alma_ops::OUTPUT_SCHEMA, "chain": chain }))
                    }
                    None => Outcome::error("E-OPS-02", format!("no task with id `{id}`")),
                }
            }
            "list_resources" => {
                let (_, l) = self.scheduled(args)?;
                let (plan, s) = (l.plan.as_ref().unwrap(), l.schedule.as_ref().unwrap());
                Outcome::ok(json!({
                    "schema": alma_ops::OUTPUT_SCHEMA,
                    "resources": alma_ops::resources::list(plan, s),
                }))
            }
            "resource_usage" => {
                let (_, l) = self.scheduled(args)?;
                let (plan, s) = (l.plan.as_ref().unwrap(), l.schedule.as_ref().unwrap());
                let id = str_arg(args, "id").unwrap_or_default();
                let period = str_arg(args, "period").unwrap_or("week");
                let mut v = alma_ops::resources::usage(plan, s, id, period)
                    .map_err(|m| Outcome::error("E-CLI-01", m))?;
                let page = Page::new(usize_arg(args, "limit"), str_arg(args, "cursor"));
                let total = v.periods.len();
                let (items, next) = page.apply(&v.periods);
                v.periods = items;
                let mut out = serde_json::to_value(&v).unwrap_or(Value::Null);
                out["total"] = total.into();
                if let Some(n) = next {
                    out["next_cursor"] = n.into();
                }
                Outcome::ok(out)
            }
            "cost" => {
                let (_, l) = self.scheduled(args)?;
                let (plan, s) = (l.plan.as_ref().unwrap(), l.schedule.as_ref().unwrap());
                let by = str_arg(args, "by").unwrap_or("wbs");
                let depth = usize_arg(args, "depth").unwrap_or(1);
                let r = alma_ops::cost::report(plan, s, by, depth)
                    .map_err(|m| Outcome::error("E-CLI-01", m))?;
                Outcome::from(&r, false)
            }
            "scurve" => {
                let (_, l) = self.scheduled(args)?;
                let (plan, s) = (l.plan.as_ref().unwrap(), l.schedule.as_ref().unwrap());
                let (path, _) = self.load(args)?;
                let want = str_arg(args, "baseline");
                let snap = if want.is_none() && !plan.baselines.iter().any(|b| b.active) {
                    None
                } else {
                    let id = alma_ops::baseline::pick(plan, want)
                        .map_err(|m| Outcome::error("E-CLI-01", m))?;
                    Some(
                        alma_ops::baseline::read_snapshot(&path, id)
                            .map_err(|m| Outcome::error("E-CLI-01", m))?,
                    )
                };
                let r = alma_ops::cost::scurve(
                    plan,
                    s,
                    str_arg(args, "period").unwrap_or("month"),
                    snap.as_ref(),
                )
                .map_err(|m| Outcome::error("E-CLI-01", m))?;
                Outcome::from(&r, false)
            }
            "baseline_list" => {
                let (path, l) = self.load(args)?;
                let Some(plan) = l.plan.as_ref() else {
                    return Err(Outcome::error("E-OPS-04", "the plan file has errors"));
                };
                Outcome::ok(json!({
                    "schema": alma_ops::OUTPUT_SCHEMA,
                    "baselines": alma_ops::baseline::list(plan, &path),
                }))
            }
            "variance" => {
                let (path, l) = self.scheduled(args)?;
                let (plan, s) = (l.plan.as_ref().unwrap(), l.schedule.as_ref().unwrap());
                let id = alma_ops::baseline::pick(plan, str_arg(args, "baseline"))
                    .map_err(|m| Outcome::error("E-CLI-01", m))?;
                let snap = alma_ops::baseline::read_snapshot(&path, id)
                    .map_err(|m| Outcome::error("E-CLI-01", m))?;
                let page = Page::new(usize_arg(args, "limit"), str_arg(args, "cursor"));
                Outcome::from(
                    &alma_ops::baseline::variance(plan, s, &snap, bool_arg(args, "changed"), page),
                    false,
                )
            }
            "baseline_save" => {
                let path = self.plan_path(args)?;
                let id = str_arg(args, "id").unwrap_or_default().to_string();
                let s = |k| str_arg(args, k).map(str::to_string);
                let meta = alma_ops::mutate::BaselineMeta {
                    id,
                    name: s("name"),
                    date: s("date").unwrap_or_else(|| alma_ops::files::today().to_string()),
                    change_ref: s("change_ref"),
                    reason: s("reason"),
                    approved_by: s("approved_by"),
                };
                let _guard = self.write_lock.lock().unwrap_or_else(|p| p.into_inner());
                match alma_ops::baseline::save(&path, &meta, bool_arg(args, "dry_run")) {
                    Ok(r) => Outcome::from(&r, !r.report.ok),
                    Err(e) => Outcome::error("E-CLI-01", format!("cannot save the baseline: {e}")),
                }
            }
            "diff" => {
                let path = self.plan_path(args)?;
                let current =
                    alma_ops::files::read_text(&path).map_err(|m| Outcome::error("E-CLI-01", m))?;
                let old = match self.path_arg(args, "against") {
                    Some(f) => alma_ops::files::read_text(&f),
                    None => {
                        alma_ops::files::git_show(&path, str_arg(args, "rev").unwrap_or("HEAD"))
                    }
                }
                .map_err(|m| Outcome::error("E-CLI-01", m))?;
                let report = alma_ops::diff::diff(&alma_ops::load(&old), &alma_ops::load(&current));
                Outcome::from(&report, false)
            }
            "keys" => {
                let mut tables: Vec<&str> = alma_ops::keys::KEYS.iter().map(|k| k.table).collect();
                tables.dedup();
                match str_arg(args, "table") {
                    None => Outcome::ok(json!({ "tables": tables })),
                    Some(t) if tables.contains(&t) => {
                        let keys: Vec<Value> = alma_ops::keys::keys_of(t)
                            .map(|k| {
                                let status = match k.tier {
                                    alma_ops::keys::Tier::A => "active".to_string(),
                                    alma_ops::keys::Tier::B => "accepted".to_string(),
                                    alma_ops::keys::Tier::R(m) => format!("reserved for {m}"),
                                };
                                json!({ "key": k.key, "status": status })
                            })
                            .collect();
                        Outcome::ok(json!({ "table": t, "keys": keys }))
                    }
                    Some(t) => Outcome::error(
                        "E-CLI-01",
                        format!("unknown table `{t}`; tables: {}", tables.join(", ")),
                    ),
                }
            }
            "render" => {
                let (path, l) = self.scheduled(args)?;
                let (plan, s) = (l.plan.as_ref().unwrap(), l.schedule.as_ref().unwrap());
                let req = Self::render_request(args, plan, &path)?;
                let warnings = alma_ops::export::warnings(plan, s, &req)
                    .map_err(|m| Outcome::error("E-CLI-01", m))?;
                match str_arg(args, "format").unwrap_or("png") {
                    "svg" => {
                        let svg = alma_ops::export::svg(plan, s, &req)
                            .map_err(|m| Outcome::error("E-CLI-01", m))?;
                        Outcome::ok(json!({ "format": "svg", "svg": svg, "warnings": warnings }))
                    }
                    "png" => {
                        let png = alma_ops::export::png(plan, s, &req)
                            .map_err(|m| Outcome::error("E-CLI-01", m))?;
                        Outcome {
                            json: json!({ "format": "png", "bytes": png.len(), "warnings": warnings }),
                            is_error: false,
                            png: Some(png),
                        }
                    }
                    other => Outcome::error(
                        "E-CLI-01",
                        format!("unknown format `{other}`; use png or svg"),
                    ),
                }
            }
            "export" => {
                let (path, l) = self.scheduled(args)?;
                let (plan, s) = (l.plan.as_ref().unwrap(), l.schedule.as_ref().unwrap());
                let out = self
                    .path_arg(args, "out")
                    .ok_or_else(|| Outcome::error("E-CLI-01", "`out` is required"))?;
                let format = out
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("svg")
                    .to_ascii_lowercase();
                let req = Self::render_request(args, plan, &path)?;
                let bytes = match format.as_str() {
                    "svg" => alma_ops::export::svg(plan, s, &req).map(String::into_bytes),
                    "png" => alma_ops::export::png(plan, s, &req),
                    "csv" => Ok(alma_ops::export::csv(plan, s).into_bytes()),
                    other => Err(format!("unknown format `{other}`; use .svg, .png or .csv")),
                }
                .map_err(|m| Outcome::error("E-CLI-01", m))?;
                alma_ops::files::write_bytes(&out, &bytes)
                    .map_err(|m| Outcome::error("E-CLI-01", m))?;
                Outcome::ok(
                    json!({ "ok": true, "file": out, "format": format, "bytes": bytes.len() }),
                )
            }
            "init" => {
                let path = self
                    .path_arg(args, "path")
                    .ok_or_else(|| Outcome::error("E-CLI-01", "`path` is required"))?;
                if path.exists() {
                    return Err(Outcome::error(
                        "E-CLI-01",
                        format!("{} already exists", path.display()),
                    ));
                }
                let text = mutate::init_text(
                    str_arg(args, "name").unwrap_or_default(),
                    str_arg(args, "start").unwrap_or_default(),
                )
                .map_err(|d| Outcome::error(d.code, d.message))?;
                let _guard = self.write_lock.lock().unwrap_or_else(|p| p.into_inner());
                mutate::write_atomic(&path, &text).map_err(|e| {
                    Outcome::error("E-CLI-01", format!("cannot write {}: {e}", path.display()))
                })?;
                Outcome::ok(json!({ "ok": true, "file": path }))
            }
            "apply" => {
                let ops: Vec<Op> =
                    serde_json::from_value(args.get("ops").cloned().unwrap_or(Value::Null))
                        .map_err(|e| {
                            Outcome::error("E-CLI-01", format!("invalid operations: {e}"))
                        })?;
                self.mutate(args, &ops)
            }
            other => {
                let Some((_, op, _)) = op_tools().into_iter().find(|(n, _, _)| n == other) else {
                    return Err(Outcome::error(
                        "E-CLI-01",
                        format!("unknown tool `{other}`"),
                    ));
                };
                let mut v: Map<String, Value> = args
                    .iter()
                    .filter(|(k, _)| *k != "file" && *k != "dry_run")
                    .map(|(k, x)| (k.clone(), x.clone()))
                    .collect();
                v.insert("op".into(), Value::String(op));
                let op: Op = serde_json::from_value(Value::Object(v))
                    .map_err(|e| Outcome::error("E-CLI-01", format!("invalid arguments: {e}")))?;
                self.mutate(args, &[op])
            }
        })
    }
}

/// Wraps an outcome as an MCP tool result: the JSON as text and structured content, plus the
/// image when there is one.
pub fn to_result(o: Outcome) -> CallToolResult {
    let text = serde_json::to_string_pretty(&o.json).unwrap_or_default();
    let mut content = vec![ContentBlock::text(text)];
    if let Some(png) = &o.png {
        let data = base64::engine::general_purpose::STANDARD.encode(png);
        content.push(ContentBlock::image(data, "image/png"));
    }
    let mut r = if o.is_error {
        CallToolResult::error(content)
    } else {
        CallToolResult::success(content)
    };
    if o.json.is_object() {
        r.structured_content = Some(o.json);
    }
    r
}

impl ServerHandler for AlmaServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("alma", alma_ops::VERSION))
            .with_instructions(INSTRUCTIONS)
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        Ok(ListToolsResult::with_all_items(tools()))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        let args = request.arguments.unwrap_or_default();
        let server = self.clone();
        let name = request.name.to_string();
        // Scheduling and rendering are CPU work; keep them off the protocol task.
        let outcome = tokio::task::spawn_blocking(move || server.call(&name, &args))
            .await
            .unwrap_or_else(|e| Outcome::error("E-CLI-01", format!("internal error: {e}")));
        Ok(CallToolResponse::Complete(to_result(outcome)))
    }
}

/// Serves MCP over stdin/stdout until the client disconnects.
pub fn serve_stdio(server: AlmaServer) -> std::io::Result<()> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    rt.block_on(async move {
        let service = server
            .serve(rmcp::transport::stdio())
            .await
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        service
            .waiting()
            .await
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        Ok(())
    })
}
