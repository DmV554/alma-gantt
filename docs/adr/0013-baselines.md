# 0013 — Baselines: metadata in the plan, frozen snapshot in a sidecar, never overwritten

- **Status:** accepted
- **Date:** 2026-11-08

## Context
SPEC.md §5 (M2) asks for a baseline snapshot. `docs/format.md` D14 already decided where it
lives: metadata in the plan, frozen dates and costs in `.alma/baselines/`, so ADR 0003 (no
computed values in the plan) holds. The knowledge notes (resources §4.3, §6.1; scheduling §4.14)
add four rules:
- planned value (PV) and BAC come from a baseline, never from the current plan;
- a baseline changes only through a recorded change;
- history is never rewritten;
- one baseline is active.

## Decision
- **Plan file:** `[[baseline]]` with `id` (global namespace), `name`, `date`, `active` (at most
  one, else `E-FMT-03`), `parent` (the baseline it revises), `change_ref`, `reason` and
  `approved_by`. A revision without `change_ref` is `L-BAS-05` (info).
- **Snapshot:** `.alma/baselines/<plan stem>/<id>.toml`. The stem keeps two plans in one
  directory apart, which refines D14's path. It holds:
  - the project start and finish;
  - BAC;
  - each task's early start, early finish and cost (with descendants);
  - the time-phased budget as a daily cumulative curve (`pv`), on days where it changes.

  Instants are `YYYY-MM-DDTHH:MM` strings and money is a decimal string, so the file is exact
  and diffs well. Users commit the directory with the plan.
- **PV is read, not recomputed.** Planned value per period is the snapshot's cumulative value
  at the end of the period. Later edits to calendars, rates or the network cannot change past PV.
- **Saving** (`alma baseline save <id>`) makes the new baseline the only active one, and links
  it to the previous active baseline as `parent`. It refuses an existing id (`E-OPS-01`), so a
  baseline is never overwritten. The snapshot is written first and the plan second, both
  atomically; if the plan write fails, the snapshot is removed. `--dry-run` writes neither.
  The command is not a batch operation, because it writes a second file. It is still an MCP
  tool (`baseline_save`) and is logged in `.alma/log.jsonl`.
- **The date** defaults to today's date (UTC), read in `alma-ops` and the CLI, never in the
  engine. It is the one place where a command's output depends on the clock; `--date` makes it
  reproducible.
- **Reports:**
  - `alma baseline ls` lists each baseline's promised finish and BAC, and flags missing or
    corrupt snapshots.
  - `alma variance [--baseline] [--changed]` gives finish and cost variance for the project,
    and per task: start and finish variance in working days, cost variance, and new and
    removed tasks. It is paginated.
  - `alma scurve` adds a `pv` column from the active baseline (or `--baseline`).

  MCP tools: `baseline_list`, `variance`, and `scurve` with `baseline`.

## Consequences
Plans gain a reference to measure against, with PV that is stable by construction. Earned value
(M4) needs progress data (status date, actuals, percent complete) on top of this; the snapshot
format is versioned (`format = 0`) so it can grow. The notes' `alma baseline revise` keeps past
PV up to a status date and takes the current plan from there on. It needs the status date, so
it waits for M4. Until then a revision is a new full snapshot, linked by `parent` and
`change_ref`. Per-task PV curves (task-level SV) are not stored yet; adding them is a new
snapshot format version. Gantt baseline bars are an export feature for later.
