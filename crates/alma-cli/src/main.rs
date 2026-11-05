//! `alma` command-line interface (SPEC.md §8a).
//!
//! Exit codes: 0 ok, 1 the plan or the change has errors, 2 invalid command or unreadable file.

use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use alma_core::diag::{Diagnostic, Severity};
use alma_ops::mutate::{self, FieldValue, MutationReport, Op};
use alma_ops::query::{self, ListFilter, Page, TASK_FIELDS};
use clap::{Args, Parser, Subcommand, ValueEnum};
use serde::Serialize;

use alma_ops::files::GUIDE;

#[derive(Parser)]
#[command(name = "alma", bin_name = "alma", version = alma_ops::VERSION, about = "Gantt planning for agents and humans")]
struct Cli {
    /// Plan file. Defaults to $ALMA_FILE, or the only *.alma.toml in the current directory.
    #[arg(short, long, global = true)]
    file: Option<PathBuf>,
    /// Print JSON instead of text.
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    cmd: Cmd,
}

// Built once per run from the command line; boxing variants would only add noise.
#[allow(clippy::large_enum_variant)]
#[derive(Subcommand)]
enum Cmd {
    /// Create a new plan file.
    Init {
        /// Path of the new file.
        path: PathBuf,
        /// Project name.
        #[arg(long)]
        name: String,
        /// Project start date (YYYY-MM-DD).
        #[arg(long)]
        start: String,
    },
    /// Validate, schedule and lint the plan.
    Check {
        /// Only show diagnostics at or above this severity.
        #[arg(long, value_enum, default_value = "info")]
        min: MinSeverity,
        #[command(flatten)]
        page: PageArgs,
    },
    /// Show one task with its computed dates.
    Show {
        /// Task id.
        id: String,
    },
    /// List tasks.
    Ls {
        /// Only direct children of this task ("" for top level).
        #[arg(long)]
        parent: Option<String>,
        /// Only critical tasks.
        #[arg(long)]
        critical: bool,
        /// Only this kind (task, milestone, loe, buffer, summary).
        #[arg(long)]
        kind: Option<String>,
        /// Comma-separated fields.
        #[arg(long, default_value = "id,name,start,finish,tf")]
        fields: String,
        #[command(flatten)]
        page: PageArgs,
    },
    /// The critical path, ordered by start.
    Critical,
    /// Why a task starts when it does: the chain of drivers.
    Explain {
        /// Task id.
        id: String,
    },
    /// Task commands.
    #[command(subcommand)]
    Task(TaskCmd),
    /// Dependency commands.
    #[command(subcommand)]
    Dep(DepCmd),
    /// Resource commands.
    #[command(subcommand)]
    Res(ResCmd),
    /// Assign a resource to a task.
    Assign {
        /// Task id.
        task: String,
        /// Resource id.
        resource: String,
        /// Units (1 = full time).
        #[arg(long)]
        units: Option<String>,
        #[command(flatten)]
        m: MutArgs,
    },
    /// Remove an assignment.
    Unassign {
        /// Task id.
        task: String,
        /// Resource id.
        resource: String,
        #[command(flatten)]
        m: MutArgs,
    },
    /// Run a batch of operations (JSON array or JSON lines; `-` reads stdin), all or nothing.
    Apply {
        /// Operations file.
        ops: PathBuf,
        #[command(flatten)]
        m: MutArgs,
    },
    /// Export the Gantt chart (.svg, .png) or the task table (.csv); `-` writes SVG or CSV
    /// to stdout. `render` is an alias.
    #[command(alias = "render")]
    Export {
        /// Output file; the extension picks the format unless --format is given.
        out: PathBuf,
        /// svg, png or csv.
        #[arg(long)]
        format: Option<String>,
        /// Time range, e.g. 2027-01..2027-06 or 2027-01-04..2027-03-31.
        #[arg(long)]
        range: Option<String>,
        /// Deepest WBS level shown (1 = top level only).
        #[arg(long)]
        depth: Option<usize>,
        /// day, week or month (default: from the range length).
        #[arg(long)]
        scale: Option<String>,
        /// Dark theme.
        #[arg(long)]
        dark: bool,
        /// Hide dependency arrows.
        #[arg(long)]
        no_arrows: bool,
        /// Base font size in points (minimum 9).
        #[arg(long)]
        font_size: Option<f64>,
    },
    /// Rewrite the plan in canonical layout (key order, WBS order, spacing); comments are kept.
    Fmt {
        /// Only report whether the file is formatted (exit 1 if not).
        #[arg(long)]
        check: bool,
    },
    /// Compare the plan with another version: a file or a git revision.
    Diff {
        /// Older plan file.
        #[arg(long, conflicts_with = "rev")]
        against: Option<PathBuf>,
        /// Git revision of the same file, e.g. HEAD or main.
        #[arg(long)]
        rev: Option<String>,
    },
    /// Iteration (sprint) cadences.
    #[command(subcommand, alias = "cadence")]
    Iter(IterCmd),
    /// Working calendars and holidays.
    #[command(subcommand)]
    Cal(CalCmd),
    /// Project settings.
    #[command(subcommand)]
    Project(ProjectCmd),
    /// Lint rule configuration for this plan.
    #[command(subcommand)]
    Rule(RuleCmd),
    /// List the keys a table accepts (task, project, calendar, resource, cadence, task.deps …).
    Keys {
        /// Table; omit to list tables.
        table: Option<String>,
    },
    /// Print the usage guide for LLM agents.
    Guide,
    /// Print the JSON Schema of batch operations.
    Schema,
}

#[derive(Subcommand)]
enum TaskCmd {
    /// Add a task.
    Add {
        /// Task id.
        id: String,
        /// Name.
        #[arg(long)]
        name: String,
        /// Parent task.
        #[arg(long)]
        parent: Option<String>,
        /// Place after this sibling.
        #[arg(long)]
        after: Option<String>,
        #[command(flatten)]
        fields: TaskFields,
        /// Update the task when the id already exists.
        #[arg(long)]
        upsert: bool,
        #[command(flatten)]
        m: MutArgs,
    },
    /// Change task fields.
    Set {
        /// Task id.
        id: String,
        /// New name.
        #[arg(long)]
        name: Option<String>,
        #[command(flatten)]
        fields: TaskFields,
        /// Remove a key (repeatable).
        #[arg(long)]
        unset: Vec<String>,
        #[command(flatten)]
        m: MutArgs,
    },
    /// Remove a task. Refuses when others depend on it, unless --cascade.
    Rm {
        /// Task id.
        id: String,
        /// Also remove its subtree and every link to removed tasks.
        #[arg(long)]
        cascade: bool,
        #[command(flatten)]
        m: MutArgs,
    },
    /// Move a task and its subtree.
    Move {
        /// Task id.
        id: String,
        /// New parent ("" for top level).
        #[arg(long)]
        parent: Option<String>,
        /// Place after this sibling.
        #[arg(long)]
        after: Option<String>,
        #[command(flatten)]
        m: MutArgs,
    },
}

#[derive(Subcommand)]
enum DepCmd {
    /// Add a dependency PRED → SUCC.
    Add {
        /// Predecessor.
        pred: String,
        /// Successor.
        succ: String,
        /// FS, SS, FF or SF.
        #[arg(long = "type")]
        kind: Option<String>,
        /// Lag, e.g. 2d, -1d, 3ed.
        #[arg(long)]
        lag: Option<String>,
        /// mandatory or discretionary.
        #[arg(long)]
        nature: Option<String>,
        /// internal or external.
        #[arg(long)]
        scope: Option<String>,
        /// Why the link (or lag) exists.
        #[arg(long)]
        rationale: Option<String>,
        #[command(flatten)]
        m: MutArgs,
    },
    /// Remove the dependency PRED → SUCC.
    Rm {
        /// Predecessor.
        pred: String,
        /// Successor.
        succ: String,
        #[command(flatten)]
        m: MutArgs,
    },
}

#[derive(Subcommand)]
enum CalCmd {
    /// Add a calendar. Use it with `project set --calendar ID` or `task set --field calendar=ID`.
    Add {
        /// Calendar id.
        id: String,
        /// Name.
        #[arg(long)]
        name: Option<String>,
        /// Working weekdays, comma-separated (default mon,tue,wed,thu,fri).
        #[arg(long, value_delimiter = ',')]
        workdays: Option<Vec<String>>,
        /// Working intervals, comma-separated (default 09:00-13:00,14:00-18:00).
        #[arg(long, value_delimiter = ',')]
        hours: Option<Vec<String>>,
        #[command(flatten)]
        m: MutArgs,
    },
    /// Add a holiday or blackout period (non-working unless --working).
    Except {
        /// Calendar id.
        calendar: String,
        /// Name, shown by `explain`.
        #[arg(long)]
        name: String,
        /// First day (YYYY-MM-DD).
        #[arg(long)]
        from: String,
        /// Last day, inclusive (default: same as --from).
        #[arg(long)]
        to: Option<String>,
        /// Extra working time instead of a holiday.
        #[arg(long)]
        working: bool,
        #[command(flatten)]
        m: MutArgs,
    },
}

#[derive(Subcommand)]
enum ProjectCmd {
    /// Set project keys.
    Set {
        /// Project name.
        #[arg(long)]
        name: Option<String>,
        /// Default calendar id.
        #[arg(long)]
        calendar: Option<String>,
        /// Project deadline (YYYY-MM-DD).
        #[arg(long)]
        deadline: Option<String>,
        /// Currency label.
        #[arg(long)]
        currency: Option<String>,
        #[command(flatten)]
        m: MutArgs,
    },
}

#[derive(Subcommand)]
enum RuleCmd {
    /// Set a lint rule's severity for the whole plan (off, info, warn, error).
    Set {
        /// Lint code, e.g. L-WBS-05.
        code: String,
        /// off, info, warn or error.
        severity: String,
        #[command(flatten)]
        m: MutArgs,
    },
}

#[derive(Subcommand)]
enum IterCmd {
    /// Add a cadence of COUNT equal iterations; ids are <prefix>01, <prefix>02, …
    Add {
        /// Cadence id.
        id: String,
        /// First iteration start (YYYY-MM-DD).
        #[arg(long)]
        start: String,
        /// Iteration length, e.g. 10d or 2w.
        #[arg(long)]
        length: String,
        /// Number of iterations.
        #[arg(long = "repeat")]
        count: u32,
        /// Band label.
        #[arg(long)]
        name: Option<String>,
        /// Prefix of generated iteration ids (default: the id).
        #[arg(long)]
        prefix: Option<String>,
        /// Calendar id.
        #[arg(long)]
        calendar: Option<String>,
        #[command(flatten)]
        m: MutArgs,
    },
    /// Remove a cadence.
    Rm {
        /// Cadence id.
        id: String,
        #[command(flatten)]
        m: MutArgs,
    },
}

#[derive(Subcommand)]
enum ResCmd {
    /// Add a resource.
    Add {
        /// Resource id.
        id: String,
        /// Name.
        #[arg(long)]
        name: Option<String>,
        /// work, equipment, material or cost.
        #[arg(long)]
        kind: Option<String>,
        /// Rate as AMOUNT/PER, e.g. 4.2/day.
        #[arg(long)]
        rate: Option<String>,
        /// Capacity in units.
        #[arg(long = "max")]
        max_units: Option<String>,
        #[command(flatten)]
        m: MutArgs,
    },
}

#[derive(Args, Default)]
struct TaskFields {
    /// task, milestone, loe or buffer.
    #[arg(long)]
    kind: Option<String>,
    /// deliverable, control_account, work_package or planning_package.
    #[arg(long)]
    wbs_role: Option<String>,
    /// predictive, iterative or flow.
    #[arg(long)]
    approach: Option<String>,
    /// Milestone class: contractual, internal or external.
    #[arg(long)]
    class: Option<String>,
    /// Accountable person.
    #[arg(long)]
    owner: Option<String>,
    /// Duration, e.g. 15d.
    #[arg(long)]
    duration: Option<String>,
    /// Effort, e.g. 120h.
    #[arg(long)]
    effort: Option<String>,
    /// Acceptance criterion.
    #[arg(long)]
    acceptance: Option<String>,
    /// Soft deadline (YYYY-MM-DD).
    #[arg(long)]
    deadline: Option<String>,
    /// Constraint as TYPE or TYPE:DATE, e.g. SNET:2027-03-01.
    #[arg(long)]
    constraint: Option<String>,
    /// Three-point estimate OPTIMISTIC/MOST-LIKELY/PESSIMISTIC, e.g. 25d/30d/40d.
    #[arg(long)]
    estimate: Option<String>,
    /// Basis of the estimate (where the numbers come from).
    #[arg(long)]
    basis: Option<String>,
    /// Any other task key, as KEY=VALUE (repeatable).
    #[arg(long = "field", value_name = "KEY=VALUE")]
    extra: Vec<String>,
}

impl TaskFields {
    fn into_map(self) -> Result<BTreeMap<String, FieldValue>, String> {
        let mut m = BTreeMap::new();
        let pairs = [
            ("kind", self.kind),
            ("wbs_role", self.wbs_role),
            ("approach", self.approach),
            ("class", self.class),
            ("owner", self.owner),
            ("duration", self.duration),
            ("effort", self.effort),
            ("acceptance", self.acceptance),
            ("deadline", self.deadline),
            ("constraint", self.constraint),
            ("estimate", self.estimate),
            ("estimate.basis", self.basis),
        ];
        for (k, v) in pairs {
            if let Some(v) = v {
                m.insert(k.to_string(), FieldValue::Text(v));
            }
        }
        for kv in self.extra {
            let (k, v) = kv
                .split_once('=')
                .ok_or_else(|| format!("--field expects KEY=VALUE, got `{kv}`"))?;
            m.insert(k.to_string(), FieldValue::Text(v.to_string()));
        }
        Ok(m)
    }
}

#[derive(Args)]
struct MutArgs {
    /// Show the effect without writing.
    #[arg(long)]
    dry_run: bool,
}

#[derive(Args)]
struct PageArgs {
    /// Maximum items.
    #[arg(long)]
    limit: Option<usize>,
    /// Cursor from a previous page.
    #[arg(long)]
    cursor: Option<String>,
}

#[derive(Clone, Copy, ValueEnum)]
enum MinSeverity {
    Info,
    Warn,
    Error,
}

/// A command failure that is not about the plan (exit code 2).
struct Usage(String);

fn main() -> ExitCode {
    let cli = Cli::parse();
    let json = cli.json;
    match run(cli) {
        Ok(code) => code,
        Err(Usage(msg)) => {
            if json {
                let d = Diagnostic::error("E-CLI-01", msg);
                println!("{}", serde_json::json!({ "ok": false, "errors": [d] }));
            } else {
                eprintln!("error: {msg}");
            }
            ExitCode::from(2)
        }
    }
}

fn resolve_file(file: Option<PathBuf>) -> Result<PathBuf, Usage> {
    alma_ops::files::resolve_plan(file.as_deref(), Path::new(".")).map_err(Usage)
}

fn read(path: &Path) -> Result<String, Usage> {
    alma_ops::files::read_text(path).map_err(Usage)
}

fn print_json<T: Serialize>(v: &T) {
    println!("{}", serde_json::to_string_pretty(v).expect("serializable"));
}

fn print_diag(d: &Diagnostic) {
    let at = d
        .span
        .map(|s| format!(" (line {})", s.line))
        .unwrap_or_default();
    println!("{:5} {}{at}: {}", d.severity.as_str(), d.code, d.message);
    if let Some(h) = &d.hint {
        println!("      hint: {h}");
    }
    if let Some(f) = &d.fix {
        println!("      fix:  {f}");
    }
}

fn run(cli: Cli) -> Result<ExitCode, Usage> {
    let json = cli.json;
    let file = cli.file;
    let loaded = |file: Option<PathBuf>| -> Result<(PathBuf, alma_ops::Loaded), Usage> {
        let path = resolve_file(file)?;
        let src = read(&path)?;
        Ok((path, alma_ops::load(&src)))
    };
    // Queries need a schedulable plan; report its errors otherwise.
    let scheduled = |l: &alma_ops::Loaded| -> Option<ExitCode> {
        if l.plan.is_some() && l.schedule.is_some() {
            return None;
        }
        let errors: Vec<&Diagnostic> = l
            .diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .collect();
        if json {
            print_json(&serde_json::json!({ "ok": false, "errors": errors }));
        } else {
            println!("the plan cannot be scheduled:");
            errors.iter().for_each(|d| print_diag(d));
        }
        Some(ExitCode::from(1))
    };

    match cli.cmd {
        Cmd::Init { path, name, start } => {
            if path.exists() {
                return Err(Usage(format!("{} already exists", path.display())));
            }
            let text = mutate::init_text(&name, &start).map_err(|d| Usage(d.message))?;
            mutate::write_atomic(&path, &text)
                .map_err(|e| Usage(format!("cannot write {}: {e}", path.display())))?;
            if json {
                print_json(&serde_json::json!({ "ok": true, "file": path }));
            } else {
                println!("created {}", path.display());
            }
            Ok(ExitCode::SUCCESS)
        }
        Cmd::Check { min, page } => {
            let (_, l) = loaded(file)?;
            let min = match min {
                MinSeverity::Info => Severity::Info,
                MinSeverity::Warn => Severity::Warn,
                MinSeverity::Error => Severity::Error,
            };
            let report = query::check(&l, min, Page::new(page.limit, page.cursor.as_deref()));
            if json {
                print_json(&report);
            } else {
                report.diagnostics.iter().for_each(print_diag);
                let c = report.counts;
                println!("{} errors, {} warnings, {} infos", c.error, c.warn, c.info);
                if let Some(next) = &report.next_cursor {
                    println!("more: --cursor {next}");
                }
            }
            Ok(if report.ok {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            })
        }
        Cmd::Show { id } => {
            let (_, l) = loaded(file)?;
            if let Some(code) = scheduled(&l) {
                return Ok(code);
            }
            let (plan, s) = (l.plan.as_ref().unwrap(), l.schedule.as_ref().unwrap());
            let views = query::task_views(plan, s);
            let Some(v) = views.into_iter().find(|v| v.id == id) else {
                return Err(Usage(format!("no task with id `{id}`")));
            };
            if json {
                print_json(&v);
            } else {
                println!("{} {} — {}", v.wbs, v.id, v.name);
                println!("  kind      {}", v.kind);
                println!("  duration  {}", v.duration);
                println!("  start     {}   (ES {}, LS {})", v.start, v.es, v.ls);
                println!("  finish    {}   (EF {}, LF {})", v.finish, v.ef, v.lf);
                if let Some(tf) = v.tf {
                    println!("  float     total {tf}d, free {}d", v.ff.unwrap_or(0.0));
                }
                println!("  critical  {}", v.critical);
            }
            Ok(ExitCode::SUCCESS)
        }
        Cmd::Ls {
            parent,
            critical,
            kind,
            fields,
            page,
        } => {
            let (_, l) = loaded(file)?;
            if let Some(code) = scheduled(&l) {
                return Ok(code);
            }
            let fields: Vec<String> = fields.split(',').map(|f| f.trim().to_string()).collect();
            if let Some(bad) = fields.iter().find(|f| !TASK_FIELDS.contains(&f.as_str())) {
                return Err(Usage(format!(
                    "unknown field `{bad}`; fields: {}",
                    TASK_FIELDS.join(",")
                )));
            }
            let filter = ListFilter {
                parent,
                critical,
                kind,
            };
            let (plan, s) = (l.plan.as_ref().unwrap(), l.schedule.as_ref().unwrap());
            let report = query::list(
                plan,
                s,
                &filter,
                &fields,
                Page::new(page.limit, page.cursor.as_deref()),
            );
            if json {
                print_json(&report);
            } else {
                for t in &report.tasks {
                    let row: Vec<String> = fields
                        .iter()
                        .map(|f| match t.get(f) {
                            Some(serde_json::Value::String(s)) => s.clone(),
                            Some(v) => v.to_string(),
                            None => "-".into(),
                        })
                        .collect();
                    println!("{}", row.join("\t"));
                }
                if let Some(next) = &report.next_cursor {
                    println!(
                        "({} of {}; more: --cursor {next})",
                        report.tasks.len(),
                        report.total
                    );
                }
            }
            Ok(ExitCode::SUCCESS)
        }
        Cmd::Critical => {
            let (_, l) = loaded(file)?;
            if let Some(code) = scheduled(&l) {
                return Ok(code);
            }
            let path = query::critical(l.plan.as_ref().unwrap(), l.schedule.as_ref().unwrap());
            let finish = alma_core::time::finish_date(l.schedule.as_ref().unwrap().finish);
            if json {
                print_json(&serde_json::json!({
                    "schema": alma_ops::OUTPUT_SCHEMA,
                    "project_finish": finish.to_string(),
                    "critical": path,
                }));
            } else {
                for v in &path {
                    println!("{}\t{}\t{}\t{}", v.id, v.start, v.finish, v.name);
                }
                println!("project finish: {finish}");
            }
            Ok(ExitCode::SUCCESS)
        }
        Cmd::Explain { id } => {
            let (_, l) = loaded(file)?;
            if let Some(code) = scheduled(&l) {
                return Ok(code);
            }
            let Some(steps) =
                query::explain(l.plan.as_ref().unwrap(), l.schedule.as_ref().unwrap(), &id)
            else {
                return Err(Usage(format!("no task with id `{id}`")));
            };
            if json {
                print_json(
                    &serde_json::json!({ "schema": alma_ops::OUTPUT_SCHEMA, "chain": steps }),
                );
            } else {
                for s in &steps {
                    println!("{} starts {}: {}", s.id, s.start, s.because);
                }
            }
            Ok(ExitCode::SUCCESS)
        }
        Cmd::Task(t) => {
            let (ops, m) = match t {
                TaskCmd::Add {
                    id,
                    name,
                    parent,
                    after,
                    fields,
                    upsert,
                    m,
                } => (
                    vec![Op::TaskAdd {
                        id,
                        name,
                        parent,
                        after,
                        fields: fields.into_map().map_err(Usage)?,
                        upsert,
                    }],
                    m,
                ),
                TaskCmd::Set {
                    id,
                    name,
                    fields,
                    unset,
                    m,
                } => {
                    let mut f = fields.into_map().map_err(Usage)?;
                    if let Some(n) = name {
                        f.insert("name".into(), FieldValue::Text(n));
                    }
                    (
                        vec![Op::TaskSet {
                            id,
                            fields: f,
                            unset,
                        }],
                        m,
                    )
                }
                TaskCmd::Rm { id, cascade, m } => (vec![Op::TaskRm { id, cascade }], m),
                TaskCmd::Move {
                    id,
                    parent,
                    after,
                    m,
                } => (vec![Op::TaskMove { id, parent, after }], m),
            };
            mutation(file, &ops, m.dry_run, json)
        }
        Cmd::Dep(d) => {
            let (ops, m) = match d {
                DepCmd::Add {
                    pred,
                    succ,
                    kind,
                    lag,
                    nature,
                    scope,
                    rationale,
                    m,
                } => (
                    vec![Op::DepAdd {
                        pred,
                        succ,
                        kind,
                        lag,
                        nature,
                        scope,
                        rationale,
                    }],
                    m,
                ),
                DepCmd::Rm { pred, succ, m } => (vec![Op::DepRm { pred, succ }], m),
            };
            mutation(file, &ops, m.dry_run, json)
        }
        Cmd::Res(ResCmd::Add {
            id,
            name,
            kind,
            rate,
            max_units,
            m,
        }) => mutation(
            file,
            &[Op::ResAdd {
                id,
                name,
                kind,
                rate,
                max_units,
            }],
            m.dry_run,
            json,
        ),
        Cmd::Assign {
            task,
            resource,
            units,
            m,
        } => mutation(
            file,
            &[Op::Assign {
                task,
                resource,
                units,
            }],
            m.dry_run,
            json,
        ),
        Cmd::Unassign { task, resource, m } => {
            mutation(file, &[Op::Unassign { task, resource }], m.dry_run, json)
        }
        Cmd::Apply { ops, m } => {
            let text = if ops.as_os_str() == "-" {
                let mut s = String::new();
                std::io::stdin()
                    .read_to_string(&mut s)
                    .map_err(|e| Usage(format!("cannot read stdin: {e}")))?;
                s
            } else {
                read(&ops)?
            };
            let parsed = parse_ops(&text).map_err(Usage)?;
            mutation(file, &parsed, m.dry_run, json)
        }
        Cmd::Export {
            out,
            format,
            range,
            depth,
            scale,
            dark,
            no_arrows,
            font_size,
        } => {
            let (_, l) = loaded(file)?;
            if let Some(code) = scheduled(&l) {
                return Ok(code);
            }
            let (plan, s) = (l.plan.as_ref().unwrap(), l.schedule.as_ref().unwrap());
            let format = format.unwrap_or_else(|| {
                out.extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("svg")
                    .to_ascii_lowercase()
            });
            let req = alma_ops::export::RenderRequest {
                range,
                depth,
                scale,
                dark,
                no_arrows,
                font_size,
            };
            let bytes: Vec<u8> = match format.as_str() {
                "svg" => alma_ops::export::svg(plan, s, &req)
                    .map_err(Usage)?
                    .into_bytes(),
                "png" => alma_ops::export::png(plan, s, &req).map_err(Usage)?,
                "csv" => alma_ops::export::csv(plan, s).into_bytes(),
                other => {
                    return Err(Usage(format!(
                        "unknown format `{other}`; use svg, png or csv"
                    )));
                }
            };
            let warnings = if format == "csv" {
                Vec::new()
            } else {
                alma_ops::export::warnings(plan, s, &req).map_err(Usage)?
            };
            for w in &warnings {
                if !json {
                    eprintln!("warning: {w}");
                }
            }
            if out.as_os_str() == "-" {
                if format == "png" {
                    return Err(Usage(
                        "PNG cannot be written to stdout; give a file name".into(),
                    ));
                }
                print!("{}", String::from_utf8_lossy(&bytes));
                return Ok(ExitCode::SUCCESS);
            }
            write_bytes(&out, &bytes)?;
            if json {
                print_json(
                    &serde_json::json!({ "ok": true, "file": out, "format": format, "bytes": bytes.len(), "warnings": warnings }),
                );
            } else {
                println!("wrote {} ({format}, {} bytes)", out.display(), bytes.len());
            }
            Ok(ExitCode::SUCCESS)
        }
        Cmd::Fmt { check } => {
            let path = resolve_file(file)?;
            let src = read(&path)?;
            let out = match mutate::fmt_text(&src) {
                Ok(out) => out,
                Err(d) => {
                    if json {
                        print_json(&serde_json::json!({ "ok": false, "errors": [d] }));
                    } else {
                        print_diag(&d);
                    }
                    return Ok(ExitCode::from(1));
                }
            };
            let changed = out != src;
            if !check && changed {
                mutate::write_atomic(&path, &out)
                    .map_err(|e| Usage(format!("cannot write {}: {e}", path.display())))?;
            }
            if json {
                print_json(
                    &serde_json::json!({ "ok": !(check && changed), "changed": changed, "check": check }),
                );
            } else if changed {
                println!(
                    "{} {}",
                    if check {
                        "would reformat"
                    } else {
                        "reformatted"
                    },
                    path.display()
                );
            } else {
                println!("{} is already formatted", path.display());
            }
            Ok(if check && changed {
                ExitCode::from(1)
            } else {
                ExitCode::SUCCESS
            })
        }
        Cmd::Diff { against, rev } => {
            let path = resolve_file(file)?;
            let current = read(&path)?;
            let old = match (against, rev) {
                (Some(f), _) => read(&f)?,
                (None, rev) => git_show(&path, rev.as_deref().unwrap_or("HEAD"))?,
            };
            let report = alma_ops::diff::diff(&alma_ops::load(&old), &alma_ops::load(&current));
            if json {
                print_json(&report);
            } else {
                if report.changed.is_empty() {
                    println!("no dates moved");
                }
                for m in &report.changed {
                    println!("  {}  start {}  finish {}", m.id, m.start, m.finish);
                }
                if let Some(d) = &report.end_date_delta {
                    println!("project finish: {d}");
                }
                if !report.new_diagnostics.is_empty() {
                    println!("new diagnostics:");
                    report.new_diagnostics.iter().for_each(print_diag);
                }
                if !report.resolved_diagnostics.is_empty() {
                    println!("resolved: {}", report.resolved_diagnostics.len());
                }
            }
            Ok(ExitCode::SUCCESS)
        }
        Cmd::Iter(c) => {
            let (op, m) = match c {
                IterCmd::Add {
                    id,
                    start,
                    length,
                    count,
                    name,
                    prefix,
                    calendar,
                    m,
                } => (
                    Op::CadenceAdd {
                        id,
                        start,
                        length,
                        count,
                        name,
                        prefix,
                        calendar,
                    },
                    m,
                ),
                IterCmd::Rm { id, m } => (Op::CadenceRm { id }, m),
            };
            mutation(file, &[op], m.dry_run, json)
        }
        Cmd::Cal(c) => {
            let (op, m) = match c {
                CalCmd::Add {
                    id,
                    name,
                    workdays,
                    hours,
                    m,
                } => (
                    Op::CalAdd {
                        id,
                        name,
                        workdays,
                        hours,
                    },
                    m,
                ),
                CalCmd::Except {
                    calendar,
                    name,
                    from,
                    to,
                    working,
                    m,
                } => (
                    Op::CalExcept {
                        calendar,
                        name,
                        to: to.unwrap_or_else(|| from.clone()),
                        from,
                        working,
                    },
                    m,
                ),
            };
            mutation(file, &[op], m.dry_run, json)
        }
        Cmd::Project(ProjectCmd::Set {
            name,
            calendar,
            deadline,
            currency,
            m,
        }) => {
            let mut fields = BTreeMap::new();
            for (k, v) in [
                ("name", name),
                ("calendar", calendar),
                ("deadline", deadline),
                ("currency", currency),
            ] {
                if let Some(v) = v {
                    fields.insert(k.to_string(), FieldValue::Text(v));
                }
            }
            mutation(file, &[Op::ProjectSet { fields }], m.dry_run, json)
        }
        Cmd::Rule(RuleCmd::Set { code, severity, m }) => {
            mutation(file, &[Op::RuleSet { code, severity }], m.dry_run, json)
        }
        Cmd::Keys { table } => {
            let tables: Vec<&str> = {
                let mut t: Vec<&str> = alma_ops::keys::KEYS.iter().map(|k| k.table).collect();
                t.dedup();
                t
            };
            let Some(table) = table else {
                if json {
                    print_json(&serde_json::json!({ "tables": tables }));
                } else {
                    println!("{}", tables.join("\n"));
                }
                return Ok(ExitCode::SUCCESS);
            };
            if !tables.contains(&table.as_str()) {
                return Err(Usage(format!(
                    "unknown table `{table}`; tables: {}",
                    tables.join(", ")
                )));
            }
            let rows: Vec<serde_json::Value> = alma_ops::keys::keys_of(&table)
                .map(|k| {
                    let tier = match k.tier {
                        alma_ops::keys::Tier::A => "active".to_string(),
                        alma_ops::keys::Tier::B => "accepted".to_string(),
                        alma_ops::keys::Tier::R(m) => format!("reserved for {m}"),
                    };
                    serde_json::json!({ "key": k.key, "status": tier })
                })
                .collect();
            if json {
                print_json(&serde_json::json!({ "table": table, "keys": rows }));
            } else {
                for r in &rows {
                    println!(
                        "{:20} {}",
                        r["key"].as_str().unwrap_or(""),
                        r["status"].as_str().unwrap_or("")
                    );
                }
            }
            Ok(ExitCode::SUCCESS)
        }
        Cmd::Guide => {
            print!("{GUIDE}");
            Ok(ExitCode::SUCCESS)
        }
        Cmd::Schema => {
            print!("{}", alma_ops::schema::OPS_SCHEMA);
            Ok(ExitCode::SUCCESS)
        }
    }
}

fn git_show(path: &Path, rev: &str) -> Result<String, Usage> {
    alma_ops::files::git_show(path, rev).map_err(Usage)
}

fn write_bytes(path: &Path, bytes: &[u8]) -> Result<(), Usage> {
    alma_ops::files::write_bytes(path, bytes).map_err(Usage)
}

fn parse_ops(text: &str) -> Result<Vec<Op>, String> {
    alma_ops::files::parse_ops(text)
}

fn mutation(
    file: Option<PathBuf>,
    ops: &[Op],
    dry_run: bool,
    json: bool,
) -> Result<ExitCode, Usage> {
    let path = resolve_file(file)?;
    let report = mutate::apply_to_file(&path, ops, dry_run)
        .map_err(|e| Usage(format!("cannot update {}: {e}", path.display())))?;
    if json {
        print_json(&report);
    } else {
        print_mutation(&report);
    }
    Ok(if report.ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    })
}

fn print_mutation(r: &MutationReport) {
    if !r.ok {
        println!("rejected; the file was not changed:");
        r.errors.iter().for_each(print_diag);
        return;
    }
    let verb = if r.dry_run { "would change" } else { "changed" };
    if r.noop {
        println!("no change (already up to date)");
        return;
    }
    if r.changed.is_empty() {
        println!("{verb} the file; no dates moved");
    } else {
        println!("{verb}:");
        for m in &r.changed {
            println!("  {}  start {}  finish {}", m.id, m.start, m.finish);
        }
    }
    if let Some(d) = &r.end_date_delta {
        println!("project finish: {d}");
    }
    if !r.new_diagnostics.is_empty() {
        println!("new diagnostics:");
        r.new_diagnostics.iter().for_each(print_diag);
    }
}
