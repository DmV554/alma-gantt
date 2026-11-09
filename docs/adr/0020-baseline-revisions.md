# 0020 — Baseline revisions keep the past

- **Status:** accepted
- **Date:** 2026-11-09

## Context
ADR 0013 made baselines immutable: a change is a new baseline with a `parent` and a
`change_ref`. Once progress exists, though, a new baseline saved from the current plan also
rewrites the planned value of the periods already reported. Past SPI and SV change after the
fact. The knowledge notes (resources §6.1; scheduling §1.13, §4.14) settle this:
- baselines change only from the last baseline forward;
- past performance is never rewritten;
- `alma baseline revise` keeps the old time-phased values up to the status date and takes the
  current plan after it;
- lint rules `L-BAS-06` (history rewritten) and `L-BAS-09` (a revision above the change
  threshold without approval) enforce it.

## Decision
**`alma baseline revise ID [--from B] [--change CR] [--reason …] [--approved-by …]`** (MCP
`baseline_revise`) writes a new active baseline whose parent is B (default: the active one).
- **The new snapshot:** planned value through the status date is copied from the parent, for
  the project curve and each task's curve. After the status date it grows by what the current
  plan adds. So BAC = parent PV(status) + (current BAC − current PV(status)). The task and
  project dates are the current plan's: they are the new target.
- **Removed tasks:** a task the plan no longer has keeps the planned value it had through the
  status date, and nothing after it.
- **The snapshot file:** it records `revised_at` (the status date). `alma baseline ls` shows it.
- **Refusals:**
  - without a status date: `E-OPS-04`, with a hint to set one or save a fresh baseline;
  - with an unknown parent: `E-OPS-02`;
  - with a format 0 parent: `L-EVM-04`.

  Nothing is written in any of these cases.
- **`L-BAS-06` (error):** the active baseline's cumulative planned value differs from its
  parent's on a day before the revision. That day is `revised_at`, or the `date` of a baseline
  made with `save`. In practice this catches a `save` after progress. The hint points to
  `revise`.
- **`L-BAS-09` (warn):** the active baseline changes BAC by more than the rule parameter
  `change_threshold`, and its `[[baseline]]` has no `approved_by`. Without the parameter the
  rule is silent: only the plan knows its threshold.
- `lint_baseline` takes the parent snapshot, loaded by `alma_ops::load_file`.

`L-BAS-07` (actuals with no baseline) is already covered by `L-BAS-01`. `L-BAS-08` (management
reserve and baseline changes balance) waits for reserve drawdowns.

## Consequences
- Planned value reported at a status date is fixed, so SPI and SV from earlier reports stay
  true after a revision.
- `save` still works after progress, but `alma check` flags it with `L-BAS-06`. The choice is
  visible, not silent.
- Tests:
  - a hand-worked revision in `crates/alma-sched/tests/evm.rs` (the kept planned value, the new
    BAC, `L-BAS-06` for a plain save, `L-BAS-09` with and without approval);
  - CLI contract tests (refusals, dry run, `ls`, the same PV through the status date);
  - an MCP test.
