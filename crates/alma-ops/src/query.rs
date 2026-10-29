//! Read-only queries: `check`, `show`, `ls`, `critical`, `explain` (SPEC.md §8a).

use alma_core::diag::{Diagnostic, Severity};
use alma_core::model::{ConstraintKind, Plan};
use alma_core::time::{Instant, finish_date, format_instant, start_date};
use alma_sched::cpm::Calendars;
use alma_sched::{Driver, PlanIndex, Schedule};
use serde::Serialize;

use crate::{Counts, Loaded, OUTPUT_SCHEMA};

/// Result of `alma check`.
#[derive(Debug, Clone, Serialize)]
pub struct CheckReport {
    /// Output schema version.
    pub schema: u32,
    /// True when there are no errors.
    pub ok: bool,
    /// Counts by severity (of all diagnostics, before pagination).
    pub counts: Counts,
    /// Diagnostics on this page.
    pub diagnostics: Vec<Diagnostic>,
    /// Cursor for the next page, when there is one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

/// `alma check`: diagnostics at or above `min`, paginated.
pub fn check(loaded: &Loaded, min: Severity, page: Page) -> CheckReport {
    let all: Vec<&Diagnostic> = loaded
        .diagnostics
        .iter()
        .filter(|d| d.severity >= min)
        .collect();
    let (items, next_cursor) = page.apply(&all);
    CheckReport {
        schema: OUTPUT_SCHEMA,
        ok: !loaded.has_errors(),
        counts: Counts::of(&loaded.diagnostics),
        diagnostics: items.into_iter().cloned().collect(),
        next_cursor,
    }
}

/// Pagination: `limit` items starting at the opaque `cursor` (an offset).
#[derive(Debug, Clone, Copy)]
pub struct Page {
    /// Maximum items.
    pub limit: usize,
    /// Offset to start at.
    pub offset: usize,
}

impl Page {
    /// Default page size for list commands.
    pub const DEFAULT_LIMIT: usize = 50;

    /// Builds a page from CLI arguments; an invalid cursor starts at 0.
    pub fn new(limit: Option<usize>, cursor: Option<&str>) -> Self {
        Self {
            limit: limit.unwrap_or(Self::DEFAULT_LIMIT).max(1),
            offset: cursor.and_then(|c| c.parse().ok()).unwrap_or(0),
        }
    }

    /// Everything.
    pub fn all() -> Self {
        Self {
            limit: usize::MAX,
            offset: 0,
        }
    }

    fn apply<T: Clone>(&self, items: &[T]) -> (Vec<T>, Option<String>) {
        let start = self.offset.min(items.len());
        let end = start.saturating_add(self.limit).min(items.len());
        let next = (end < items.len()).then(|| end.to_string());
        (items[start..end].to_vec(), next)
    }
}

/// One task with its computed values. Durations and floats are in working days.
#[derive(Debug, Clone, Serialize)]
pub struct TaskView {
    /// Id.
    pub id: String,
    /// Name.
    pub name: String,
    /// WBS code, derived from the tree (1.2.3).
    pub wbs: String,
    /// Parent id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    /// `task`, `milestone`, `loe`, `buffer` or `summary`.
    pub kind: String,
    /// WBS role.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wbs_role: Option<String>,
    /// Resolved duration, as a literal.
    pub duration: String,
    /// Early start (date).
    pub start: String,
    /// Early finish (date).
    pub finish: String,
    /// Early start instant.
    pub es: String,
    /// Early finish instant.
    pub ef: String,
    /// Late start instant.
    pub ls: String,
    /// Late finish instant.
    pub lf: String,
    /// Total float in working days.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tf: Option<f64>,
    /// Free float in working days.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ff: Option<f64>,
    /// On the critical set.
    pub critical: bool,
}

impl TaskView {
    /// The value of a field by name, for `--fields`.
    pub fn field(&self, name: &str) -> Option<serde_json::Value> {
        let v = serde_json::to_value(self).ok()?;
        v.get(name).cloned()
    }
}

/// Every field name `TaskView` has.
pub const TASK_FIELDS: &[&str] = &[
    "id", "name", "wbs", "parent", "kind", "wbs_role", "duration", "start", "finish", "es", "ef",
    "ls", "lf", "tf", "ff", "critical",
];

/// WBS codes in source order.
pub fn wbs_codes(plan: &Plan, index: &PlanIndex) -> Vec<String> {
    let mut codes = vec![String::new(); plan.tasks.len()];
    let mut counters: Vec<usize> = vec![0; plan.tasks.len()];
    let mut roots = 0usize;
    for i in 0..plan.tasks.len() {
        match index.parent[i] {
            None => {
                roots += 1;
                codes[i] = roots.to_string();
            }
            Some(p) => {
                counters[p] += 1;
                codes[i] = if codes[p].is_empty() {
                    // Parent appears after the child: fall back to the child's position.
                    counters[p].to_string()
                } else {
                    format!("{}.{}", codes[p], counters[p])
                };
            }
        }
    }
    codes
}

/// Builds the views of every task, in source order.
pub fn task_views(plan: &Plan, schedule: &Schedule) -> Vec<TaskView> {
    let index = PlanIndex::new(plan);
    let codes = wbs_codes(plan, &index);
    let units = plan.settings.units;
    let day = units.minutes_per_day as f64;
    let cals = Calendars::new(plan);
    plan.tasks
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let d = &schedule.tasks[i];
            let summary = index.is_summary(i);
            let duration = if summary {
                let cal = cals.of_task(plan, i);
                alma_core::Duration::working(cal.between(d.es, d.ef)).display(units)
            } else {
                d.duration.display(units)
            };
            let round = |m: i64| (m as f64 / day * 100.0).round() / 100.0;
            TaskView {
                id: t.id.clone(),
                name: t.name.clone(),
                wbs: codes[i].clone(),
                parent: t.parent.clone(),
                kind: if summary {
                    "summary".into()
                } else {
                    t.kind.as_str().into()
                },
                wbs_role: t.wbs_role.map(|r| r.as_str().to_string()),
                duration,
                start: start_date(d.es).to_string(),
                finish: if d.ef == d.es {
                    start_date(d.es).to_string()
                } else {
                    finish_date(d.ef).to_string()
                },
                es: format_instant(d.es),
                ef: format_instant(d.ef),
                ls: format_instant(d.ls),
                lf: format_instant(d.lf),
                tf: d.total_float.map(round),
                ff: d.free_float.map(round),
                critical: d.critical,
            }
        })
        .collect()
}

/// Filter for `alma ls`.
#[derive(Debug, Clone, Default)]
pub struct ListFilter {
    /// Only direct children of this task.
    pub parent: Option<String>,
    /// Only critical tasks.
    pub critical: bool,
    /// Only this kind (`summary` included).
    pub kind: Option<String>,
}

/// A page of task views.
#[derive(Debug, Clone, Serialize)]
pub struct ListReport {
    /// Output schema version.
    pub schema: u32,
    /// Total matching tasks.
    pub total: usize,
    /// Tasks on this page; only the requested fields.
    pub tasks: Vec<serde_json::Map<String, serde_json::Value>>,
    /// Cursor for the next page.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

/// `alma ls`.
pub fn list(
    plan: &Plan,
    schedule: &Schedule,
    filter: &ListFilter,
    fields: &[String],
    page: Page,
) -> ListReport {
    let views: Vec<TaskView> = task_views(plan, schedule)
        .into_iter()
        .filter(|v| match &filter.parent {
            Some(p) if p.is_empty() => v.parent.is_none(),
            Some(p) => v.parent.as_deref() == Some(p.as_str()),
            None => true,
        })
        .filter(|v| !filter.critical || v.critical)
        .filter(|v| filter.kind.as_ref().is_none_or(|k| &v.kind == k))
        .collect();
    let total = views.len();
    let (items, next_cursor) = page.apply(&views);
    let tasks = items
        .iter()
        .map(|v| {
            fields
                .iter()
                .filter_map(|f| v.field(f).map(|val| (f.clone(), val)))
                .collect()
        })
        .collect();
    ListReport {
        schema: OUTPUT_SCHEMA,
        total,
        tasks,
        next_cursor,
    }
}

/// `alma critical`: critical network tasks ordered by early start, then source order.
pub fn critical(plan: &Plan, schedule: &Schedule) -> Vec<TaskView> {
    let mut v: Vec<(Instant, usize, TaskView)> = task_views(plan, schedule)
        .into_iter()
        .enumerate()
        .filter(|(i, v)| v.critical && schedule.tasks[*i].in_network)
        .map(|(i, v)| (schedule.tasks[i].es, i, v))
        .collect();
    v.sort_by_key(|(es, i, _)| (*es, *i));
    v.into_iter().map(|(_, _, v)| v).collect()
}

/// One step of an `explain` chain.
#[derive(Debug, Clone, Serialize)]
pub struct ExplainStep {
    /// Task id.
    pub id: String,
    /// Early start of the task.
    pub start: String,
    /// Why it starts then.
    pub because: String,
    /// The driving predecessor, when a link set the date.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub driven_by: Option<String>,
}

/// `alma explain`: the chain of drivers from `id` back to the project start or a constraint
/// (F-SCH-11).
pub fn explain(plan: &Plan, schedule: &Schedule, id: &str) -> Option<Vec<ExplainStep>> {
    let index = PlanIndex::new(plan);
    let units = plan.settings.units;
    let cals = Calendars::new(plan);
    let mut cur = *index.by_id.get(id)?;
    // A summary is explained by its earliest leaf.
    if index.is_summary(cur) {
        cur = index
            .leaves(cur)
            .into_iter()
            .filter(|&l| schedule.tasks[l].in_network)
            .min_by_key(|&l| (schedule.tasks[l].es, l))?;
    }
    let mut steps = Vec::new();
    for _ in 0..plan.tasks.len() + 1 {
        let t = &plan.tasks[cur];
        let d = &schedule.tasks[cur];
        let (because, next) = match d.driver {
            Driver::ProjectStart => ("starts at the project start".to_string(), None),
            Driver::Constraint(k) => {
                let date = t
                    .constraint
                    .as_ref()
                    .and_then(|c| c.date)
                    .map(|d| d.to_string())
                    .unwrap_or_default();
                (format!("{} constraint on {date}", constraint_text(k)), None)
            }
            Driver::Dep { task, dep, pred } => {
                let link = &plan.tasks[task].deps[dep];
                let lag = if link.lag.minutes == 0 {
                    String::new()
                } else {
                    format!(" {}", link.lag.display(units))
                };
                let via = if task != cur {
                    format!(" (link on summary `{}`)", plan.tasks[task].id)
                } else {
                    String::new()
                };
                let verb = match link.kind {
                    alma_core::model::DepType::Fs => "cannot start until it finishes",
                    alma_core::model::DepType::Ss => "cannot start until it starts",
                    alma_core::model::DepType::Ff => "cannot finish until it finishes",
                    alma_core::model::DepType::Sf => "cannot finish until it starts",
                };
                (
                    format!(
                        "{} {verb}{lag}: `{}`{via}",
                        link.kind.as_str(),
                        plan.tasks[pred].id
                    ),
                    Some(pred),
                )
            }
            Driver::Rollup | Driver::Span => ("derived".into(), None),
        };
        let mut because = because;
        // Name the first non-working calendar exception between the driver and the start.
        if let Some(p) = next {
            let cal = cals.of_task(plan, cur);
            let from = alma_core::time::day_of(schedule.tasks[p].es);
            let to = alma_core::time::day_of(d.es);
            if let Some(ex) = (from..to)
                .find(|&x| cal.intervals(x).is_empty() && cal.exception_on(x).is_some())
                .and_then(|x| cal.exception_on(x))
            {
                because.push_str(&format!("; shifted past \"{ex}\""));
            }
        }
        steps.push(ExplainStep {
            id: t.id.clone(),
            start: format_instant(d.es),
            because,
            driven_by: next.map(|p| plan.tasks[p].id.clone()),
        });
        match next {
            Some(p) => cur = p,
            None => break,
        }
    }
    Some(steps)
}

fn constraint_text(k: ConstraintKind) -> &'static str {
    match k {
        ConstraintKind::Snet => "start-no-earlier-than",
        ConstraintKind::Fnet => "finish-no-earlier-than",
        ConstraintKind::Mso => "must-start-on",
        ConstraintKind::Mfo => "must-finish-on",
        ConstraintKind::Snlt => "start-no-later-than",
        ConstraintKind::Fnlt => "finish-no-later-than",
        ConstraintKind::Asap => "as-soon-as-possible",
        ConstraintKind::Alap => "as-late-as-possible",
    }
}
