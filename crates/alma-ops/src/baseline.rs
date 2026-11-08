//! Baselines: `alma baseline save|ls` and `alma variance` (ADR 0013, `docs/format.md` D14).
//!
//! The plan keeps each baseline's metadata (`[[baseline]]`); the frozen dates, costs and planned
//! value go to `.alma/baselines/<plan>/<id>.toml` next to the plan file. Saving writes the
//! snapshot first and the plan second, both atomically; if the plan cannot be written, the
//! snapshot is removed, so neither exists without the other.

// Edits return one `Diagnostic` once per command, as in `mutate`; boxing it buys nothing.
#![allow(clippy::result_large_err)]

use std::path::{Path, PathBuf};

use alma_core::model::{Plan, Snapshot};
use alma_core::time::{finish_date, start_date};
use alma_format::snapshot;
use alma_sched::Schedule;
use alma_sched::cpm::Calendars;
use rust_decimal::Decimal;
use serde::Serialize;

use crate::cost::money;
use crate::mutate::{BaselineMeta, MutationReport, apply_edit, write_atomic};
use crate::query::Page;
use crate::{OUTPUT_SCHEMA, load};

/// Where the snapshot of baseline `id` of the plan at `plan_path` lives.
pub fn snapshot_path(plan_path: &Path, id: &str) -> PathBuf {
    let dir = plan_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let name = plan_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("plan");
    let stem = name
        .strip_suffix(".alma.toml")
        .or_else(|| name.strip_suffix(".toml"))
        .unwrap_or(name);
    dir.join(".alma")
        .join("baselines")
        .join(stem)
        .join(format!("{id}.toml"))
}

/// Reads the snapshot of baseline `id`.
pub fn read_snapshot(plan_path: &Path, id: &str) -> Result<Snapshot, String> {
    let path = snapshot_path(plan_path, id);
    let text = std::fs::read_to_string(&path).map_err(|e| {
        format!(
            "baseline `{id}` has no snapshot at {} ({e}); it was not saved with `alma baseline save`, or the .alma directory was not committed",
            path.display()
        )
    })?;
    let snap = snapshot::from_text(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    if snap.baseline != id {
        return Err(format!(
            "{} holds baseline `{}`, not `{id}`",
            path.display(),
            snap.baseline
        ));
    }
    Ok(snap)
}

/// The active baseline of a plan, or the one named.
pub fn pick<'a>(plan: &'a Plan, id: Option<&str>) -> Result<&'a str, String> {
    match id {
        Some(id) => plan
            .baselines
            .iter()
            .find(|b| b.id == id)
            .map(|b| b.id.as_str())
            .ok_or_else(|| {
                let ids: Vec<&str> = plan.baselines.iter().map(|b| b.id.as_str()).collect();
                format!("no baseline `{id}`; baselines: {}", ids.join(", "))
            }),
        None => plan
            .baselines
            .iter()
            .find(|b| b.active)
            .map(|b| b.id.as_str())
            .ok_or_else(|| {
                "the plan has no active baseline; save one with `alma baseline save <id>`"
                    .to_string()
            }),
    }
}

/// The snapshot to compare with: the named baseline, else the active one; `None` when no id is
/// given and no baseline is active.
pub fn snapshot_for(
    plan: &Plan,
    plan_path: &Path,
    id: Option<&str>,
) -> Result<Option<Snapshot>, String> {
    if id.is_none() && !plan.baselines.iter().any(|b| b.active) {
        return Ok(None);
    }
    let id = pick(plan, id)?;
    read_snapshot(plan_path, id).map(Some)
}

/// What a saved baseline froze.
#[derive(Debug, Clone, Serialize)]
pub struct SavedView {
    /// Id.
    pub id: String,
    /// Snapshot file.
    pub file: String,
    /// Project finish.
    pub finish: String,
    /// Budget at completion.
    pub bac: f64,
}

/// Result of `alma baseline save`.
#[derive(Debug, Clone, Serialize)]
pub struct SaveReport {
    /// The mutation report (the plan only gains the `[[baseline]]` table; no date moves).
    #[serde(flatten)]
    pub report: MutationReport,
    /// The baseline, when the save applied (or would, with `--dry-run`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub baseline: Option<SavedView>,
}

/// `alma baseline save`: freezes the current schedule as a new active baseline.
pub fn save(path: &Path, meta: &BaselineMeta, dry_run: bool) -> std::io::Result<SaveReport> {
    let src = std::fs::read_to_string(path)?;
    let (text, report) = apply_edit(&src, dry_run, false, |doc| {
        crate::mutate::add_baseline(doc, meta)
    });
    let Some(text) = text else {
        return Ok(SaveReport {
            report,
            baseline: None,
        });
    };
    let loaded = load(&text);
    let (Some(plan), Some(s)) = (&loaded.plan, &loaded.schedule) else {
        return Ok(SaveReport {
            report,
            baseline: None,
        });
    };
    let snap = alma_sched::baseline::snapshot(&meta.id, plan, s);
    let file = snapshot_path(path, &meta.id);
    let view = SavedView {
        id: meta.id.clone(),
        file: file.display().to_string(),
        finish: finish_date(snap.finish).to_string(),
        bac: money(snap.bac),
    };
    if !dry_run {
        if let Some(dir) = file.parent() {
            std::fs::create_dir_all(dir)?;
        }
        write_atomic(&file, &snapshot::to_text(&snap))?;
        if let Err(e) = write_atomic(path, &text) {
            let _ = std::fs::remove_file(&file);
            return Err(e);
        }
        crate::mutate::log_command(path, "baseline.save", &meta.id);
    }
    Ok(SaveReport {
        report,
        baseline: Some(view),
    })
}

/// One baseline in `alma baseline ls`.
#[derive(Debug, Clone, Serialize)]
pub struct BaselineView {
    /// Id.
    pub id: String,
    /// Name.
    pub name: String,
    /// Day saved.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    /// Used by default.
    pub active: bool,
    /// Baseline it revises.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    /// Change request.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub change_ref: Option<String>,
    /// Project finish it promised.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish: Option<String>,
    /// Budget at completion.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bac: Option<f64>,
    /// Why the snapshot cannot be read, when it cannot.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// `alma baseline ls`.
pub fn list(plan: &Plan, plan_path: &Path) -> Vec<BaselineView> {
    plan.baselines
        .iter()
        .map(|b| {
            let snap = read_snapshot(plan_path, &b.id);
            BaselineView {
                id: b.id.clone(),
                name: b.name.clone(),
                date: b.date.map(|d| d.to_string()),
                active: b.active,
                parent: b.parent.clone(),
                change_ref: b.change_ref.clone(),
                finish: snap
                    .as_ref()
                    .ok()
                    .map(|s| finish_date(s.finish).to_string()),
                bac: snap.as_ref().ok().map(|s| money(s.bac)),
                error: snap.err(),
            }
        })
        .collect()
}

/// One task in `alma variance`.
#[derive(Debug, Clone, Serialize)]
pub struct VarianceRow {
    /// Task id.
    pub id: String,
    /// Baseline start, absent for tasks added after the baseline.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub baseline_start: Option<String>,
    /// Baseline finish.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub baseline_finish: Option<String>,
    /// Current start.
    pub start: String,
    /// Current finish.
    pub finish: String,
    /// Start variance in working days (positive = later than the baseline).
    pub start_var: f64,
    /// Finish variance in working days.
    pub finish_var: f64,
    /// Baseline cost.
    pub baseline_cost: f64,
    /// Current cost.
    pub cost: f64,
    /// Cost variance, current minus baseline (positive = more expensive).
    pub cost_var: f64,
    /// True for tasks added after the baseline.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub new: bool,
}

/// Result of `alma variance`.
#[derive(Debug, Clone, Serialize)]
pub struct VarianceReport {
    /// Output schema version.
    pub schema: u32,
    /// Baseline compared against.
    pub baseline: String,
    /// Baseline finish.
    pub baseline_finish: String,
    /// Current finish.
    pub finish: String,
    /// Finish variance in working days on the project calendar.
    pub finish_var: f64,
    /// Budget at completion.
    pub bac: f64,
    /// Current total cost.
    pub cost: f64,
    /// Variance at completion seen from the plan: current total minus BAC.
    pub cost_var: f64,
    /// Matching tasks (all, or only those that moved or changed cost with `changed_only`).
    pub total: usize,
    /// Tasks on this page.
    pub tasks: Vec<VarianceRow>,
    /// Tasks in the baseline that the plan no longer has.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub removed: Vec<String>,
    /// Cursor for the next page.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

/// `alma variance`: how the current schedule differs from a baseline.
pub fn variance(
    plan: &Plan,
    schedule: &Schedule,
    snap: &Snapshot,
    changed_only: bool,
    page: Page,
) -> VarianceReport {
    let cals = Calendars::new(plan);
    let day = plan.settings.units.minutes_per_day as f64;
    let days = |m: i64| (m as f64 / day * 100.0).round() / 100.0;
    let (rows, removed) = alma_sched::baseline::variance(plan, schedule, snap);
    let all: Vec<VarianceRow> = rows
        .iter()
        .map(|v| {
            let d = &schedule.tasks[v.task];
            let finish_of = |t| {
                if d.ef == d.es {
                    start_date(t).to_string()
                } else {
                    finish_date(t).to_string()
                }
            };
            VarianceRow {
                id: plan.tasks[v.task].id.clone(),
                baseline_start: v.baseline.map(|(s, _)| start_date(s).to_string()),
                baseline_finish: v.baseline.map(|(s, f)| {
                    if s == f {
                        start_date(f).to_string()
                    } else {
                        finish_date(f).to_string()
                    }
                }),
                start: start_date(d.es).to_string(),
                finish: finish_of(d.ef),
                start_var: days(v.start_var),
                finish_var: days(v.finish_var),
                baseline_cost: money(v.baseline_cost),
                cost: money(v.cost),
                cost_var: money(v.cost - v.baseline_cost),
                new: v.baseline.is_none(),
            }
        })
        .filter(|r| {
            !changed_only || r.new || r.start_var != 0.0 || r.finish_var != 0.0 || r.cost_var != 0.0
        })
        .collect();
    let total = all.len();
    let (tasks, next_cursor) = page.apply(&all);
    let cost = alma_sched::cost::costs(plan, schedule).total;
    let cal = cals.project();
    let fv = if schedule.finish >= snap.finish {
        cal.between(snap.finish, schedule.finish)
    } else {
        -cal.between(schedule.finish, snap.finish)
    };
    VarianceReport {
        schema: OUTPUT_SCHEMA,
        baseline: snap.baseline.clone(),
        baseline_finish: finish_date(snap.finish).to_string(),
        finish: finish_date(schedule.finish).to_string(),
        finish_var: days(fv),
        bac: money(snap.bac),
        cost: money(cost),
        cost_var: money(cost - snap.bac),
        total,
        tasks,
        removed,
        next_cursor,
    }
}

/// Planned value per bucket from a snapshot, for the S-curve.
pub fn pv_points(
    snap: &Snapshot,
    buckets: &[(jiff::civil::Date, jiff::civil::Date)],
) -> Vec<Decimal> {
    alma_sched::baseline::pv_curve(snap, buckets)
}
